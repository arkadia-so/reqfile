//! REQFILE_FORMAT: the Reqfile.yaml format, and errors that name the file and line.

use reqfile_test_support::repo;

const VALID: &str = r#"reqfile: 1

product:
  - id: FAST_SEARCH
    ref: Nielsen, "Response Times"
    must: Search answers in under a second.
    why: Users leave slow pages.
    who: Shoppers.
    checks:
      - command:
          run: "true"
          fix_hint: Make search faster.

code:
  - id: NO_TODO
    must: No TODO comments.
    why: They rot.
    who: Maintainers.
    checks:
      - command:
          run: "true"
          fix_hint: Remove the TODO.
"#;

/// Runs `reqfile check` on a repository whose only Reqfile is `content`.
fn check(content: &str) -> reqfile_test_support::Run {
    let repo = repo!();
    repo.write("Reqfile.yaml", content);
    repo.run(&["check"])
}

fn assert_config_error(content: &str, expected: &str) {
    let run = check(content);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(expected),
        "expected {expected:?} in:\n{}",
        run.stdout
    );
}

#[test]
fn a_valid_reqfile_with_product_and_code_requirements_passes() {
    let run = check(VALID);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(run.stdout.contains("2 checks run"), "{}", run.stdout);
}

#[test]
fn the_version_key_is_required_anywhere_in_the_file() {
    assert_config_error(
        &VALID.replace("reqfile: 1\n", ""),
        "Reqfile.yaml:2: a Reqfile needs a `reqfile: 1` version key",
    );
    assert_config_error(
        "",
        "Reqfile.yaml:1: a Reqfile needs a `reqfile: 1` version key",
    );
    let run = check(&(VALID.replace("reqfile: 1\n", "") + "reqfile: 1\n"));
    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn who_and_ref_are_optional() {
    let run = check(
        &VALID
            .replace("    who: Maintainers.\n", "")
            .replace("    ref: Nielsen, \"Response Times\"\n", ""),
    );
    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn an_unsupported_version_fails() {
    assert_config_error(
        &VALID.replace("reqfile: 1", "reqfile: 2"),
        "Reqfile.yaml:1: unsupported Reqfile version 2",
    );
}

#[test]
fn other_type_keys_fail() {
    assert_config_error(
        &VALID.replace("product:", "process:"),
        "Reqfile.yaml:3: unknown key `process` in the Reqfile",
    );
}

#[test]
fn unknown_requirement_keys_fail() {
    assert_config_error(
        &VALID.replace("    must: No TODO", "    should: No TODO"),
        "Reqfile.yaml:16: unknown key `should` in a requirement",
    );
}

#[test]
fn unknown_check_keys_fail() {
    assert_config_error(
        &VALID.replace(
            "          fix_hint: Remove the TODO.",
            "          fix_hint: Remove the TODO.\n          retries: 3",
        ),
        "Reqfile.yaml:23: unknown key `retries` in a command check",
    );
}

#[test]
fn missing_fields_fail() {
    for (field, line) in [
        ("must", "    must: No TODO comments.\n"),
        ("why", "    why: They rot.\n"),
    ] {
        assert_config_error(
            &VALID.replace(line, ""),
            &format!(
                "Reqfile.yaml:15: requirement NO_TODO is missing the required field `{field}`"
            ),
        );
    }
    assert_config_error(
        &VALID.replace("  - id: NO_TODO\n", "  - ref: x\n"),
        "Reqfile.yaml:15: a requirement is missing the required field `id`",
    );
    assert_config_error(
        &VALID.replace("          fix_hint: Remove the TODO.\n", ""),
        "Reqfile.yaml:21: a command check is missing the required field `fix_hint`",
    );
}

#[test]
fn ids_are_screaming_snake_case() {
    for bad in ["noTodo", "NO-TODO", "_NO_TODO", "NO__TODO", "NO_TODO_"] {
        assert_config_error(
            &VALID.replace("id: NO_TODO", &format!("id: {bad}")),
            &format!("Reqfile.yaml:15: requirement id `{bad}` must be SCREAMING_SNAKE_CASE"),
        );
    }
}

#[test]
fn fields_must_have_the_right_type() {
    assert_config_error(
        &VALID.replace("must: No TODO comments.", "must: [a, b]"),
        "Reqfile.yaml:16: `must` must be a non-empty string",
    );
    assert_config_error(
        &VALID.replace("reqfile: 1", "reqfile: one"),
        "Reqfile.yaml:1: `reqfile` must be an integer",
    );
}

#[test]
fn duplicate_keys_fail() {
    assert_config_error(
        &VALID.replace(
            "    why: They rot.\n",
            "    why: They rot.\n    why: Again.\n",
        ),
        "Reqfile.yaml:18: duplicate key `why` (first defined at line 17)",
    );
}

#[test]
fn invalid_yaml_names_the_line() {
    assert_config_error(
        &VALID.replace("    who: Maintainers.", "    who: [Maintainers."),
        "Reqfile.yaml:",
    );
}

#[test]
fn near_miss_file_names_fail() {
    for name in [
        "reqfile.yaml",
        "REQFILE.yaml",
        "Reqfile.yml",
        "sub/reqfile.yml",
    ] {
        let repo = repo!();
        repo.write(name, VALID);
        let run = repo.run(&["check"]);
        assert_eq!(run.code, 3, "{name}: {}", run.output());
        assert!(
            run.stdout.contains(&format!(
                "{name}:1: a requirements file must be named exactly Reqfile.yaml"
            )),
            "{name}: {}",
            run.stdout
        );
    }
}

#[test]
fn errors_name_files_in_subfolders() {
    let repo = repo!();
    repo.write("Reqfile.yaml", VALID);
    repo.write(
        "services/api/Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: X\n    must: y\n",
    );
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "services/api/Reqfile.yaml:3: requirement X is missing the required field `why`"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn near_miss_names_of_other_tools_files_are_ignored() {
    let repo = repo!();
    repo.write("Reqfile.yaml", VALID).write(
        ".github/workflows/reqfile.yml",
        "name: reqfile\non: pull_request\njobs: {}\n",
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn a_near_miss_without_version_key_still_fails() {
    let repo = repo!();
    repo.write("reqfile.yml", "product: []\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("reqfile.yml:1: a requirements file must be named exactly Reqfile.yaml"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_near_miss_renamed_before_staging_is_no_longer_an_error() {
    let repo = repo!();
    repo.write("Reqfile.yml", VALID)
        .write(".reqfile/config.yml", "base: main\n")
        .commit("misnamed");
    std::fs::rename(
        repo.path().join("Reqfile.yml"),
        repo.path().join("Reqfile.yaml"),
    )
    .expect("rename");
    std::fs::rename(
        repo.path().join(".reqfile/config.yml"),
        repo.path().join(".reqfile/config.yaml"),
    )
    .expect("rename");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
}
