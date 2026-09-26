//! EXAMPLES: `reqfile test` runs each requirement's checks on its labeled examples.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, repo, reqfile};

/// Flags any .txt file containing TODO, through a script of the requirement.
const NO_TODO: &str = r#"- command:
    run: .reqfile/NO_TODO/check.sh
    files: "**/*.txt"
    pass_files: true
    fix_hint: Remove the TODO."#;
const SCRIPT: &str = "#!/bin/sh\nif grep -l TODO \"$@\"; then exit 1; fi\n";

fn no_todo_repo() -> reqfile_test_support::Repo {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("NO_TODO", NO_TODO)]));
    repo.write(".reqfile/NO_TODO/check.sh", SCRIPT);
    std::process::Command::new("chmod")
        .args(["+x", ".reqfile/NO_TODO/check.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    repo
}

#[test]
fn examples_of_a_command_check_must_match_their_labels() {
    let repo = no_todo_repo();
    repo.write(
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO: finish\n",
    );
    repo.write(
        ".reqfile/NO_TODO/examples/violation-nested/docs/plan.txt",
        "a TODO here\n",
    );
    repo.write(".reqfile/NO_TODO/examples/ok-done/notes.txt", "done\n");

    let run = repo.run(&["test"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "NO_TODO  3 examples, 3 as labeled\n\n1 requirements with examples, 3 examples. 0 failing their labels.\n"
    );
}

#[test]
fn a_command_check_disagreeing_with_a_label_fails() {
    let repo = no_todo_repo();
    repo.write(
        ".reqfile/NO_TODO/examples/violation-lowercase/notes.txt",
        "todo: finish\n",
    );
    repo.write(
        ".reqfile/NO_TODO/examples/ok-mentions-todo/notes.txt",
        "Never write TODO.\n",
    );
    repo.write(
        ".reqfile/NO_TODO/examples/violation-no-text-file/main.rs",
        "// TODO\n",
    );

    let run = repo.run(&["test"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("NO_TODO  3 examples, 0 as labeled\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  violation-lowercase: missed\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  ok-mentions-todo: false alarm\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("  violation-no-text-file: not selected\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn decision_checks_are_measured_rather_than_asserted() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    let examples = ".reqfile/DECOMPLECT/examples";
    repo.write(
        &format!("{examples}/violation-caught/a.py"),
        "def f():\n    pass  # VIOLATION\n",
    );
    repo.write(
        &format!("{examples}/violation-missed/a.py"),
        "def f():\n    pass\n",
    );
    repo.write(
        &format!("{examples}/violation-unsure/a.py"),
        "def f():\n    pass  # UNSURE\n",
    );
    repo.write(
        &format!("{examples}/violation-not-python/a.rs"),
        "fn f() {}\n",
    );
    repo.write(&format!("{examples}/ok-clean/a.py"), "def f():\n    pass\n");
    repo.write(
        &format!("{examples}/ok-flagged/a.py"),
        "def f():\n    pass  # VIOLATION\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["test"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains(
            "DECOMPLECT  6 examples, measured: 1 of 4 violations caught (1 missed, 1 not selected, 1 uncertain); 1 of 2 correct examples flagged (0 uncertain)\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  ok-flagged: false alarm\n"),
        "{}",
        run.stdout
    );
    assert_eq!(
        jev.questions_received().len(),
        5,
        "every selected example unit is asked"
    );
}

#[test]
fn a_case_that_cannot_run_is_an_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(
        ".reqfile/DECOMPLECT/examples/violation-x/a.py",
        "def f():\n    pass\n",
    );

    let run = repo.run(&["test"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("  violation-x: error: 1 of 1 units could not be judged, first at a.py:1: OPENROUTER_API_KEY is not set"),
        "{}",
        run.stdout
    );
}

#[test]
fn example_folders_must_be_labeled() {
    let repo = no_todo_repo();
    repo.write(".reqfile/NO_TODO/examples/maybe-todo/notes.txt", "TODO\n");

    let run = repo.run(&["test"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr
            .contains("an example folder must be named violation-… or ok-…"),
        "{}",
        run.stderr
    );
}

#[test]
fn examples_are_never_checked_as_code() {
    let repo = no_todo_repo();
    repo.write(
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );
    repo.write("clean.txt", "fine\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn only_tests_the_listed_requirements() {
    let repo = no_todo_repo();
    repo.write(
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let run = repo.run(&["test", "--only", "OTHER"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr
            .contains("--only: no requirement has the id OTHER"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_violation_a_command_examined_is_missed_even_if_no_unit_was_selected() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    let checks =
        "- command:\n    run: \"true\"\n    files: \"**/*.rs\"\n    fix_hint: x\n- decision";
    repo.write("Reqfile.yaml", &reqfile(&[("MIXED", checks)]));
    repo.write(".reqfile/MIXED/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(".reqfile/MIXED/examples/violation-rust/a.rs", "fn f() {}\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["test"]);

    assert!(
        run.stdout.contains("  violation-rust: missed\n"),
        "{}",
        run.stdout
    );
}
