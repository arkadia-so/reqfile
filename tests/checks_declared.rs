//! CHECKS_DECLARED: every requirement has at least one check, a command or a decision.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, repo, reqfile};

fn assert_config_error(checks: &str, expected: &str) {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("RULE", checks)]));
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(expected),
        "expected {expected:?} in:\n{}",
        run.stdout
    );
}

#[test]
fn a_requirement_without_checks_fails() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("RULE", "")]).replace("    checks:\n", "    checks: []\n"),
    );
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("Reqfile.yaml:7: requirement RULE declares no checks; add at least one `command` or `decision` check"),
        "{}",
        run.stdout
    );

    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("RULE", "")]).replace("    checks:\n", ""),
    );
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("Reqfile.yaml:3: requirement RULE is missing the required field `checks`"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_check_must_be_a_command_or_a_decision() {
    assert_config_error(
        "- lint",
        "Reqfile.yaml:8: unknown check `lint`; a check is either `decision` or a `command` mapping",
    );
    assert_config_error(
        "- script:\n    run: x",
        "Reqfile.yaml:8: unknown check `script`; a check is either `decision` or a `command` mapping",
    );
    assert_config_error(
        "- 42",
        "Reqfile.yaml:8: a check is either `decision` or a `command` mapping",
    );
    assert_config_error(
        "- command:\n    run: \"true\"\n    fix_hint: x\n  decision:",
        "Reqfile.yaml:8: a check is either `decision` or a `command` mapping",
    );
}

#[test]
fn decision_modes_are_advisory_or_blocking() {
    assert_config_error(
        "- decision: { mode: strict }",
        "Reqfile.yaml:8: unknown decision mode `strict`; expected `advisory` or `blocking`",
    );
}

#[test]
fn a_requirement_has_at_most_one_decision_check() {
    assert_config_error(
        "- decision\n- decision",
        "Reqfile.yaml:9: requirement RULE declares more than one decision check",
    );
}

#[test]
fn command_and_decision_checks_are_accepted() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            (
                "WITH_COMMAND",
                "- command:\n    run: \"true\"\n    fix_hint: x",
            ),
            ("WITH_DECISION", "- decision"),
            ("WITH_BLOCKING_DECISION", "- decision: { mode: blocking }"),
            ("WITH_ADVISORY_DECISION", "- decision:\n    mode: advisory"),
        ]),
    );
    // A blocking decision check needs a pinned model.
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  model: typesafe/jev-1.13-20260917\n", jev.config()),
    );
    for id in [
        "WITH_DECISION",
        "WITH_BLOCKING_DECISION",
        "WITH_ADVISORY_DECISION",
    ] {
        repo.write(&format!(".reqfile/{id}/decision.yaml"), PYTHON_FUNCTIONS);
    }
    repo.write("a.py", "def f():\n    return 1\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(run.stdout.contains("4 checks run"), "{}", run.stdout);
}
