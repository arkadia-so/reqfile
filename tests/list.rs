//! INSPECTION: `reqfile list` lists every requirement of the repository, with
//! its type, checks and source.

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

/// A repository defining FAIL_FAST in `std/` and taking it at the root, once
/// as it is and once, in `app/`, with its own checks.
fn use_repo() -> Repo {
    let repo = repo!();
    repo.write(
        "std/Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n{}process:\n{}",
            requirement("FAIL_FAST", DECISION),
            requirement("REVIEWED", COMMAND)
        ),
    );
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: FAIL_FAST, use: ./std }\nprocess:\n  - { id: REVIEWED, use: ./std }\n",
    );
    repo.write(
        "app/Reqfile.yaml",
        &format!("reqfile: 1\ncode:\n  - id: FAIL_FAST\n    use: ../std\n    checks:\n{COMMAND}"),
    );
    repo
}

#[test]
fn list_shows_use_blocks_with_their_source() {
    let repo = use_repo();

    let run = repo.run(&["list"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "From Reqfile.yaml:\n  FAIL_FAST (code)  use: ./std  checks: decision (inherited)\n  REVIEWED (process)  use: ./std  checks: command (inherited)\n\
         \nFrom app/Reqfile.yaml:\n  FAIL_FAST (code)  use: ../std  checks: command (set here)\n\
         \nFrom std/Reqfile.yaml:\n  FAIL_FAST (code)  checks: decision\n  REVIEWED (process)  checks: command\n\
         \n5 requirements: 0 product, 3 code, 2 process.\n"
    );
}

#[test]
fn list_supports_json_output() {
    let repo = use_repo();

    let run = repo.run(&["list", "--format", "json", "--kind", "code"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let json = run.json();
    let requirements = json["requirements"].as_array().expect("a list");
    assert_eq!(requirements.len(), 3);
    assert_eq!(
        requirements[0],
        serde_json::json!({
            "reqfile": "Reqfile.yaml",
            "line": 3,
            "id": "FAIL_FAST",
            "kind": "code",
            "must": "Must FAIL_FAST.",
            "why": "Because of FAIL_FAST.",
            "who": null,
            "ref": null,
            "checks": ["decision"],
            "imported": {
                "use": "./std",
                "definition": "std/Reqfile.yaml:3",
                "commit": null,
                "checks_inherited": true,
            },
        })
    );
    assert_eq!(requirements[2]["imported"], serde_json::Value::Null);
}
