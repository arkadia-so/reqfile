//! IMPORTS: use blocks take a requirement by reference from a folder or a
//! repository pinned to a tag or commit, checked as if declared where used.

use std::path::{Path, PathBuf};

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Remotes, Repo, Run, remotes, repo, reqfile};

/// A command check that fails with `label` and the files it receives.
fn list_files(label: &str) -> String {
    format!(
        r#"- command:
    run: "fail() {{ echo \"{label} $*\"; exit 1; }}; fail"
    files: "**/*.txt"
    pass_files: true
    fix_hint: None."#
    )
}

/// A command check that always fails with `label`.
fn say(label: &str) -> String {
    format!("- command:\n    run: echo {label}; exit 1\n    fix_hint: None.")
}

/// A Reqfile with one use block of type `kind`.
fn use_block(kind: &str, id: &str, location: &str) -> String {
    format!("reqfile: 1\n{kind}:\n  - {{ id: {id}, use: {location} }}\n")
}

/// A decision.yaml asking `question` about Python functions.
fn asking(question: &str) -> String {
    PYTHON_FUNCTIONS.replace("Does this function do more than one job?", question)
}

/// The question each request asked about its unit, as (unit path, question).
fn asked(jev: &FakeJev, id: &str) -> Vec<(String, String)> {
    let mut asked: Vec<(String, String)> = jev
        .questions_received()
        .iter()
        .map(|r| {
            let questions = r.body["questions"].as_object().expect("questions");
            assert_eq!(questions.len(), 1, "one question per unit: {:?}", r.body);
            (
                r.body["state"]["path"]
                    .as_str()
                    .expect("a path")
                    .to_string(),
                r.body["questions"][id]["instructions"]["question"]
                    .as_str()
                    .expect("a question")
                    .to_string(),
            )
        })
        .collect();
    asked.sort();
    asked
}

/// `acme/reqs` with a commit and tag per `(tag, reqfile)`, each commit
/// replacing the Reqfile; returns the source and its commits.
fn acme(remotes: &Remotes, versions: &[(&str, &str)]) -> (Repo, Vec<String>) {
    let source = remotes.repo("acme/reqs");
    let mut commits = Vec::new();
    for (tag, content) in versions {
        source.write("Reqfile.yaml", content).commit(tag);
        source.git(&["tag", tag]);
        commits.push(source.head());
    }
    (source, commits)
}

/// A consumer repository fetching its sources from `remotes`.
fn consumer(remotes: &Remotes) -> Repo {
    let repo = repo!();
    remotes.serve(&repo);
    repo
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().expect("canonical path")
}

