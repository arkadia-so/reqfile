//! OUTPUT: every finding carries the requirement, a message, the fix hint and its location,
//! and names the other requirements its fix would break.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Reply, Repo, remotes, repo, reqfile};

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
            "advisory_errors": 0,
            "decisions": 0,
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

#[test]
fn a_finding_of_an_imported_requirement_names_its_block_and_resolved_source() {
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            "- command:\n    run: grep -n TODO notes.txt && exit 1 || exit 0\n    fix_hint: Remove the TODO.",
        )]),
    );
    source.commit("define NO_TODO");
    let commit = source.head();
    let repo = repo!();
    remotes.serve(&repo);
    repo.write(
        "app/Reqfile.yaml",
        &format!("reqfile: 1\ncode:\n  - {{ id: NO_TODO, use: acme/reqs@{commit} }}  # v1\n"),
    );
    repo.write("app/notes.txt", "TODO: finish\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains(&format!(
            "  fix: Remove the TODO.\n  from: app/Reqfile.yaml:3, use acme/reqs@{commit} (Reqfile.yaml:3 at commit {commit})\n"
        )),
        "{}",
        run.stdout
    );
    let json = repo.run(&["check", "--format", "json"]).json();
    let finding = &json["findings"][0];
    assert_eq!(finding["block"], "app/Reqfile.yaml:3");
    assert_eq!(
        finding["source"],
        format!("acme/reqs@{commit} (Reqfile.yaml:3 at commit {commit})")
    );
    // A commit pin says what it is: no source line to report.
    assert!(json.get("sources").is_none(), "{json}");
}

/// A SARIF check flagging `files`, a requirement its fix breaks (its must
/// holds `CORE_MARK`) and one it does not; Jev answers by question id.
fn conflict_repo(jev: &FakeJev, files: &[&str]) -> Repo {
    let repo = repo!();
    let results: Vec<String> = files
        .iter()
        .map(|f| format!(r#"{{"ruleId": "C", "message": {{"text": "{f} has one user elsewhere"}}, "locations": [{{"physicalLocation": {{"artifactLocation": {{"uri": "{f}"}}, "region": {{"startLine": 1}}}}}}]}}"#))
        .collect();
    repo.write(
        "result.sarif",
        &format!(r#"{{"runs": [{{"results": [{}]}}]}}"#, results.join(", ")),
    );
    let block = |id: &str, must: &str, run: &str, format: &str, fix: &str| {
        format!(
            "  - id: {id}\n    must: {must}\n    why: A reason.\n    checks:\n      - command:\n          run: {run}\n{format}          fix_hint: {fix}\n"
        )
    };
    repo.write(
        "Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n{}{}{}",
            block(
                "COLOCATION",
                "Code lives next to its users.",
                "cat result.sarif; exit 1",
                "          format: sarif\n",
                "Move the file next to its only user.",
            ),
            block(
                "CORE",
                "State machines live under core/. CORE_MARK",
                "\"true\"",
                "",
                "None."
            ),
            block(
                "DOCS",
                "Every module has a doc comment.",
                "\"true\"",
                "",
                "None."
            ),
        ),
    );
    for file in files {
        repo.write(file, "pub fn step() {}\n");
    }
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.env("OPENROUTER_API_KEY", "key");
    repo
}

fn conflict_jev() -> FakeJev {
    FakeJev::start(|_| Reply::ByQuestion(vec![("CORE", 0.9), ("DOCS", 0.1)]))
}

#[test]
fn finding_whose_fix_would_break_another_requirement_names_it() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/cache.rs"]);

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains(
            "COLOCATION  core/cache.rs:1  core/cache.rs has one user elsewhere (C)\n  fix: Move the file next to its only user.\n  breaks: CORE (p=0.90)\n"
        ),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("DOCS (p="), "{}", run.stdout);
    let asked = jev.conflicts_received();
    assert_eq!(asked.len(), 1, "one request per finding");
    let questions = asked[0].body["questions"].as_object().expect("questions");
    assert_eq!(questions.len(), 2, "every other requirement that applies");
    assert!(
        questions["CORE"]["instructions"]["question"]
            .as_str()
            .is_some_and(|q| q.contains("CORE_MARK")),
        "{questions:?}"
    );
    assert_eq!(
        asked[0].body["state"]["finding"]["requirement"],
        "COLOCATION"
    );
    assert_eq!(asked[0].body["state"]["source"], "pub fn step() {}\n");
}

