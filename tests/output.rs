//! OUTPUT: every finding carries the requirement, a message, the fix hint and its location.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Reply, Repo, repo, reqfile};

const SARIF: &str = r#"{"runs": [{"results": [{"ruleId": "E722", "message": {"text": "Do not use bare `except`"}, "locations": [{"physicalLocation": {"artifactLocation": {"uri": "api/handler.py"}, "region": {"startLine": 42}}}]}]}]}"#;

/// A repository with a SARIF violation, an exit-format violation, an
/// advisory and an uncertain decision finding, and a tool error.
fn mixed_repo(jev: &FakeJev) -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            (
                "FAIL_FAST",
                "- command:\n    run: cat result.sarif; exit 1\n    format: sarif\n    fix_hint: Catch the specific error.\n- decision",
            ),
            ("NO_TODO", "- command:\n    run: echo 'notes.md has a TODO'; exit 1\n    fix_hint: Remove the TODO."),
            ("BROKEN", "- command:\n    run: exit 7\n    fix_hint: Never shown."),
            ("QUIET", "- command:\n    run: exit 1\n    files: \"**/*.go\"\n    fix_hint: Never shown."),
        ]),
    );
    repo.write("result.sarif", SARIF);
    repo.write(".reqfile/FAIL_FAST/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(
        "api/worker.py",
        "def a():\n    pass  # VIOLATION\n\n\ndef b():\n    pass  # UNSURE\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");
    repo
}

#[test]
fn the_summary_lists_findings_errors_and_counts() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert_eq!(
        run.stdout,
        "FAIL_FAST  api/handler.py:42  Do not use bare `except` (E722)\n  fix: Catch the specific error.\n\
         NO_TODO  notes.md has a TODO\n  fix: Remove the TODO.\n\
         advisory  FAIL_FAST  api/worker.py:1  p=0.95  It mixes several jobs.\n  fix: Split the function.\n\
         uncertain  FAIL_FAST  api/worker.py:5  p=0.50  Does this function do more than one job?\n  fix: Split the function.\n\
         error  BROKEN  `exit 7` failed with exit code 7\n    (no output)\n\
         \n\
         4 checks run, 1 with nothing to check, 2 code units judged. 2 violations, 2 advisory findings, 1 error.\n"
    );
}

#[test]
fn json_output_has_the_same_content() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let run = repo.run(&["check", "--format", "json"]);

    assert_eq!(run.code, 3, "{}", run.output());
    let json = run.json();
    let findings = json["findings"].as_array().expect("findings");
    assert_eq!(findings.len(), 4);
    assert_eq!(
        findings[0],
        serde_json::json!({
            "requirement": "FAIL_FAST",
            "kind": "violation",
            "file": "api/handler.py",
            "line": 42,
            "message": "Do not use bare `except` (E722)",
            "fix_hint": "Catch the specific error.",
        })
    );
    assert_eq!(findings[1]["file"], serde_json::Value::Null);
    assert_eq!(findings[1]["message"], "notes.md has a TODO");
    assert_eq!(findings[2]["kind"], "advisory");
    assert_eq!(findings[2]["probability"], 0.95);
    assert_eq!(findings[2]["file"], "api/worker.py");
    assert_eq!(findings[2]["line"], 1);
    assert_eq!(findings[3]["kind"], "uncertain");
    assert_eq!(json["errors"][0]["requirement"], "BROKEN");
    assert_eq!(
        json["summary"],
        serde_json::json!({
            "checks_run": 4,
            "checks_with_nothing_to_check": 1,
            "checks_left_for_full_run": 0,
            "units_judged": 2,
            "violations": 2,
            "advisory_findings": 2,
            "errors": 1,
        })
    );
    assert_eq!(json["exit_code"], 3);
}

#[test]
fn json_output_reports_config_errors_too() {
    let repo = repo!();
    repo.write("Reqfile.yaml", "reqfile: 1\ncode: 3\n");

    let run = repo.run(&["check", "--format", "json"]);

    assert_eq!(run.code, 3, "{}", run.output());
    let json = run.json();
    assert_eq!(
        json["errors"][0]["message"],
        "Reqfile.yaml:2: `code` must be a list, found an integer"
    );
    assert_eq!(json["errors"][0]["requirement"], serde_json::Value::Null);
    assert_eq!(json["summary"]["errors"], 1);
}

#[test]
fn a_clean_run_prints_only_the_summary() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("CLEAN", "- command:\n    run: \"true\"\n    fix_hint: x")]),
    );

    let run = repo.run(&["check"]);

    assert_eq!(
        run.stdout,
        "1 check run, 0 with nothing to check. 0 violations, 0 advisory findings, 0 errors.\n"
    );
}

#[test]
fn a_run_that_judged_no_code_says_so() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("constants.py", "LIMIT = 3\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "0 checks run, 1 with nothing to check, 0 code units judged. 0 violations, 0 advisory findings, 0 errors.\n"
    );
    assert!(jev.questions_received().is_empty());
}

#[test]
fn a_reader_that_stops_early_is_not_a_crash() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "FAILS",
            "- command:\n    run: echo found; exit 1\n    fix_hint: x",
        )]),
    );
    let script = format!(
        "'{}' check | true; echo \"status=${{PIPESTATUS[0]}}\"",
        reqfile_test_support::binary!()
    );

    let output = std::process::Command::new("bash")
        .args(["-c", &script])
        .current_dir(repo.path())
        .output()
        .expect("run bash");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("status=1"), "{stdout}{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn decision_findings_record_the_exact_model_that_judged_them() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let first = repo.run(&["check", "--format", "json"]).json();
    let cached = repo.run(&["check", "--format", "json"]).json();

    for json in [first, cached] {
        assert_eq!(json["findings"][0]["model"], "typesafe/jev-1.13-20260917");
    }
}

#[test]
fn a_finding_names_the_model_that_answered_it() {
    let jev = FakeJev::start(|body| {
        if body["questions"].get("probe").is_some() {
            Reply::Probability(0.0)
        } else {
            Reply::Status(
                200,
                r#"{"model": "typesafe/jev-1.14-20261101", "answers": {"DECOMPLECT": {"type": "noul", "noul": 0.95}}}"#.into(),
            )
        }
    });
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let json = repo.run(&["check", "--format", "json"]).json();

    assert_eq!(json["findings"][0]["model"], "typesafe/jev-1.14-20261101");
}