#[test]
fn a_use_block_resolves_to_the_definition_with_its_id_at_its_location() {
    let remotes = remotes!();
    acme(
        &remotes,
        &[(
            "v1",
            &reqfile(&[
                ("NO_TODO", &list_files("NO_TODO got")),
                ("OTHER", &say("OTHER ran")),
            ]),
        )],
    );
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "NO_TODO", "acme/reqs@v1"),
    );
    repo.write("a.txt", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("NO_TODO  NO_TODO got a.txt"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("OTHER"), "{}", run.stdout);

    // A local source in scope: each unit is judged once, by the nearest block.
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &use_block("code", "FAIL_FAST", "./std"));
    repo.write("std/Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("std/.reqfile/FAIL_FAST/decision.yaml", &asking("Q1?"));
    repo.write("src/a.py", "def f():\n    pass  # VIOLATION\n");
    repo.write("std/tools/b.py", "def g():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        asked(&jev, "FAIL_FAST"),
        vec![
            ("src/a.py".to_string(), "Q1?".to_string()),
            ("std/tools/b.py".to_string(), "Q1?".to_string()),
        ]
    );
    assert!(
        run.stdout.contains("advisory  FAIL_FAST  src/a.py:1")
            && run.stdout.contains("from: Reqfile.yaml:3, use ./std"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("advisory  FAIL_FAST  std/tools/b.py:1"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_local_location_is_relative_to_the_folder_of_its_reqfile() {
    let repo = repo!();
    repo.write(
        "shared/Reqfile.yaml",
        &reqfile(&[("SHARED", &say("shared ran"))]),
    );
    repo.write(
        "app/Reqfile.yaml",
        &use_block("code", "SHARED", "../shared"),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("SHARED  shared ran"), "{}", run.stdout);

    repo.write("app/Reqfile.yaml", &use_block("code", "SHARED", "./shared"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "app/Reqfile.yaml:3: `use: ./shared`: no Reqfile at or under app/shared defines SHARED"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn a_use_block_is_never_a_resolution_target() {
    // acme's root takes FAIL_FAST from its std/ with local checks asking QX;
    // a consumer of acme gets std's definition, never acme's own use block.
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: FAIL_FAST\n    use: ./std\n    checks:\n      - command:\n          run: echo QX; exit 1\n          fix_hint: None.\n",
    );
    source.write("std/Reqfile.yaml", &reqfile(&[("FAIL_FAST", &say("Q1"))]));
    source.commit("s1");
    source.git(&["tag", "v1"]);
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "FAIL_FAST", "acme/reqs@v1"),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("FAIL_FAST  Q1"), "{}", run.stdout);
    assert!(!run.stdout.contains("QX"), "{}", run.stdout);
    assert!(
        run.stdout
            .contains("use acme/reqs@v1 (std/Reqfile.yaml:3 at commit"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_use_block_pointing_to_another_use_block_is_an_error() {
    let repo = repo!();
    repo.write("def/Reqfile.yaml", &reqfile(&[("RULE", &say("ran"))]));
    repo.write("lib/Reqfile.yaml", &use_block("code", "RULE", "../def"));
    repo.write("Reqfile.yaml", &use_block("code", "RULE", "./lib"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "Reqfile.yaml:3: `use: ./lib`: RULE in lib/Reqfile.yaml:3 is a use block, not a definition; point `use` to the folder that defines RULE"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn the_section_type_must_match_the_definition() {
    let repo = repo!();
    repo.write(
        "lib/Reqfile.yaml",
        "reqfile: 1\nprocess:\n  - id: REVIEWED\n    must: Reviewed.\n    why: Because.\n    checks:\n      - command:\n          run: \"true\"\n          fix_hint: x\n",
    );
    repo.write("Reqfile.yaml", &use_block("code", "REVIEWED", "./lib"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "Reqfile.yaml:3: requirement REVIEWED is listed under `code`, but its definition in lib/Reqfile.yaml:3 is a process requirement; list it under `process`"
        ),
        "{}",
        run.stdout
    );

    repo.write("Reqfile.yaml", &use_block("process", "REVIEWED", "./lib"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn only_code_and_process_requirements_can_be_imported() {
    let repo = repo!();
    repo.write(
        "lib/Reqfile.yaml",
        "reqfile: 1\nproduct:\n  - id: FAST\n    must: Fast.\n    why: Because.\n    checks:\n      - command:\n          run: \"true\"\n          fix_hint: x\n",
    );
    repo.write("Reqfile.yaml", &use_block("product", "FAST", "./lib"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("only code and process requirements can be taken with `use`"),
        "{}",
        run.stdout
    );
}

#[test]
fn source_discovery_honors_the_sources_exclusions() {
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source.write("Reqfile.yaml", &reqfile(&[("KEPT", &say("kept ran"))]));
    source.write(
        "vendor/Reqfile.yaml",
        &reqfile(&[("VENDORED", &say("vendored"))]),
    );
    source.write(".reqfile/config.yaml", "exclude: [\"vendor/**\"]\n");
    source.commit("s1");
    source.git(&["tag", "v1"]);
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: KEPT, use: acme/reqs@v1 }\n",
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("KEPT  kept ran"), "{}", run.stdout);

    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: VENDORED, use: acme/reqs@v1 }\n",
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(&format!(
            "`use: acme/reqs@v1`: no Reqfile at or under the root of acme/reqs at commit {} defines VENDORED",
            source.head()
        )),
        "{}",
        run.stdout
    );
}

#[test]
fn source_settings_are_never_used_as_settings() {
    let ours = FakeJev::by_marker();
    let theirs = FakeJev::by_marker();
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source.write("Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    source.write(".reqfile/FAIL_FAST/decision.yaml", PYTHON_FUNCTIONS);
    source.write(
        ".reqfile/config.yaml",
        &format!(
            "{}  model: source/model\n  api_key_env: SOURCE_KEY\n",
            theirs.config()
        ),
    );
    source.commit("s1");
    source.git(&["tag", "v1"]);
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "FAIL_FAST", "acme/reqs@v1"),
    );
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  model: consumer/model\n", ours.config()),
    );
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "consumer-key");
    repo.env("SOURCE_KEY", "source-key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("advisory  FAIL_FAST  a.py:1"),
        "{}",
        run.stdout
    );
    assert!(theirs.received().is_empty());
    let received = ours.received();
    assert!(!received.is_empty());
    for request in received {
        assert_eq!(request.body["model"], "consumer/model");
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer consumer-key")
        );
    }
}

#[test]
fn branch_names_short_ids_and_head_are_rejected_as_refs() {
    let remotes = remotes!();
    acme(&remotes, &[("v1", &reqfile(&[("RULE", &say("ran"))]))]);
    let repo = consumer(&remotes);

    for (reference, message) in [
        (
            "main",
            "has no tag main; pin requirements to a tag or a full commit id, never a branch",
        ),
        ("3f2a9c1", "`3f2a9c1` looks like a short commit id"),
        ("HEAD", "`HEAD` is not a tag or a full commit id"),
    ] {
        repo.write(
            "Reqfile.yaml",
            &use_block("code", "RULE", &format!("acme/reqs@{reference}")),
        );

        let run = repo.run(&["check"]);

        assert_eq!(run.code, 3, "{reference}: {}", run.output());
        assert!(run.stdout.contains(message), "{reference}: {}", run.stdout);
    }
}

#[test]
fn a_tag_resolves_through_refs_tags_even_if_a_branch_has_the_same_name() {
    let remotes = remotes!();
    let (source, _) = acme(
        &remotes,
        &[("release", &reqfile(&[("RULE", &say("tagged"))]))],
    );
    source.git(&["checkout", "-q", "-b", "release"]);
    source
        .write("Reqfile.yaml", &reqfile(&[("RULE", &say("branch"))]))
        .commit("on the branch");
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "RULE", "acme/reqs@release"),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  tagged"), "{}", run.stdout);
    assert!(!run.stdout.contains("RULE  branch"), "{}", run.stdout);
}

#[test]
fn a_missing_commit_is_an_error_naming_the_source() {
    let remotes = remotes!();
    acme(&remotes, &[("v1", &reqfile(&[("RULE", &say("ran"))]))]);
    let repo = consumer(&remotes);
    let missing = "0123456789abcdef0123456789abcdef01234567";
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "RULE", &format!("acme/reqs@{missing}")),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(&format!(
            "cannot fetch acme/reqs@{missing}: commit {missing} is not in"
        )),
        "{}",
        run.stdout
    );
}

#[test]
fn each_run_reports_the_commit_every_ref_resolved_to() {
    // The same id at two versions: the root at v1, services/api at v2.
    let jev = FakeJev::by_marker();
    let remotes = remotes!();
    let definition = reqfile(&[("FAIL_FAST", "- decision")]);
    let source = remotes.repo("acme/reqs");
    let mut commits = Vec::new();
    for (tag, question) in [("v1", "Q1?"), ("v2", "Q2?")] {
        source.write("Reqfile.yaml", &definition);
        source.write(".reqfile/FAIL_FAST/decision.yaml", &asking(question));
        source.commit(tag);
        source.git(&["tag", tag]);
        commits.push(source.head());
    }
    let repo = consumer(&remotes);
    repo.write(
        "Reqfile.yaml",
        &use_block("code", "FAIL_FAST", "acme/reqs@v1"),
    );
    repo.write(
        "services/api/Reqfile.yaml",
        &use_block("code", "FAIL_FAST", "acme/reqs@v2"),
    );
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");
    repo.write("services/api/b.py", "def g():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    for (tag, commit) in ["v1", "v2"].iter().zip(&commits) {
        assert!(
            run.stdout.contains(&format!(
                "source  acme/reqs@{tag} resolved to commit {commit}\n"
            )),
            "{}",
            run.stdout
        );
    }
    assert!(run.stdout.contains("2 advisory findings"), "{}", run.stdout);
    assert_eq!(
        asked(&jev, "FAIL_FAST"),
        vec![
            ("a.py".to_string(), "Q1?".to_string()),
            ("services/api/b.py".to_string(), "Q2?".to_string()),
        ]
    );
    let json = repo.run(&["check", "--format", "json"]).json();
    assert_eq!(json["sources"][0]["location"], "acme/reqs@v1");
    assert_eq!(json["sources"][0]["commit"], commits[0].as_str());
    assert_eq!(json["sources"][0]["offline"], false);
    let explain = repo.run(&["explain", "services/api/b.py"]);
    assert!(
        explain.stdout.contains(&format!(
            "From services/api/Reqfile.yaml:\n  FAIL_FAST (code)\n    must: Something.\n    why: A reason.\n    use: acme/reqs@v2 (Reqfile.yaml:3 at commit {})",
            commits[1]
        )),
        "{}",
        explain.stdout
    );
}

#[test]
fn tags_are_resolved_again_on_each_run_and_the_cache_is_used_only_offline() {
    let remotes = remotes!();
    let (source, commits) = acme(
        &remotes,
        &[
            ("v1", &reqfile(&[("RULE", &say("first"))])),
            ("v2", &reqfile(&[("RULE", &say("second"))])),
        ],
    );
    let repo = consumer(&remotes);
    repo.write("Reqfile.yaml", &use_block("code", "RULE", "acme/reqs@v1"));

    let run = repo.run(&["check"]);
    assert!(run.stdout.contains("RULE  first"), "{}", run.stdout);

    source.git(&["tag", "-f", "v1", &commits[1]]);

    let run = repo.run(&["check"]);
    assert!(run.stdout.contains("RULE  second"), "{}", run.stdout);
    assert!(
        run.stdout.contains(&format!(
            "source  acme/reqs@v1 resolved to commit {}\n",
            commits[1]
        )),
        "{}",
        run.stdout
    );

    let path = remotes.base().join("acme/reqs");
    std::fs::rename(&path, remotes.base().join("acme/gone")).expect("take the remote away");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  second"), "{}", run.stdout);
    assert!(
        run.stdout.contains(&format!(
            "source  acme/reqs@v1 resolved to commit {} (cannot be reached: using the last resolution cached here)",
            commits[1]
        )),
        "{}",
        run.stdout
    );
    let json = repo.run(&["check", "--format", "json"]).json();
    assert_eq!(json["sources"][0]["offline"], true);
}

#[test]
fn imported_checks_run_in_the_folder_of_the_use_block() {
    let remotes = remotes!();
    acme(
        &remotes,
        &[(
            "v1",
            &reqfile(&[(
                "RULE",
                "- command:\n    run: \"fail() { echo \\\"in $(basename \\\"$(pwd)\\\") got $*\\\"; exit 1; }; fail\"\n    files: \"*.txt\"\n    pass_files: true\n    fix_hint: None.",
            )]),
        )],
    );
    let repo = consumer(&remotes);
    repo.write(
        "app/Reqfile.yaml",
        &use_block("code", "RULE", "acme/reqs@v1"),
    );
    repo.write("app/x.txt", "").write("top.txt", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("RULE  in app got x.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn why_and_who_can_be_set_locally() {
    let repo = repo!();
    repo.write("lib/Reqfile.yaml", &reqfile(&[("RULE", &say("ran"))]));
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: RULE\n    use: ./lib\n    why: Local why.\n    who: Local who.\n",
    );

    let explain = repo.run(&["explain", "a.txt"]);

    assert_eq!(explain.code, 0, "{}", explain.output());
    assert!(
        explain
            .stdout
            .contains("    must: Something.\n    why: Local why.\n"),
        "{}",
        explain.stdout
    );
    let json = repo.run(&["list", "--format", "json"]).json();
    let root = json["requirements"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|r| r["reqfile"] == "Reqfile.yaml")
        .expect("the use block")
        .clone();
    assert_eq!(root["why"], "Local why.");
    assert_eq!(root["who"], "Local who.");
    assert_eq!(root["must"], "Something.");
}

#[test]
fn local_checks_replace_the_definitions_checks_entirely() {
    let repo = repo!();
    repo.write(
        "lib/Reqfile.yaml",
        &reqfile(&[("RULE", &format!("{}\n{}", say("DEF1"), say("DEF2")))]),
    );
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: RULE\n    use: ./lib\n    checks:\n      - command:\n          run: echo LOCAL; exit 1\n          fix_hint: None.\n",
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  LOCAL"), "{}", run.stdout);
    // lib's own block still runs the definition's checks on lib/.
    assert_eq!(run.stdout.matches("DEF1").count(), 1, "{}", run.stdout);
    assert!(
        run.stdout.contains("3 checks run"),
        "the root runs one check, lib two: {}",
        run.stdout
    );
}

#[test]
fn reqfile_assets_points_to_the_definitions_folder_for_inherited_checks() {
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "RULE",
            "- command:\n    run: cat \"$REQFILE_ASSETS/marker\"; exit 1\n    fix_hint: None.",
        )]),
    );
    source.write(".reqfile/RULE/marker", "from the source\n");
    source.commit("s1");
    source.git(&["tag", "v1"]);
    let repo = consumer(&remotes);
    repo.write("Reqfile.yaml", &use_block("code", "RULE", "acme/reqs@v1"));

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains("RULE  from the source"),
        "{}",
        run.stdout
    );

    // A folder source: the definition's folder in this repository.
    let repo = repo!();
    repo.write(
        "lib/Reqfile.yaml",
        &reqfile(&[(
            "RULE",
            "- command:\n    run: echo \"$REQFILE_ASSETS\"; cat \"$REQFILE_ASSETS/marker\"; exit 1\n    fix_hint: None.",
        )]),
    );
    repo.write("lib/.reqfile/RULE/marker", "from lib\n");
    repo.write("app/Reqfile.yaml", &use_block("code", "RULE", "../lib"));

    let run = repo.run(&["check", "--only", "RULE"]);

    let expected = canonical(repo.path()).join("lib/.reqfile/RULE");
    assert_eq!(
        run.stdout
            .matches(&format!("RULE  {}\n    from lib", expected.display()))
            .count(),
        2,
        "both blocks read lib's files: {}",
        run.stdout
    );
}

