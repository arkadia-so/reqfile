//! CASCADE: Reqfiles in any folder, inherited by subfolders, scoped to their own folder.

use reqfile_test_support::{repo, reqfile};

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
fn duplicate_ids_across_reqfiles_name_both_files() {
    let repo = repo!();
    let checks = "- command:\n    run: \"true\"\n    fix_hint: None.";
    repo.write("Reqfile.yaml", &reqfile(&[("SAME_ID", checks)]));
    repo.write("app/Reqfile.yaml", &reqfile(&[("SAME_ID", checks)]));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "app/Reqfile.yaml:3: duplicate requirement id SAME_ID, also defined in Reqfile.yaml"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn duplicate_ids_in_one_reqfile_fail() {
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
            "Reqfile.yaml:11: duplicate requirement id SAME_ID, also defined in Reqfile.yaml"
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
