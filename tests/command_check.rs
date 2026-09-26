//! COMMAND_CHECK: shell commands, their files, exit codes, SARIF output and timeouts.

use std::time::{Duration, Instant};

use reqfile_test_support::{repo, reqfile};

fn check_with(command: &str) -> reqfile_test_support::Run {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "RULE",
            &format!("- command:\n{command}\n    fix_hint: Fix it."),
        )]),
    );
    repo.write("a.txt", "")
        .write("src/b.txt", "")
        .write("src/c.rs", "");
    repo.run(&["check"])
}

#[test]
fn exit_zero_passes() {
    let run = check_with("    run: \"true\"");
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("1 check run, 0 with nothing to check. 0 violations"),
        "{}",
        run.stdout
    );
}

#[test]
fn exit_one_is_a_violation_carrying_the_output() {
    let run = check_with("    run: echo out; echo err >&2; exit 1");
    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("RULE  out\n    err\n  fix: Fix it."),
        "{}",
        run.stdout
    );
}

#[test]
fn a_violation_carries_the_last_50_lines_of_output() {
    let run = check_with("    run: \"for i in $(seq 1 60); do echo line$i; done; exit 1\"");
    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  line11\n"), "{}", run.stdout);
    assert!(run.stdout.contains("    line60\n"), "{}", run.stdout);
    assert!(!run.stdout.contains("line10\n"), "{}", run.stdout);
}

#[test]
fn violation_codes_replace_the_default() {
    let run = check_with("    run: exit 101\n    violation_codes: [101]");
    assert_eq!(run.code, 1, "{}", run.output());

    let run = check_with("    run: exit 1\n    violation_codes: [101]");
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("error  RULE  `exit 1` failed with exit code 1"),
        "{}",
        run.stdout
    );
}