#[test]
fn reqfile_assets_points_to_the_local_folder_for_local_checks() {
    let repo = repo!();
    repo.write("lib/Reqfile.yaml", &reqfile(&[("RULE", &say("DEF"))]));
    repo.write("lib/.reqfile/RULE/marker", "from lib\n");
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: RULE\n    use: ./lib\n    checks:\n      - command:\n          run: cat \"$REQFILE_ASSETS/marker\"; exit 1\n          fix_hint: None.\n",
    );
    repo.write(".reqfile/RULE/marker", "from the root\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("RULE  from the root"), "{}", run.stdout);
    assert!(!run.stdout.contains("RULE  from lib"), "{}", run.stdout);
}

#[test]
fn local_assets_other_than_examples_with_inherited_checks_are_an_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &use_block("code", "FAIL_FAST", "./std"));
    repo.write("std/Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    repo.write("std/.reqfile/FAIL_FAST/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(
        ".reqfile/FAIL_FAST/examples/ok-clean/a.py",
        "def f():\n    pass\n",
    );
    repo.write("a.py", "def f():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "examples alone are allowed: {}", run.output());

    repo.write(".reqfile/FAIL_FAST/decision.yaml", PYTHON_FUNCTIONS);

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains(
            "Reqfile.yaml:3: requirement FAIL_FAST takes its checks from its definition, whose files are in its own .reqfile/FAIL_FAST/, so .reqfile/FAIL_FAST/ may only hold examples/; found .reqfile/FAIL_FAST/decision.yaml"
        ),
        "{}",
        run.stdout
    );
}