#[test]
fn conflicts_are_grouped_by_requirement_pair_as_decisions_to_make() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/a.rs", "core/b.rs"]);

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains(
            "decide  COLOCATION vs CORE  p=0.90  fixing COLOCATION in 2 files would break CORE\n    core/a.rs, core/b.rs\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .ends_with("1 decision to make between requirements that cannot both hold.\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn conflicting_findings_are_marked_in_json_so_hooks_route_them_to_a_human() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/cache.rs"]);

    let run = repo.run(&["check", "--format", "json"]);

    // A conflict informs; the exit code still says the check failed.
    assert_eq!(run.code, 1, "{}", run.output());
    let json = run.json();
    assert_eq!(
        json["findings"][0]["conflicts"],
        serde_json::json!([{ "requirement": "CORE", "probability": 0.9 }])
    );
    assert_eq!(json["summary"]["decisions"], 1);
}

#[test]
fn conflict_answers_are_cached_like_decision_answers() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/cache.rs"]);

    let first = repo.run(&["check"]);
    let second = repo.run(&["check"]);

    for run in [&first, &second] {
        assert!(
            run.stdout.contains("breaks: CORE (p=0.90)"),
            "{}",
            run.stdout
        );
    }
    assert_eq!(jev.conflicts_received().len(), 1, "then cached");
}

#[test]
fn conflicts_are_not_checked_without_a_jev_key_and_the_summary_says_so() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/cache.rs"]);
    repo.env("OPENROUTER_API_KEY", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.ends_with(
            "Conflicts between requirements not checked: OPENROUTER_API_KEY is not set.\n"
        ),
        "{}",
        run.stdout
    );
    assert!(jev.received().is_empty());
}

#[test]
fn a_finding_no_other_requirement_applies_to_asks_nothing() {
    let jev = conflict_jev();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("NO_TODO", "- command:\n    run: cat result.sarif; exit 1\n    format: sarif\n    fix_hint: Remove it.")]));
    repo.write("result.sarif", SARIF);
    repo.write(".reqfile/config.yaml", &jev.config());

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(jev.received().is_empty());
    assert!(!run.stdout.contains("Conflicts between"), "{}", run.stdout);
}

#[test]
fn conflict_answers_are_recorded_in_the_run_log() {
    let jev = conflict_jev();
    let repo = conflict_repo(&jev, &["core/cache.rs"]);

    repo.run(&["check", "--log", "run.jsonl"]);

    let log = std::fs::read_to_string(repo.path().join("run.jsonl")).expect("the log");
    let conflicts: Vec<serde_json::Value> = log
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("JSON lines"))
        .filter(|l| l["kind"] == "conflict")
        .collect();
    assert_eq!(conflicts.len(), 2, "{log}");
    let core = conflicts
        .iter()
        .find(|l| l["breaks"] == "CORE")
        .expect("CORE");
    assert_eq!(core["requirement"], "COLOCATION");
    assert_eq!(core["file"], "core/cache.rs");
    assert_eq!(core["conflict"], true);
}

#[test]
fn pretty_output_is_a_table_then_the_details_of_what_does_not_pass() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let run = repo.run(&["check", "--format", "pretty"]);

    assert_eq!(run.code, 3, "{}", run.output());
    let out = &run.stdout;
    assert!(
        out.starts_with("reqfile check · 4 requirements · 4 checks\n\n"),
        "{out}"
    );
    // One row per requirement, most informative status first in each row.
    assert!(
        out.contains("  ✗ FAIL_FAST  1 violation · 1 advisory · 1 uncertain · 2 units judged"),
        "{out}"
    );
    assert!(out.contains("  ✗ NO_TODO    1 violation"), "{out}");
    assert!(
        out.contains("  ! BROKEN     error · `exit 7` failed with exit code 7"),
        "{out}"
    );
    assert!(out.contains("  – QUIET      nothing to check"), "{out}");
    // Details group findings by requirement, each fix hint once, no color in a pipe.
    assert!(out.contains("\n✗ FAIL_FAST  "), "{out}");
    assert!(
        out.contains("  ✗ api/handler.py:42  Do not use bare `except` (E722)\n"),
        "{out}"
    );
    assert!(
        out.contains("\n  fix  Catch the specific error.\n  fix  Split the function.\n"),
        "{out}"
    );
    assert!(out.contains("  ✗ notes.md has a TODO\n"), "{out}");
    assert!(!out.contains('\x1b'), "{out}");
    assert!(
        out.contains("✗ 2 violations   ● 1 advisory   ? 1 uncertain   ! 1 error"),
        "{out}"
    );
}

