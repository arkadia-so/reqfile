//! CONFIG_CASCADE: `.reqfile/config.yaml` in any folder, the nearest one
//! winning for each key, and nothing specific to the repository root.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Received, repo, reqfile};

const LIST_TXT: &str = r#"- command:
    run: "fail() { echo \"got $*\"; exit 1; }; fail"
    files: "**/*.txt"
    pass_files: true
    fix_hint: None."#;

/// The model of the question requests carrying the question of `id`.
fn models_asking(received: &[Received], id: &str) -> Vec<String> {
    received
        .iter()
        .filter(|r| r.body["questions"].get(id).is_some())
        .map(|r| r.body["model"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn each_setting_comes_from_the_nearest_config() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("ROOT_RULE", "- decision")]));
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_RULE", "- decision")]));
    repo.write(
        "app/deep/Reqfile.yaml",
        &reqfile(&[("DEEP_RULE", "- decision")]),
    );
    for dir in ["", "app/", "app/deep/"] {
        let id = match dir {
            "" => "ROOT_RULE",
            "app/" => "APP_RULE",
            _ => "DEEP_RULE",
        };
        repo.write(
            &format!("{dir}.reqfile/{id}/decision.yaml"),
            PYTHON_FUNCTIONS,
        );
    }
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  model: root-model-1.0.0\n", jev.config()),
    );
    repo.write(
        "app/.reqfile/config.yaml",
        "decision:\n  model: app-model-2.0.0\n",
    );
    repo.write("app/deep/code.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let received = jev.questions_received();
    assert_eq!(models_asking(&received, "ROOT_RULE"), ["root-model-1.0.0"]);
    assert_eq!(models_asking(&received, "APP_RULE"), ["app-model-2.0.0"]);
    assert_eq!(
        models_asking(&received, "DEEP_RULE"),
        ["app-model-2.0.0"],
        "a folder without a config inherits the nearest one"
    );
    assert_eq!(
        received.len(),
        2,
        "questions sharing settings share a request"
    );
}

#[test]
fn settings_are_resolved_key_by_key() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_RULE", "- decision")]));
    repo.write("app/.reqfile/APP_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  api_key_env: ROOT_KEY\n", jev.config()),
    );
    repo.write(
        "app/.reqfile/config.yaml",
        "decision:\n  model: app-model-2.0.0\n",
    );
    repo.write("app/code.py", "def f():\n    pass\n");
    repo.env("ROOT_KEY", "root-key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let received = jev.questions_received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body["model"], "app-model-2.0.0");
    assert_eq!(
        received[0].authorization.as_deref(),
        Some("Bearer root-key")
    );
}

#[test]
fn a_nested_endpoint_sends_that_folder_s_questions_elsewhere() {
    let root_jev = FakeJev::by_marker();
    let app_jev = FakeJev::by_marker();
    // Another endpoint serving another model: answers are not shared.
    app_jev.answer_as("other/jev-2.0.0");
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("ROOT_RULE", "- decision")]));
    repo.write(".reqfile/ROOT_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_RULE", "- decision")]));
    repo.write("app/.reqfile/APP_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &root_jev.config());
    repo.write("app/.reqfile/config.yaml", &app_jev.config());
    repo.write("app/code.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        models_asking(&root_jev.questions_received(), "ROOT_RULE").len(),
        1
    );
    assert_eq!(
        models_asking(&root_jev.questions_received(), "APP_RULE").len(),
        0
    );
    assert_eq!(
        models_asking(&app_jev.questions_received(), "APP_RULE").len(),
        1
    );
}

#[test]
fn each_folder_is_compared_with_its_own_base() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("ROOT_LIST", LIST_TXT)]));
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_LIST", LIST_TXT)]));
    repo.write(".reqfile/config.yaml", "base: main\n");
    repo.write("app/.reqfile/config.yaml", "base: release\n");
    repo.write("top.txt", "")
        .write("app/x.txt", "")
        .commit("files");
    repo.git(&["branch", "release"]);
    repo.write("top.txt", "v2")
        .write("app/x.txt", "v2")
        .commit("on main");
    repo.git(&["checkout", "-q", "-b", "feature"]);

    let run = repo.run(&["check", "--changed"]);

    assert!(
        !run.stdout.contains("ROOT_LIST"),
        "nothing changed since main: {}",
        run.stdout
    );
    assert!(
        run.stdout.contains("APP_LIST  got x.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn the_base_defaults_to_the_remote_default_branch() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write("old.txt", "").commit("base");
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("new.txt", "");

    let run = repo.run(&["check", "--changed"]);

    assert!(run.stdout.contains("LIST  got new.txt\n"), "{}", run.stdout);
}

#[test]
fn nothing_is_specific_to_the_repository_root() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "svc/Reqfile.yaml",
        &reqfile(&[("SVC_RULE", "- decision"), ("SVC_LIST", LIST_TXT)]),
    );
    repo.write("svc/.reqfile/SVC_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(
        "svc/.reqfile/config.yaml",
        &format!("exclude: [\"gen/**\"]\n{}", jev.config()),
    );
    repo.write("svc/code.py", "def f():\n    pass  # VIOLATION\n");
    repo.write("svc/gen/out.txt", "").write("svc/notes.txt", "");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout
            .contains("advisory  SVC_RULE  svc/code.py:1  p=0.95"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("SVC_LIST  got notes.txt\n"),
        "{}",
        run.stdout
    );
    assert!(repo.path().join(".git/reqfile/jev-cache").is_file());
    assert!(!repo.path().join(".reqfile").exists());
}

#[test]
fn an_invalid_config_names_its_file_and_line() {
    for (content, expected) in [
        (
            "base: main\nexclud: []\n",
            "app/.reqfile/config.yaml:2: unknown key `exclud` in the config",
        ),
        (
            "decision:\n  modle: x\n",
            "app/.reqfile/config.yaml:2: unknown key `modle` in decision",
        ),
        (
            "decision:\n  concurrency: 0\n",
            "app/.reqfile/config.yaml:2: `concurrency` must be at least 1",
        ),
        (
            "exclude: gen/**\n",
            "app/.reqfile/config.yaml:1: `exclude` must be a list",
        ),
    ] {
        let repo = repo!();
        repo.write("app/Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
        repo.write("app/.reqfile/config.yaml", content);

        let run = repo.run(&["check"]);

        assert_eq!(run.code, 3, "{content}: {}", run.output());
        assert!(
            run.stdout.contains(expected),
            "expected {expected:?} in:\n{}",
            run.stdout
        );
    }
}

#[test]
fn a_near_miss_config_name_fails() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write("app/.reqfile/config.yml", "base: main\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "app/.reqfile/config.yml:1: a config file must be named exactly .reqfile/config.yaml"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn answers_of_every_settings_group_stay_cached() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("ROOT_RULE", "- decision")]));
    repo.write(".reqfile/ROOT_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_RULE", "- decision")]));
    repo.write("app/.reqfile/APP_RULE/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(
        "app/.reqfile/config.yaml",
        "decision:\n  model: app-model-2.0.0\n",
    );
    repo.write("app/code.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    repo.run(&["check"]);
    let asked = jev.questions_received().len();
    repo.run(&["check"]);

    assert_eq!(asked, 2, "one request per model");
    assert_eq!(
        jev.questions_received().len(),
        asked,
        "the second run asks nothing again"
    );
}