/// Runs `reqfile check` `times` times at once in each folder, and returns
/// the outputs per folder.
fn concurrently(repo: &Repo, folders: &[&Path], times: usize) -> Vec<Vec<Run>> {
    std::thread::scope(|scope| {
        let handles: Vec<Vec<_>> = folders
            .iter()
            .map(|folder| {
                (0..times)
                    .map(|_| {
                        let folder = folder.to_string_lossy().into_owned();
                        scope.spawn(move || repo.run_in(&folder, &["check"]))
                    })
                    .collect()
            })
            .collect();
        handles
            .into_iter()
            .map(|runs| runs.into_iter().map(|h| h.join().expect("a run")).collect())
            .collect()
    })
}

/// A second worktree of `repo` on a new branch, outside the repository.
fn worktree(repo: &Repo) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a folder");
    let path = dir.path().join("b");
    repo.git(&["worktree", "add", "-q", "-b", "b", &path.to_string_lossy()]);
    (dir, canonical(&path))
}

#[test]
fn two_worktrees_with_different_local_assets_never_affect_each_other() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("std/Reqfile.yaml", &reqfile(&[("FAIL_FAST", &say("DEF"))]));
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: FAIL_FAST\n    use: ./std\n    checks:\n      - decision\n      - command:\n          run: echo \"assets=$REQFILE_ASSETS\"; exit 1\n          fix_hint: None.\n",
    );
    let thresholds = |above: &str| {
        PYTHON_FUNCTIONS.replace("violation_above: 0.8", &format!("violation_above: {above}"))
    };
    repo.write(".reqfile/FAIL_FAST/decision.yaml", &thresholds("0.9"));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass  # P80\n");
    repo.commit("setup");
    repo.env("OPENROUTER_API_KEY", "key");
    let (_dir, b) = worktree(&repo);
    std::fs::write(
        b.join(".reqfile/FAIL_FAST/decision.yaml"),
        thresholds("0.6"),
    )
    .expect("write B's thresholds");
    let a = canonical(repo.path());
    let sequential_a = repo.run_in(&a.to_string_lossy(), &["check"]);
    let sequential_b = repo.run_in(&b.to_string_lossy(), &["check"]);
    assert!(
        sequential_a
            .stdout
            .contains("uncertain  FAIL_FAST  a.py:1  p=0.80"),
        "{}",
        sequential_a.stdout
    );
    assert!(
        sequential_b
            .stdout
            .contains("advisory  FAIL_FAST  a.py:1  p=0.80"),
        "{}",
        sequential_b.stdout
    );
    assert!(
        sequential_a.stdout.contains(&format!(
            "assets={}",
            a.join(".reqfile/FAIL_FAST").display()
        )),
        "{}",
        sequential_a.stdout
    );
    assert!(
        sequential_b.stdout.contains(&format!(
            "assets={}",
            b.join(".reqfile/FAIL_FAST").display()
        )),
        "{}",
        sequential_b.stdout
    );

    let runs = concurrently(&repo, &[&a, &b], 20);

    for (runs, sequential) in runs.iter().zip([&sequential_a, &sequential_b]) {
        for run in runs {
            assert_eq!(run.code, sequential.code, "{}", run.output());
            assert_eq!(run.stdout, sequential.stdout);
        }
    }
}

