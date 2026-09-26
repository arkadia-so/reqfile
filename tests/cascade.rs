//! SCOPE: Reqfiles in any folder, inherited by subfolders, scoped to their
//! own folder; for each id, the nearest block applies.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, repo, reqfile};

/// A command check that fails listing the files it receives.
fn list_files(glob: &str) -> String {
    format!(
        r#"- command:
    run: "fail() {{ echo \"got $*\"; exit 1; }}; fail"
    files: "{glob}"
    pass_files: true
    fix_hint: None."#
    )
}

#[test]
fn a_requirement_targets_only_files_under_its_folder() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("ROOT_RULE", &list_files("**/*.txt"))]),
    );
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("APP_RULE", &list_files("**/*.txt"))]),
    );
    repo.write("top.txt", "")
        .write("app/inner.txt", "")
        .write("app/deep/nested.txt", "")
        .write("lib/other.txt", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout
            .contains("ROOT_RULE  got app/deep/nested.txt app/inner.txt lib/other.txt top.txt"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("APP_RULE  got deep/nested.txt inner.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_requirement_with_no_files_under_its_folder_does_not_run() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("ROOT_RULE", &list_files("**/*.txt"))]),
    );
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("APP_RULE", &list_files("**/*.txt"))]),
    );
    repo.write("top.txt", "").write("app/code.rs", "");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("ROOT_RULE  got top.txt"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("APP_RULE"), "{}", run.stdout);
    assert!(
        run.stdout.contains("1 check run, 1 with nothing to check."),
        "{}",
        run.stdout
    );
}

#[test]
fn commands_run_from_the_folder_of_their_reqfile() {
    let repo = repo!();
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[(
            "APP_RULE",
            "- command:\n    run: echo \"in $(basename \"$(pwd)\")\"; exit 1\n    fix_hint: None.",
        )]),
    );

    let run = repo.run(&["check"]);

    assert!(run.stdout.contains("APP_RULE  in app\n"), "{}", run.stdout);
}

#[test]
fn two_definitions_with_the_same_id_in_a_repository_are_an_error() {
    let repo = repo!();
    let checks = "- command:\n    run: \"true\"\n    fix_hint: None.";
    repo.write("Reqfile.yaml", &reqfile(&[("SAME_ID", checks)]));
    repo.write("app/Reqfile.yaml", &reqfile(&[("SAME_ID", checks)]));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "app/Reqfile.yaml:3: requirement SAME_ID is also defined in Reqfile.yaml; define it once and take it elsewhere with `use`"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn two_blocks_with_the_same_id_in_one_reqfile_are_an_error() {
    let repo = repo!();
    let checks = "- command:\n    run: \"true\"\n    fix_hint: None.";
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("SAME_ID", checks), ("SAME_ID", checks)]),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "Reqfile.yaml:11: requirement id SAME_ID appears twice in this Reqfile (first at line 3)"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn checks_run_from_any_folder_of_the_repository() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("ROOT_RULE", &list_files("**/*.txt"))]),
    );
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("APP_RULE", &list_files("**/*.txt"))]),
    );
    repo.write("top.txt", "").write("app/inner.txt", "");

    let run = repo.run_in("app", &["check"]);

    assert!(
        run.stdout.contains("ROOT_RULE  got app/inner.txt top.txt"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("APP_RULE  got inner.txt"),
        "{}",
        run.stdout
    );
}

#[test]
fn the_nearest_block_with_an_id_applies_and_shadows_farther_ones() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("LIST", &list_files("**/*.txt"))]),
    );
    // A nearer block with the same id, taking the root's definition.
    repo.write(
        "app/Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: LIST, use: .. }\n",
    );
    repo.write("top.txt", "")
        .write("lib/other.txt", "")
        .write("app/inner.txt", "")
        .write("app/deep/nested.txt", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("LIST  got lib/other.txt top.txt\n"),
        "the root block leaves app/ to the nearer block: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("LIST  got deep/nested.txt inner.txt\n"),
        "{}",
        run.stdout
    );
    let explain = repo.run(&["explain", "app/inner.txt"]);
    assert!(
        explain
            .stdout
            .contains("From app/Reqfile.yaml:\n  LIST (code)\n"),
        "{}",
        explain.stdout
    );
    assert!(
        !explain.stdout.contains("From Reqfile.yaml:"),
        "{}",
        explain.stdout
    );
}

#[test]
fn a_code_unit_is_judged_once_per_id_when_blocks_overlap() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: DECOMPLECT, use: ./std }\n",
    );
    repo.write(
        "std/Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision")]),
    );
    repo.write("std/.reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("src/a.py", "def a():\n    pass  # VIOLATION\n");
    repo.write("std/tools/b.py", "def b():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let received = jev.questions_received();
    let paths: Vec<&str> = received
        .iter()
        .map(|r| r.body["state"]["path"].as_str().expect("a path"))
        .collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert!(
        paths.contains(&"src/a.py") && paths.contains(&"std/tools/b.py"),
        "{paths:?}"
    );
    for request in &received {
        let questions = request.body["questions"].as_object().expect("questions");
        assert_eq!(questions.len(), 1, "one question per id: {questions:?}");
    }
    assert!(run.stdout.contains("2 advisory findings"), "{}", run.stdout);
}