#[test]
fn other_exit_codes_are_tool_errors() {
    let run = check_with("    run: echo crashed; exit 2");
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("error  RULE  `echo crashed; exit 2` failed with exit code 2\n    crashed"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_missing_tool_is_a_tool_error() {
    let run = check_with("    run: definitely-not-an-installed-tool --check");
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("failed with exit code 127"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_signal_is_a_tool_error() {
    let run = check_with("    run: kill -9 $$");
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("error  RULE  `kill -9 $$` was killed by signal 9"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_timeout_is_a_tool_error_and_stops_the_command() {
    let started = Instant::now();
    let run = check_with("    run: sleep 30\n    timeout: 1");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("error  RULE  `sleep 30` timed out after 1 seconds"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_timeout_also_stops_what_the_command_started() {
    let started = Instant::now();
    let run = check_with("    run: \"sh -c 'sleep 30' & sleep 30\"\n    timeout: 1");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn files_restrict_when_the_command_runs() {
    let run = check_with("    run: exit 1\n    files: \"**/*.py\"");
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("0 checks run, 1 with nothing to check."),
        "{}",
        run.stdout
    );

    let run = check_with("    run: echo args $#; exit 1\n    files: \"**/*.txt\"");
    assert!(run.stdout.contains("RULE  args 0\n"), "{}", run.stdout);
}

#[test]
fn pass_files_appends_the_matching_files() {
    let run = check_with(
        "    run: \"fail() { echo \\\"got $*\\\"; exit 1; }; fail\"\n    files: \"**/*.txt\"\n    pass_files: true",
    );
    assert!(
        run.stdout.contains("RULE  got a.txt src/b.txt\n"),
        "{}",
        run.stdout
    );

    let run = check_with(
        "    run: \"fail() { echo \\\"got $*\\\"; exit 1; }; fail\"\n    files: \"*.txt\"\n    pass_files: true",
    );
    assert!(run.stdout.contains("RULE  got a.txt\n"), "{}", run.stdout);
}

#[test]
fn file_names_with_spaces_are_passed_intact() {
    let repo = repo!();
    let command = "- command:\n    run: \"fail() { for f in \\\"$@\\\"; do echo \\\"[$f]\\\"; done; exit 1; }; fail\"\n    files: \"**/*.txt\"\n    pass_files: true\n    fix_hint: Fix it.";
    repo.write("Reqfile.yaml", &reqfile(&[("RULE", command)]));
    repo.write("my notes.txt", "");
    let run = repo.run(&["check"]);
    assert!(
        run.stdout.contains("RULE  [my notes.txt]\n"),
        "{}",
        run.stdout
    );
}

const SARIF: &str = r#"{
  "version": "2.1.0",
  "runs": [{
    "tool": { "driver": { "name": "fake" } },
    "results": [
      {
        "ruleId": "E722",
        "message": { "text": "Do not use bare `except`" },
        "locations": [{ "physicalLocation": { "artifactLocation": { "uri": "src/handler.py" }, "region": { "startLine": 42 } } }]
      },
      {
        "ruleId": "X1",
        "message": { "text": "Absolute location" },
        "locations": [{ "physicalLocation": { "artifactLocation": { "uri": "file://ROOT/app/other%20file.py" }, "region": { "startLine": 7 } } }]
      },
      { "message": { "text": "No location" } }
    ]
  }]
}"#;

#[test]
fn sarif_output_gives_one_violation_per_result_with_its_location() {
    let repo = repo!();
    let root = repo.path().canonicalize().expect("canonical root");
    repo.write(
        "app/result.sarif",
        &SARIF.replace("ROOT", &root.to_string_lossy()),
    );
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("RULE", "- command:\n    run: cat result.sarif; exit 1\n    format: sarif\n    fix_hint: Fix it.")]),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains(
            "RULE  app/src/handler.py:42  Do not use bare `except` (E722)\n  fix: Fix it."
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("RULE  app/other file.py:7  Absolute location (X1)\n"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("RULE  No location\n"), "{}", run.stdout);
    assert!(run.stdout.contains("3 violations"), "{}", run.stdout);
}

#[test]
fn unparseable_sarif_is_a_tool_error() {
    let run = check_with("    run: echo not json; exit 1\n    format: sarif");
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("output is not valid SARIF"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_violation_code_with_no_sarif_results_is_a_tool_error() {
    let run = check_with(
        "    run: \"echo '{\\\"runs\\\": [{\\\"results\\\": []}]}'; exit 1\"\n    format: sarif",
    );
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("its SARIF output lists no results"),
        "{}",
        run.stdout
    );
}

#[test]
fn sarif_commands_keep_stderr_out_of_the_parsed_output() {
    let run = check_with(
        "    run: \"echo warning >&2; echo '{\\\"runs\\\": [{\\\"results\\\": [{\\\"message\\\": {\\\"text\\\": \\\"found\\\"}}]}]}'; exit 1\"\n    format: sarif",
    );
    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  found\n"), "{}", run.stdout);
}

#[test]
fn invalid_command_fields_are_config_errors() {
    for (fields, expected) in [
        (
            "    run: x\n    pass_files: true",
            "`pass_files` needs a `files` glob",
        ),
        (
            "    run: x\n    format: json",
            "unknown format `json`; expected `exit` or `sarif`",
        ),
        (
            "    run: x\n    violation_codes: [0]",
            "`violation_codes` must list exit codes between 1 and 255",
        ),
        (
            "    run: x\n    timeout: 0",
            "`timeout` must be a positive number of seconds",
        ),
        ("    run: x\n    files: \"a/[\"", "invalid `files` glob"),
    ] {
        let run = check_with(fields);
        assert_eq!(run.code, 3, "{fields}: {}", run.output());
        assert!(run.stdout.contains(expected), "{fields}: {}", run.stdout);
    }
}

#[test]
fn a_background_process_holding_the_output_cannot_outlive_the_timeout() {
    let started = Instant::now();
    let run = check_with("    run: sleep 30 &\n    timeout: 1");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("timed out after 1 seconds"),
        "{}",
        run.stdout
    );
}