#[test]
fn two_worktrees_pinning_different_commits_never_affect_each_other() {
    let remotes = remotes!();
    let (_, commits) = acme(
        &remotes,
        &[
            ("v1", &reqfile(&[("RULE", &say("Q1"))])),
            ("v2", &reqfile(&[("RULE", &say("Q2"))])),
        ],
    );
    let repo = consumer(&remotes);
    let pinned = |commit: &str| use_block("code", "RULE", &format!("acme/reqs@{commit}"));
    repo.write("Reqfile.yaml", &pinned(&commits[0]))
        .commit("pin S1");
    let (_dir, b) = worktree(&repo);
    std::fs::write(b.join("Reqfile.yaml"), pinned(&commits[1])).expect("pin S2 in B");
    let a = canonical(repo.path());

    let runs = concurrently(&repo, &[&a, &b], 20);

    for (runs, (expected, other)) in runs.iter().zip([("Q1", "Q2"), ("Q2", "Q1")]) {
        for run in runs {
            assert_eq!(run.code, 1, "{}", run.output());
            assert!(
                run.stdout.contains(&format!("RULE  {expected}")),
                "{}",
                run.stdout
            );
            assert!(!run.stdout.contains(other), "{}", run.stdout);
        }
    }
}

/// `acme/reqs@v1` defining a code, a process and a product requirement,
/// whose checks would leave a file behind if they ran.
fn acme_with_every_type(remotes: &Remotes) -> String {
    let command = "      - command:\n          run: touch ran-marker\n          fix_hint: x\n";
    let block = |id: &str| {
        format!("  - id: {id}\n    must: Must {id}.\n    why: Because.\n    checks:\n{command}")
    };
    let content = format!(
        "reqfile: 1\nproduct:\n{}code:\n{}process:\n{}",
        block("FAST"),
        block("NO_TODO"),
        block("REVIEWED")
    );
    let (_, commits) = acme(remotes, &[("v1", &content)]);
    commits[0].clone()
}

