//! LIST: `reqfile list` lists every requirement of the repository, with its type and checks.

use reqfile_test_support::{Repo, repo};

fn requirement(id: &str, checks: &str) -> String {
    format!("  - id: {id}\n    must: Must {id}.\n    why: Because of {id}.\n    checks:\n{checks}")
}

const COMMAND: &str = "      - command:\n          run: \"true\"\n          fix_hint: x\n";
const DECISION: &str = "      - decision\n";
const BLOCKING: &str = "      - decision: { mode: blocking }\n";

fn cascade_repo() -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n{}{}",
            requirement("FAIL_FAST", &format!("{COMMAND}{DECISION}{COMMAND}")),
            requirement("DECOMPLECT", BLOCKING)
        ),
    );
    repo.write(
        "services/api/Reqfile.yaml",
        &format!("reqfile: 1\nproduct:\n{}", requirement("FAST_API", COMMAND)),
    );
    repo.write(
        "services/web/Reqfile.yaml",
        &format!("reqfile: 1\nproduct:\n{}", requirement("PRETTY", COMMAND)),
    );
    repo
}

#[test]
fn every_requirement_is_listed_with_its_type_and_checks() {
    let repo = cascade_repo();

    let run = repo.run_in("services/api", &["list"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "From Reqfile.yaml:\n  FAIL_FAST (code)  checks: command, decision\n  DECOMPLECT (code)  checks: decision (blocking)\n\
         \nFrom services/api/Reqfile.yaml:\n  FAST_API (product)  checks: command\n\
         \nFrom services/web/Reqfile.yaml:\n  PRETTY (product)  checks: command\n\
         \n4 requirements: 2 product, 2 code.\n"
    );
}

#[test]
fn kind_filters_by_type() {
    let repo = cascade_repo();

    let run = repo.run(&["list", "--kind", "product"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "From services/api/Reqfile.yaml:\n  FAST_API (product)  checks: command\n\
         \nFrom services/web/Reqfile.yaml:\n  PRETTY (product)  checks: command\n\
         \n2 product requirements.\n"
    );
}

#[test]
fn nothing_to_list_is_said() {
    let repo = repo!();
    assert_eq!(
        repo.run(&["list"]).stdout,
        "No requirements in this repository.\n"
    );

    let repo = cascade_repo();
    repo.write("services/api/Reqfile.yaml", "reqfile: 1\ncode: []\n");
    repo.write("services/web/Reqfile.yaml", "reqfile: 1\ncode: []\n");
    let run = repo.run(&["list", "--kind", "product"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(run.stdout, "No product requirements in this repository.\n");
}

#[test]
fn invalid_reqfiles_are_config_errors() {
    let repo = cascade_repo();
    repo.write("services/web/Reqfile.yaml", "reqfile: 2\n");

    let run = repo.run(&["list"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr
            .contains("services/web/Reqfile.yaml:1: unsupported Reqfile version 2"),
        "{}",
        run.stderr
    );
}
