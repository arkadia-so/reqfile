//! EXPLAIN: `reqfile explain <path>` lists the requirements that apply, with their Reqfile.

use reqfile_test_support::{Repo, repo};

fn requirement(id: &str, must: &str) -> String {
    format!(
        "  - id: {id}\n    must: {must}\n    why: Because of {id}.\n    who: Someone.\n    checks:\n      - command:\n          run: \"true\"\n          fix_hint: x\n"
    )
}

fn cascade_repo() -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n{}",
            requirement("FAIL_FAST", "No swallowed errors.")
        ),
    );
    repo.write(
        "services/api/Reqfile.yaml",
        &format!(
            "reqfile: 1\nproduct:\n{}",
            requirement("FAST_API", "Answers in 100 ms.")
        ),
    );
    repo.write(
        "services/web/Reqfile.yaml",
        &format!(
            "reqfile: 1\nproduct:\n{}",
            requirement("PRETTY", "Looks good.")
        ),
    );
    repo.write("services/api/handler.py", "");
    repo
}

#[test]
fn a_file_gets_the_requirements_of_its_folder_and_ancestors() {
    let repo = cascade_repo();

    let run = repo.run(&["explain", "services/api/handler.py"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "Requirements that apply to services/api/handler.py:\n\
         \nFrom Reqfile.yaml:\n  FAIL_FAST (code)\n    must: No swallowed errors.\n    why: Because of FAIL_FAST.\n\
         \nFrom services/api/Reqfile.yaml:\n  FAST_API (product)\n    must: Answers in 100 ms.\n    why: Because of FAST_API.\n"
    );
}

#[test]
fn a_folder_gets_its_own_requirements() {
    let repo = cascade_repo();

    let run = repo.run(&["explain", "services/web"]);

    assert!(
        run.stdout
            .contains("From services/web/Reqfile.yaml:\n  PRETTY (product)"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("FAIL_FAST"), "{}", run.stdout);
    assert!(!run.stdout.contains("FAST_API"), "{}", run.stdout);
}

#[test]
fn a_parent_folder_does_not_get_requirements_of_its_children() {
    let repo = cascade_repo();

    let run = repo.run(&["explain", "services"]);

    assert!(run.stdout.contains("FAIL_FAST"), "{}", run.stdout);
    assert!(!run.stdout.contains("FAST_API"), "{}", run.stdout);
    assert!(!run.stdout.contains("PRETTY"), "{}", run.stdout);
}

#[test]
fn paths_are_relative_to_the_current_folder_and_may_not_exist_yet() {
    let repo = cascade_repo();

    let run = repo.run_in("services/api", &["explain", "new/module.py"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .starts_with("Requirements that apply to services/api/new/module.py:"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("FAST_API"), "{}", run.stdout);

    let run = repo.run_in("services/api", &["explain", "."]);
    assert!(run.stdout.contains("FAST_API"), "{}", run.stdout);

    let run = repo.run_in("services/api", &["explain", "../web"]);
    assert!(run.stdout.contains("PRETTY"), "{}", run.stdout);
}

#[test]
fn the_root_gets_only_root_requirements() {
    let repo = cascade_repo();

    let run = repo.run(&["explain", "."]);

    assert!(
        run.stdout
            .starts_with("Requirements that apply to the repository root:"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("FAIL_FAST"), "{}", run.stdout);
    assert!(!run.stdout.contains("FAST_API"), "{}", run.stdout);
}

#[test]
fn without_reqfiles_nothing_applies() {
    let repo = repo!();

    let run = repo.run(&["explain", "src/main.rs"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(run.stdout, "No requirements apply to src/main.rs.\n");
}

#[test]
fn excluded_paths_have_no_requirements() {
    let repo = cascade_repo();
    repo.write(".reqfile/config.yaml", "exclude: [\"vendor/**\"]\n");

    let run = repo.run(&["explain", "vendor/lib.py"]);

    assert_eq!(
        run.stdout,
        "vendor/lib.py is excluded by .reqfile/config.yaml; no requirements apply.\n"
    );
}

#[test]
fn invalid_reqfiles_are_config_errors() {
    let repo = cascade_repo();
    repo.write("services/web/Reqfile.yaml", "reqfile: 2\n");

    let run = repo.run(&["explain", "services/api"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr
            .contains("services/web/Reqfile.yaml:1: unsupported Reqfile version 2"),
        "{}",
        run.stderr
    );
}
