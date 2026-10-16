//! PACKAGES, SELECTION, INSPECTION and NO_FALSE_GREEN: composed check programs
//! keep their own assets, provenance and bindings across import and eval.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Repo, remotes, repo, reqfile};

fn adapter(kind: &str, checks: &str) -> String {
    format!(
        "adapter: 1\nkind: {kind}\nchecks:\n  verify:\n{}\n",
        checks
            .lines()
            .map(|l| format!("    {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn command(run: &str) -> String {
    format!("- command:\n    run: {run}\n    fast: true\n    fix_hint: Repair the operation.")
}

fn reference(path: &str) -> String {
    format!("- adapter: {{ use: {path}, check: verify }}")
}

fn requirement(repo: &Repo, check: &str) {
    repo.write("Reqfile.yaml", &reqfile(&[("RELIABLE", check)]));
}

#[test]
fn two_composed_checks_keep_distinct_assets_and_report_their_origins() {
    let repo = repo!();
    requirement(
        &repo,
        &format!(
            "{}\n{}",
            reference("./adapters/a"),
            reference("./adapters/b")
        ),
    );
    for (name, text) in [("a", "first"), ("b", "second")] {
        repo.write(
            &format!("adapters/{name}/Adapter.yaml"),
            &adapter(
                "language",
                &command("cat \"$REQFILE_ASSETS/message\"; exit 1"),
            ),
        );
        repo.write(&format!("adapters/{name}/message"), text);
    }
    let run = repo.run(&["check", "--format", "json"]);
    assert_eq!(run.code, 1, "{}", run.output());
    let report = run.json();
    assert_eq!(report["findings"].as_array().unwrap().len(), 2);
    assert_eq!(report["findings"][0]["message"], "first");
    assert_eq!(report["findings"][1]["message"], "second");
    assert!(
        report["findings"][0]["source"]
            .as_str()
            .unwrap()
            .contains("adapters/a/Adapter.yaml#verify")
    );
    assert!(
        report["findings"][1]["source"]
            .as_str()
            .unwrap()
            .contains("adapters/b/Adapter.yaml#verify")
    );
    let explain = repo.run(&["explain", "."]);
    assert_eq!(explain.code, 0, "{}", explain.output());
    assert!(
        explain
            .stdout
            .contains("language adapters/a/Adapter.yaml#verify")
    );
    let listed = repo.run(&["list", "--format", "json"]).json();
    assert_eq!(
        listed["requirements"][0]["adapters"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn project_library_and_language_compose_with_explicit_json_bindings() {
    let repo = repo!();
    requirement(
        &repo,
        "- adapter:\n    use: ./adapters/project\n    check: verify\n    with:\n      operation: '$(touch injected); `touch injected2`'\n      owner: account\n      only_project: ignored_by_child",
    );
    repo.write("adapters/project/Adapter.yaml", "adapter: 1\nkind: project\ninputs: {required: [operation, owner], optional: [only_project]}\nchecks:\n  verify:\n    - adapter: {use: ../library, check: verify, with: {owner: workspace}}\n");
    repo.write("adapters/library/Adapter.yaml", "adapter: 1\nkind: library\ninputs: {required: [operation, owner]}\nchecks:\n  verify:\n    - adapter: {use: ../language, check: verify}\n");
    repo.write("adapters/language/Adapter.yaml", "adapter: 1\nkind: language\ninputs: {required: [operation, owner]}\nchecks:\n  verify:\n    - command:\n        run: echo \"$REQFILE_BINDING\"; test -f marker; exit 1\n        fix_hint: Repair it.\n");
    repo.write("marker", "project cwd");
    let run = repo.run(&["check", "--format", "json"]);
    assert_eq!(run.code, 1, "{}", run.output());
    let report = run.json();
    let binding: serde_json::Value =
        serde_json::from_str(report["findings"][0]["message"].as_str().unwrap()).unwrap();
    assert_eq!(binding["operation"], "$(touch injected); `touch injected2`");
    assert_eq!(binding["owner"], "workspace");
    assert!(binding.get("only_project").is_none());
    assert!(!repo.path().join("injected").exists());
    assert!(!repo.path().join("injected2").exists());
    let source = report["findings"][0]["source"].as_str().unwrap();
    assert!(source.contains("project adapters/project"));
    assert!(source.contains("library adapters/library"));
    assert!(source.contains("language adapters/language"));
}

#[test]
fn malformed_manifests_and_inputs_are_errors_never_empty_passes() {
    for (manifest, binding, expected) in [
        (
            "adapter: 99\nkind: language\nchecks: {}",
            "",
            "unsupported adapter version",
        ),
        ("adapter: 1\nkind: magic\nchecks: {}", "", "adapter kind"),
        (
            "adapter: 1\nkind: language\nchecks: {}",
            "",
            "at least one check",
        ),
        (
            "adapter: 1\nkind: language\nchecks: {verify: []}",
            "",
            "declares no checks",
        ),
        (
            "adapter: 1\nkind: language\ninputs: {required: [entry]}\nchecks: {verify: [{command: {run: 'true', fix_hint: x}}]}",
            "",
            "missing required adapter input `entry`",
        ),
        (
            "adapter: 1\nkind: language\ninputs: {required: [entry], optional: [entry]}\nchecks: {verify: [{command: {run: 'true', fix_hint: x}}]}",
            "",
            "both required and optional",
        ),
        (
            "adapter: 1\nkind: language\nchecks: {verify: [{command: {run: 'true', fix_hint: x}}]}",
            ", with: {typo: x}",
            "unknown adapter input `typo`",
        ),
        (
            "adapter: 1\nkind: language\nchecks: {another: [{command: {run: 'true', fix_hint: x}}]}",
            "",
            "no check export `verify`",
        ),
        (
            "adapter: 1\nkind: language\nunknown: true\nchecks: {}",
            "",
            "unknown key `unknown`",
        ),
    ] {
        let repo = repo!();
        requirement(
            &repo,
            &format!("- adapter: {{ use: ./adapter, check: verify{binding} }}"),
        );
        repo.write("adapter/Adapter.yaml", manifest);
        let run = repo.run(&["check"]);
        assert_eq!(run.code, 3, "{}", run.output());
        assert!(
            run.output().contains(expected),
            "expected {expected}: {}",
            run.output()
        );
    }
}

#[test]
fn missing_manifests_cycles_and_package_escapes_are_errors() {
    let repo = repo!();
    requirement(&repo, &reference("./missing"));
    assert_eq!(repo.run(&["check"]).code, 3);
    requirement(&repo, &reference("./a"));
    repo.write("a/Adapter.yaml", &adapter("language", &reference("../b")));
    repo.write("b/Adapter.yaml", &adapter("library", &reference("../a")));
    let cycle = repo.run(&["check"]);
    assert_eq!(cycle.code, 3, "{}", cycle.output());
    assert!(cycle.output().contains("adapter cycle"));
    repo.write(
        "a/Adapter.yaml",
        &adapter("language", &reference("../../outside")),
    );
    let escape = repo.run(&["check"]);
    assert_eq!(escape.code, 3, "{}", escape.output());
    assert!(escape.output().contains("escapes its package"));
}

#[test]
fn a_local_import_resolves_adapter_paths_at_the_definition() {
    let repo = repo!();
    repo.write(
        "shared/Reqfile.yaml",
        &reqfile(&[("RELIABLE", &reference("./adapter"))]),
    );
    repo.write(
        "shared/adapter/Adapter.yaml",
        &adapter("library", &command("test -f customer.txt")),
    );
    repo.write(
        "app/Reqfile.yaml",
        "reqfile: 1\ncode:\n  - {id: RELIABLE, use: ../shared}\n",
    );
    // The definition also runs in shared/.
    repo.write("shared/customer.txt", "source");
    repo.write("app/customer.txt", "consumer");
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(run.stdout.contains("2 checks run"));
}

#[test]
fn pinned_git_adapters_and_nested_references_work_without_requirement_definitions() {
    let remotes = remotes!();
    let source = remotes.repo("acme/checks");
    source.write(
        "adapters/library/Adapter.yaml",
        &adapter("library", &reference("../language")),
    );
    source.write(
        "adapters/language/Adapter.yaml",
        &adapter(
            "language",
            &command("cat \"$REQFILE_ASSETS/message\"; exit 1"),
        ),
    );
    source
        .write("adapters/language/message", "pinned tool")
        .commit("checker");
    let pin = source.head();
    let repo = repo!();
    remotes.serve(&repo);
    requirement(
        &repo,
        &format!("- adapter: {{use: acme/checks@{pin}, path: adapters/library, check: verify}}"),
    );
    let run = repo.run(&["check", "--format", "json"]);
    assert_eq!(run.code, 1, "{}", run.output());
    assert_eq!(run.json()["findings"][0]["message"], "pinned tool");
    assert!(
        run.json()["findings"][0]["source"]
            .as_str()
            .unwrap()
            .contains(&pin)
    );
}

#[test]
fn inherited_checks_in_a_git_requirement_keep_the_source_relative_adapter() {
    let remotes = remotes!();
    let source = remotes.repo("acme/requirements");
    source.write(
        "rule/Reqfile.yaml",
        &reqfile(&[("RELIABLE", &reference("./adapter"))]),
    );
    source.write(
        "rule/adapter/Adapter.yaml",
        &adapter("project", &command("test -f consumer.txt")),
    );
    source.commit("package");
    let repo = repo!();
    remotes.serve(&repo);
    repo.write(
        "Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n  - {{id: RELIABLE, use: acme/requirements@{}}}\n",
            source.head()
        ),
    );
    repo.write("consumer.txt", "customer code");
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn changing_a_transitive_adapter_asset_reruns_unchanged_source_files() {
    let repo = repo!();
    requirement(&repo, &reference("./adapters/library"));
    repo.write(
        "adapters/library/Adapter.yaml",
        &adapter("library", &reference("../language")),
    );
    repo.write("adapters/language/Adapter.yaml", &adapter("language", "- command:\n    run: sh \"$REQFILE_ASSETS/check.sh\"\n    files: '**/*.ts'\n    pass_files: true\n    fast: true\n    fix_hint: Fix it."));
    repo.write("adapters/language/check.sh", "exit 0\n");
    repo.write("src/unchanged.ts", "export const value = 1;")
        .commit("base");
    let no_change = repo.run(&["check", "--changed", "HEAD"]);
    assert_eq!(no_change.code, 0, "{}", no_change.output());
    assert!(no_change.stdout.contains("0 checks run"));
    repo.write("adapters/language/check.sh", "echo \"$*\"; exit 1\n");
    let changed = repo.run(&["check", "--changed", "HEAD"]);
    assert_eq!(changed.code, 1, "{}", changed.output());
    assert!(changed.stdout.contains("src/unchanged.ts"));
}

#[test]
fn executable_assets_under_examples_invalidate_unchanged_targets() {
    let repo = repo!();
    requirement(&repo, &reference("./adapter"));
    repo.write("adapter/Adapter.yaml", &adapter("project", "- command:\n    run: sh \"$REQFILE_ASSETS/examples/check.sh\"\n    files: '**/*.ts'\n    fast: true\n    fix_hint: Repair the operation."));
    repo.write("adapter/examples/check.sh", "exit 0\n");
    repo.write("src/main.ts", "export {};").commit("base");
    assert_eq!(repo.run(&["check", "--changed", "HEAD"]).code, 0);
    repo.write("adapter/examples/check.sh", "exit 1\n");
    let run = repo.run(&["check", "--changed", "HEAD"]);
    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("1 check run"));
}

#[test]
fn conventional_project_binding_is_allowed_and_invalidates_inherited_checks() {
    let remotes = remotes!();
    let source = remotes.repo("acme/requirements");
    source.write(
        "Reqfile.yaml",
        &reqfile(&[("RELIABLE", &reference("./adapter"))]),
    );
    source.write("adapter/Adapter.yaml", &adapter("project", "- command:\n    run: test \"$(cat .reqfile/RELIABLE/binding.json)\" = good\n    files: '**/*.ts'\n    fast: true\n    fix_hint: Fix it."));
    source.commit("package");
    let repo = repo!();
    remotes.serve(&repo);
    repo.write(
        "Reqfile.yaml",
        &format!(
            "reqfile: 1\ncode:\n  - {{id: RELIABLE, use: acme/requirements@{}}}\n",
            source.head()
        ),
    );
    repo.write("src/main.ts", "export {};");
    repo.write(".reqfile/RELIABLE/binding.json", "good")
        .commit("base");
    assert_eq!(repo.run(&["check"]).code, 0);
    repo.write(".reqfile/RELIABLE/binding.json", "bad");
    let changed = repo.run(&["check", "--changed", "HEAD"]);
    assert_eq!(changed.code, 1, "{}", changed.output());
    std::fs::remove_file(repo.path().join(".reqfile/RELIABLE/binding.json")).unwrap();
    let deleted = repo.run(&["check", "--changed", "HEAD"]);
    assert_eq!(deleted.code, 1, "{}", deleted.output());
    assert!(deleted.stdout.contains("1 check run"));
}

#[test]
fn eval_uses_case_local_binding_but_original_adapter_assets() {
    let repo = repo!();
    requirement(&repo, &reference("./adapter"));
    repo.write(
        "adapter/Adapter.yaml",
        &adapter("project", &command("sh \"$REQFILE_ASSETS/check.sh\"")),
    );
    repo.write(
        "adapter/check.sh",
        "test \"$(cat .reqfile/RELIABLE/binding.json)\" = fixture\n",
    );
    repo.write(".reqfile/RELIABLE/binding.json", "host-must-not-be-read");
    repo.write(
        ".reqfile/RELIABLE/examples/case/example.yaml",
        "expected: ok\nrationale: The case owns its binding.\n",
    );
    repo.write(
        ".reqfile/RELIABLE/examples/case/files/.reqfile/RELIABLE/binding.json",
        "fixture",
    );
    let run = repo.run(&["eval", "--only", "RELIABLE"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("1 examples, 1 as labeled"),
        "{}",
        run.output()
    );
}

#[test]
fn local_check_overrides_still_replace_all_inherited_checks() {
    let repo = repo!();
    repo.write(
        "shared/Reqfile.yaml",
        &reqfile(&[("RELIABLE", &reference("./adapter"))]),
    );
    repo.write(
        "shared/adapter/Adapter.yaml",
        &adapter("project", &command("exit 1")),
    );
    repo.write("Reqfile.yaml", "reqfile: 1\ncode:\n  - id: RELIABLE\n    use: ./shared\n    checks:\n      - command: {run: 'true', fix_hint: x}\n");
    let run = repo.run(&["check", "--format", "json"]);
    assert_eq!(run.code, 1, "{}", run.output());
    assert_eq!(run.json()["summary"]["checks_run"], 2);
    assert_eq!(run.json()["findings"].as_array().unwrap().len(), 1);
    assert!(
        run.json()["findings"][0]["source"]
            .as_str()
            .unwrap()
            .contains("shared/adapter")
    );
}

#[test]
fn adapter_decision_reads_its_own_spec() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    requirement(&repo, &reference("./adapter"));
    repo.write("adapter/Adapter.yaml", &adapter("language", "- decision"));
    repo.write("adapter/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("main.py", "def example():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "test-key");
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(run.stdout.contains("1 code unit judged"));
}

#[test]
fn a_composition_cannot_silently_overwrite_a_decision_question() {
    let repo = repo!();
    requirement(
        &repo,
        &format!("{}\n{}", reference("./a"), reference("./b")),
    );
    for dir in ["a", "b"] {
        repo.write(
            &format!("{dir}/Adapter.yaml"),
            &adapter("language", "- decision"),
        );
    }
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(run.output().contains("more than one decision check"));
}

#[test]
fn adapter_tags_and_unknown_reference_fields_are_rejected() {
    for check in [
        "- adapter: {use: acme/checks@v1, check: verify}",
        "- adapter: {use: ./a, check: verify, typo: true}",
        "- adapter: {use: ./a, path: ../outside, check: verify}",
        "- adapter: {use: acme/checks@0123456789012345678901234567890123456789, path: ../outside, check: verify}",
    ] {
        let repo = repo!();
        requirement(&repo, check);
        let run = repo.run(&["check"]);
        assert_eq!(run.code, 3, "{}", run.output());
    }
}

#[test]
fn a_symlink_cannot_supply_adapter_code_from_outside_the_repository() {
    let repo = repo!();
    requirement(&repo, &reference("./adapter"));
    repo.write("external.yaml", &adapter("language", &command("true")));
    std::fs::create_dir(repo.path().join("adapter")).unwrap();
    std::os::unix::fs::symlink(
        repo.path().join("external.yaml"),
        repo.path().join("adapter/Adapter.yaml"),
    )
    .unwrap();
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 3, "{}", run.output());
    assert!(run.output().contains("not a regular"));
}

#[test]
fn add_previews_nested_adapter_commands_without_executing_them() {
    let remotes = remotes!();
    let source = remotes.repo("acme/requirements");
    source.write(
        "Reqfile.yaml",
        &reqfile(&[("RELIABLE", &reference("./library"))]),
    );
    source.write(
        "library/Adapter.yaml",
        &adapter("library", &reference("../language")),
    );
    source.write(
        "language/Adapter.yaml",
        &adapter("language", &command("touch should-not-run; exit 1")),
    );
    source.commit("package");
    let repo = repo!();
    remotes.serve(&repo);
    let run = repo.run(&[
        "add",
        &format!("acme/requirements@{}", source.head()),
        "--dry-run",
    ]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(run.stdout.contains("command: touch should-not-run; exit 1"));
    assert!(!repo.path().join("should-not-run").exists());
    assert!(!repo.path().join("Reqfile.yaml").exists());
}
