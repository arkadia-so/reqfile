//! NO_FALSE_GREEN: exit 0 only when every check ran and found no blocking violation.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Reply, repo, reqfile};

const PASS: &str = "- command:\n    run: \"true\"\n    fix_hint: x";
const VIOLATION: &str = "- command:\n    run: exit 1\n    fix_hint: x";
const CRASH: &str = "- command:\n    run: exit 2\n    fix_hint: x";

fn run_with(requirements: &[(&str, &str)]) -> reqfile_test_support::Run {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(requirements));
    repo.run(&["check"])
}

#[test]
fn every_check_passing_exits_0() {
    let run = run_with(&[("A", PASS), ("B", PASS)]);
    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn a_blocking_violation_exits_1() {
    let run = run_with(&[("A", PASS), ("B", VIOLATION)]);
    assert_eq!(run.code, 1, "{}", run.output());
}

#[test]
fn a_tool_error_exits_3_and_takes_precedence() {
    let run = run_with(&[("A", VIOLATION), ("B", CRASH)]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.ends_with(
            "2 checks run, 0 with nothing to check. 1 violation, 0 advisory findings, 1 error.\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn a_missing_tool_exits_3() {
    let run = run_with(&[(
        "A",
        "- command:\n    run: ruff-that-is-not-installed check\n    fix_hint: x",
    )]);
    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn a_timeout_exits_3() {
    let run = run_with(&[(
        "A",
        "- command:\n    run: sleep 5\n    timeout: 1\n    fix_hint: x",
    )]);
    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn unparseable_output_exits_3() {
    let run = run_with(&[(
        "A",
        "- command:\n    run: echo '<xml/>'; exit 1\n    format: sarif\n    fix_hint: x",
    )]);
    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn a_config_error_exits_3() {
    let run = run_with(&[("A", "- command:\n    fix_hint: x")]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.ends_with(
            "0 checks run, 0 with nothing to check. 0 violations, 0 advisory findings, 1 error.\n"
        ),
        "{}",
        run.stdout
    );
}

/// A repository with one decision check, `- decision` or blocking, whose
/// calls to Jev fail with 401.
fn failing_decision(check: &str) -> (FakeJev, reqfile_test_support::Repo) {
    let jev = FakeJev::start(|_| Reply::Status(401, "bad key".into()));
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", check)]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  model: typesafe/jev-1.13-20260917\n", jev.config()),
    );
    repo.write("a.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "wrong");
    (jev, repo)
}

#[test]
fn an_error_in_an_advisory_check_is_reported_without_changing_the_exit_code() {
    let (_jev, repo) = failing_decision("- decision");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("PASSING", PASS)]),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains(
            "advisory error  DECOMPLECT  1 of 1 units could not be judged, first at a.py:1: Jev returned HTTP 401: bad key"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .ends_with("0 violations, 0 advisory findings, 1 error (1 advisory).\n"),
        "{}",
        run.stdout
    );
    let json = repo.run(&["check", "--format", "json"]).json();
    assert_eq!(json["errors"][0]["blocking"], false);
    assert_eq!(json["summary"]["advisory_errors"], 1);
    assert_eq!(json["exit_code"], 0);
}

#[test]
fn an_error_in_a_blocking_check_exits_3() {
    let (_jev, repo) = failing_decision("- decision: { mode: blocking }");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("\nerror  DECOMPLECT  1 of 1 units")
            || run.stdout.starts_with("error  DECOMPLECT  1 of 1 units"),
        "{}",
        run.stdout
    );
    // A command check always blocks.
    let run = run_with(&[("A", CRASH)]);
    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn a_missing_api_key_with_only_advisory_decision_checks_exits_0_and_names_the_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/DEEP_MODULES/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    for id in ["DECOMPLECT", "DEEP_MODULES"] {
        assert!(
            run.stdout.contains(&format!(
                "advisory error  {id}  1 of 1 units could not be judged, first at a.py:1: OPENROUTER_API_KEY is not set"
            )),
            "{}",
            run.stdout
        );
    }
    assert!(jev.received().is_empty());
}

#[test]
fn advisory_findings_alone_exit_0() {
    let jev = FakeJev::start(|_| Reply::Probability(0.99));
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.ends_with(
            "1 check run, 0 with nothing to check, 1 code unit judged. 0 violations, 1 advisory finding, 0 errors.\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn outside_a_git_repository_exits_3() {
    let dir = tempfile_dir();
    let output = std::process::Command::new(reqfile_test_support::binary!())
        .arg("check")
        .current_dir(&dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().expect("a parent"))
        .output()
        .expect("run reqfile");
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stdout).contains("not inside a git repository"));
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("reqfile-no-git-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create a folder");
    dir
}

#[test]
fn the_summary_counts_checks_without_matching_files() {
    let run = run_with(&[
        ("A", PASS),
        (
            "B",
            "- command:\n    run: exit 1\n    files: \"**/*.go\"\n    fix_hint: x",
        ),
    ]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.ends_with(
            "1 check run, 1 with nothing to check. 0 violations, 0 advisory findings, 0 errors.\n"
        ),
        "{}",
        run.stdout
    );
}