#[test]
fn add_prints_use_blocks_under_the_matching_section_and_runs_nothing() {
    let remotes = remotes!();
    let commit = acme_with_every_type(&remotes);
    let repo = consumer(&remotes);
    repo.write("app/main.txt", "");

    let run = repo.run_in("app", &["add", "acme/reqs@v1", "NO_TODO", "REVIEWED"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        format!(
            "acme/reqs@v1 resolved to commit {commit}.\n\
             \nAdd to app/Reqfile.yaml:\n\
             \ncode:\n  - {{ id: NO_TODO, use: acme/reqs@v1 }}\n\
             \nprocess:\n  - {{ id: REVIEWED, use: acme/reqs@v1 }}\n\
             \nTheir checks will run, in the folder of that Reqfile:\n  NO_TODO  command: touch ran-marker\n  REVIEWED  command: touch ran-marker\n\
             \nThen verify them on their examples and on this code:\n  reqfile test --only NO_TODO,REVIEWED\n  reqfile check --only NO_TODO,REVIEWED\n"
        )
    );
    assert!(!repo.path().join("app/Reqfile.yaml").exists());
    assert!(!repo.path().join("app/ran-marker").exists());
    assert!(!repo.path().join("ran-marker").exists());

    let run = repo.run_in("app", &["add", "acme/reqs@v1", "FAST"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr.contains("FAST is a product requirement"),
        "{}",
        run.stderr
    );
}

#[test]
fn add_without_ids_adds_every_importable_definition_of_the_location() {
    let remotes = remotes!();
    acme_with_every_type(&remotes);
    let repo = consumer(&remotes);

    let run = repo.run(&["add", "acme/reqs@v1"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("\ncode:\n  - { id: NO_TODO, use: acme/reqs@v1 }\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("\nprocess:\n  - { id: REVIEWED, use: acme/reqs@v1 }\n"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("FAST"), "{}", run.stdout);

    // A folder: relative to the current folder.
    repo.write("lib/Reqfile.yaml", &reqfile(&[("LOCAL_RULE", &say("ran"))]));
    repo.write("app/main.txt", "");

    let run = repo.run_in("app", &["add", "../lib"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("\ncode:\n  - { id: LOCAL_RULE, use: ../lib }\n"),
        "{}",
        run.stdout
    );
}