#[test]
fn quiet_output_keeps_only_what_fails_and_the_summary() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let run = repo.run(&["check", "--format", "pretty", "-q"]);

    assert!(!run.stdout.contains("reqfile check ·"), "{}", run.stdout);
    assert!(run.stdout.contains("✗ NO_TODO"), "{}", run.stdout);
    assert!(run.stdout.contains("✗ 2 violations"), "{}", run.stdout);
}

#[test]
fn verbose_output_lists_what_each_check_ran() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let run = repo.run(&["check", "--format", "pretty", "-v"]);

    assert!(run.stdout.contains("\nChecks\n"), "{}", run.stdout);
    assert!(
        run.stdout
            .contains("FAIL_FAST  command  cat result.sarif; exit 1"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("FAIL_FAST  decision · 2 units (0 cached)"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("QUIET      nothing to check"),
        "{}",
        run.stdout
    );
}

#[test]
fn outside_a_terminal_the_default_output_stays_plain_for_agents() {
    let jev = FakeJev::by_marker();
    let repo = mixed_repo(&jev);

    let default = repo.run(&["check"]);
    let plain = repo.run(&["check", "--format", "plain"]);
    let legacy = repo.run(&["check", "--format", "summary"]);

    assert_eq!(default.stdout, plain.stdout);
    assert_eq!(legacy.stdout, plain.stdout);
    assert!(
        plain.stdout.starts_with("FAIL_FAST  api/handler.py:42"),
        "{}",
        plain.stdout
    );
}

/// A decision check on one Python function, run without its Jev key.
fn keyless_repo(jev: &FakeJev) -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            ("FAIL_FAST", "- decision"),
            ("NO_TODO", "- command:\n    run: \"true\"\n    fix_hint: x"),
        ]),
    );
    repo.write(".reqfile/FAIL_FAST/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass\n");
    repo
}

#[test]
fn a_missing_jev_key_is_announced_first_and_once() {
    let jev = FakeJev::by_marker();
    let repo = keyless_repo(&jev);

    let plain = repo.run(&["check"]);
    let pretty = repo.run(&["check", "--format", "pretty"]);
    let json = repo.run(&["check", "--format", "json"]).json();

    // Advisory decision checks: the run does not fail, it says why they did not judge.
    assert_eq!(plain.code, 0, "{}", plain.output());
    assert!(
        plain.stdout.starts_with("setup  OPENROUTER_API_KEY is not set: the decision checks of FAIL_FAST will not judge any code"),
        "{}",
        plain.stdout
    );
    let banner = pretty
        .stdout
        .find("! Jev is not set up: OPENROUTER_API_KEY is not set")
        .expect(&pretty.stdout);
    let table = pretty.stdout.find("FAIL_FAST").expect(&pretty.stdout);
    assert!(banner < table, "{}", pretty.stdout);
    assert!(
        !pretty.stdout.contains("\n! FAIL_FAST  "),
        "no detail section repeats it: {}",
        pretty.stdout
    );
    assert!(
        !pretty
            .stdout
            .contains("Conflicts between requirements not checked"),
        "{}",
        pretty.stdout
    );
    assert_eq!(json["setup"][0]["variable"], "OPENROUTER_API_KEY");
    assert_eq!(
        json["setup"][0]["requirements"],
        serde_json::json!(["FAIL_FAST"])
    );
    assert_eq!(json["setup"][0]["blocking"], false);

    repo.env("OPENROUTER_API_KEY", "key");
    let keyed = repo.run(&["check"]);
    assert!(!keyed.stdout.contains("setup  "), "{}", keyed.stdout);
}
