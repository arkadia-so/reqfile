//! SELECTION: `--changed [base]` and `--only ID,...`.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Repo, remotes, repo, reqfile};

const LIST_TXT: &str = r#"- command:
    run: "fail() { echo \"got $*\"; exit 1; }; fail"
    files: "**/*.txt"
    pass_files: true
    fix_hint: None."#;

const ANY_TXT: &str = r#"- command:
    run: echo triggered; exit 1
    files: "**/*.txt"
    fix_hint: None."#;

const ALWAYS: &str = "- command:\n    run: echo always; exit 1\n    fix_hint: None.";

/// A repository with committed `old.txt`, `kept.txt` and `gone.txt` on
/// main, and a `feature` branch checked out.
fn branch_repo(checks: &[(&str, &str)]) -> Repo {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(checks));
    repo.write("old.txt", "")
        .write("kept.txt", "")
        .write("gone.txt", "")
        .commit("base");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo
}

#[test]
fn changed_restricts_file_checks_to_files_changed_since_the_merge_base() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);
    repo.write("committed.txt", "").commit("on feature");
    repo.write("kept.txt", "staged").git(&["add", "kept.txt"]);
    repo.write("old.txt", "unstaged");
    repo.write("untracked.txt", "");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout
            .contains("LIST  got committed.txt kept.txt old.txt untracked.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn changes_on_the_base_after_the_merge_base_are_ignored() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);
    repo.write("feature.txt", "").commit("on feature");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main-only.txt", "").commit("on main");
    repo.git(&["checkout", "-q", "feature"]);

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("LIST  got feature.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn deleted_files_trigger_commands_that_do_not_take_files() {
    let repo = branch_repo(&[("LIST", LIST_TXT), ("TRIGGERED", ANY_TXT)]);
    repo.remove("gone.txt");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("TRIGGERED  triggered"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("LIST"), "{}", run.stdout);
    assert!(
        run.stdout.contains("1 check run, 1 with nothing to check."),
        "{}",
        run.stdout
    );
}

#[test]
fn commands_without_files_still_run() {
    let repo = branch_repo(&[("ALWAYS", ALWAYS), ("LIST", LIST_TXT)]);

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(run.stdout.contains("ALWAYS  always"), "{}", run.stdout);
    assert!(
        run.stdout.contains("1 check run, 1 with nothing to check."),
        "{}",
        run.stdout
    );
}

#[test]
fn the_base_defaults_to_the_config() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);
    repo.write(".reqfile/config.yaml", "base: main\n")
        .commit("config");
    repo.git(&["branch", "-f", "main"]);
    repo.write("new.txt", "");

    let run = repo.run(&["check", "--changed"]);

    assert!(run.stdout.contains("LIST  got new.txt\n"), "{}", run.stdout);
}

#[test]
fn changed_accepts_other_options_after_it() {
    let repo = branch_repo(&[("LIST", LIST_TXT), ("ALWAYS", ALWAYS)]);
    repo.write(".reqfile/config.yaml", "base: main\n")
        .commit("config");
    repo.git(&["branch", "-f", "main"]);
    repo.write("new.txt", "");

    let run = repo.run(&["check", "--changed", "--only", "LIST"]);

    assert!(run.stdout.contains("LIST  got new.txt\n"), "{}", run.stdout);
    assert!(!run.stdout.contains("ALWAYS"), "{}", run.stdout);
}

#[test]
fn changed_without_a_base_is_a_config_error() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);

    let run = repo.run(&["check", "--changed"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("--changed needs a base"),
        "{}",
        run.stdout
    );
}

#[test]
fn an_unknown_base_is_an_error() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);

    let run = repo.run(&["check", "--changed", "origin/nowhere"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("cannot find the merge-base of origin/nowhere and HEAD"),
        "{}",
        run.stdout
    );
}

#[test]
fn decision_units_come_from_changed_files_only() {
    let jev = FakeJev::by_marker();
    let repo = branch_repo(&[("DECOMPLECT", "- decision")]);
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("old.py", "def old():\n    pass  # VIOLATION\n")
        .commit("old code");
    repo.git(&["branch", "-f", "main"]);
    repo.write("new.py", "def new():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("DECOMPLECT  new.py:1"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("old.py"), "{}", run.stdout);
    assert_eq!(jev.questions_received().len(), 1);
}

#[test]
fn only_runs_the_listed_requirements() {
    let repo = repo!();
    let fails =
        |name: &str| format!("- command:\n    run: echo {name}; exit 1\n    fix_hint: None.");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            ("FIRST", &fails("first")),
            ("SECOND", &fails("second")),
            ("THIRD", &fails("third")),
        ]),
    );

    let run = repo.run(&["check", "--only", "FIRST,THIRD"]);

    assert!(run.stdout.contains("FIRST  first"), "{}", run.stdout);
    assert!(run.stdout.contains("THIRD  third"), "{}", run.stdout);
    assert!(!run.stdout.contains("SECOND"), "{}", run.stdout);
    assert!(run.stdout.contains("2 checks run"), "{}", run.stdout);
}

#[test]
fn only_with_an_unknown_id_is_a_config_error() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("FIRST", ALWAYS)]));

    let run = repo.run(&["check", "--only", "FIRST,TYPO"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout
            .contains("--only: no requirement has the id TYPO"),
        "{}",
        run.stdout
    );
}

#[test]
fn only_resolves_bases_for_the_selected_requirements() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write(".reqfile/config.yaml", "base: main\n");
    repo.write("other/Reqfile.yaml", &reqfile(&[("OTHER", ALWAYS)]));
    repo.write("other/.reqfile/config.yaml", "base: origin/nowhere\n");
    repo.write("new.txt", "");

    let run = repo.run(&["check", "--changed", "--only", "LIST"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("LIST  got new.txt"), "{}", run.stdout);
}

#[test]
fn a_changed_requirement_is_checked_on_its_whole_scope() {
    let repo = branch_repo(&[
        ("LIST", LIST_TXT),
        ("OTHER", &LIST_TXT.replace("got", "other")),
    ]);
    repo.write("new.txt", "");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            ("LIST", &LIST_TXT.replace("None.", "Changed hint.")),
            ("OTHER", &LIST_TXT.replace("got", "other")),
        ]),
    );

    let run = repo.run(&["check", "--changed", "main"]);

    // The Reqfile changed, so both of its requirements see every file.
    assert!(
        run.stdout
            .contains("LIST  got gone.txt kept.txt new.txt old.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_changed_decision_question_is_asked_about_unchanged_code() {
    let jev = FakeJev::by_marker();
    let repo = branch_repo(&[("DECOMPLECT", "- decision"), ("LIST", LIST_TXT)]);
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("old.py", "def old():\n    pass  # VIOLATION\n")
        .commit("old code");
    repo.git(&["branch", "-f", "main"]);
    repo.write(
        ".reqfile/DECOMPLECT/decision.yaml",
        &PYTHON_FUNCTIONS.replace("It mixes several jobs.", "It mixes jobs."),
    );
    repo.write("new.txt", "");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("DECOMPLECT  old.py:1"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("LIST  got new.txt\n"),
        "unrelated requirements stay incremental: {}",
        run.stdout
    );
}

#[test]
fn a_changed_config_above_a_requirement_widens_it() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);
    repo.write("new.txt", "");
    repo.write(".reqfile/config.yaml", "exclude: [\"gone.txt\"]\n");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("LIST  got kept.txt new.txt old.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_nested_config_change_widens_the_requirements_above_it() {
    let repo = branch_repo(&[("LIST", LIST_TXT)]);
    repo.write("app/.reqfile/config.yaml", "exclude: [\"old.txt\"]\n");
    repo.write("app/old.txt", "")
        .commit("app with an exclusion");
    repo.git(&["branch", "-f", "main"]);
    // Removing the exclusion brings app/old.txt into the scope of the root requirement.
    repo.write("app/.reqfile/config.yaml", "exclude: []\n");

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(run.stdout.contains("app/old.txt"), "{}", run.stdout);
}

#[test]
fn a_widened_requirement_still_sees_deleted_files() {
    let repo = branch_repo(&[("TRIGGERED", ANY_TXT)]);
    repo.remove("gone.txt");
    repo.remove("old.txt");
    repo.remove("kept.txt");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("TRIGGERED", &ANY_TXT.replace("None.", "Changed hint."))]),
    );

    let run = repo.run(&["check", "--changed", "main"]);

    assert!(
        run.stdout.contains("TRIGGERED  triggered"),
        "{}",
        run.stdout
    );
}

/// PYTHON_FUNCTIONS asking `question` instead.
fn asking(question: &str) -> String {
    PYTHON_FUNCTIONS.replace("Does this function do more than one job?", question)
}

/// The (path, question, model) of every question Jev received, sorted.
fn asked(jev: &FakeJev) -> Vec<(String, String, String)> {
    let mut asked: Vec<(String, String, String)> = jev
        .questions_received()
        .iter()
        .flat_map(|r| {
            let path = r.body["state"]["path"]
                .as_str()
                .expect("a path")
                .to_string();
            let model = r.body["model"].as_str().expect("a model").to_string();
            r.body["questions"]
                .as_object()
                .expect("questions")
                .values()
                .map(move |q| {
                    (
                        path.clone(),
                        q["instructions"]["question"]
                            .as_str()
                            .expect("a question")
                            .to_string(),
                        model.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    asked.sort();
    asked
}

fn paths_and_questions(jev: &FakeJev) -> Vec<(String, String)> {
    asked(jev).into_iter().map(|(p, q, _)| (p, q)).collect()
}

fn pair(path: &str, question: &str) -> (String, String) {
    (path.to_string(), question.to_string())
}

/// Commits everything on `main`, then works on a `feature` branch.
fn branch(repo: &Repo) {
    repo.commit("base");
    repo.git(&["checkout", "-q", "-b", "feature"]);
}

/// The root takes FAIL_FAST from std/, which defines it asking Q1, with a
/// unit in src/ and one in std/tools/.
fn local_source_repo(jev: &FakeJev) -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: FAIL_FAST, use: ./std }\n",
    );
    repo.write("std/Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    repo.write("std/.reqfile/FAIL_FAST/decision.yaml", &asking("Q1"));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("src/a.py", "def a():\n    pass\n");
    repo.write("std/tools/b.py", "def b():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");
    repo
}

#[test]
fn a_change_to_a_local_sources_assets_rechecks_every_block_resolving_to_it() {
    let jev = FakeJev::by_marker();
    let repo = local_source_repo(&jev);
    repo.write("notes.txt", "");
    branch(&repo);
    repo.write("std/.reqfile/FAIL_FAST/decision.yaml", &asking("Q1b"));

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![pair("src/a.py", "Q1b"), pair("std/tools/b.py", "Q1b")],
        "both the root block and std's definition use the edited files"
    );
}

/// root `use: ./oss/reqfile`, whose block uses `./std` defined in
/// oss/reqfile/std, with a unit in each of the three scopes.
fn transitive_repo(jev: &FakeJev) -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: FAIL_FAST, use: ./oss/reqfile }\n",
    );
    repo.write(
        "oss/reqfile/Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: FAIL_FAST, use: ./std }\n",
    );
    repo.write(
        "oss/reqfile/std/Reqfile.yaml",
        &reqfile(&[("FAIL_FAST", "- decision")]),
    );
    repo.write(
        "oss/reqfile/std/.reqfile/FAIL_FAST/decision.yaml",
        &asking("Q1"),
    );
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("src/a.py", "def a():\n    pass\n");
    repo.write("oss/reqfile/src/b.py", "def b():\n    pass\n");
    repo.write("oss/reqfile/std/c.py", "def c():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");
    branch(&repo);
    repo
}

#[test]
fn a_why_change_in_an_intermediate_use_block_rechecks_only_its_scope() {
    // Editing the definition's files rechecks every scope resolving to it.
    let jev = FakeJev::by_marker();
    let repo = transitive_repo(&jev);
    repo.write(
        "oss/reqfile/std/.reqfile/FAIL_FAST/decision.yaml",
        &asking("Q1b"),
    );
    let run = repo.run(&["check", "--changed", "main"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![
            pair("oss/reqfile/src/b.py", "Q1b"),
            pair("oss/reqfile/std/c.py", "Q1b"),
            pair("src/a.py", "Q1b"),
        ]
    );

    // Editing only the why of the intermediate block rechecks only its scope.
    let jev = FakeJev::by_marker();
    let repo = transitive_repo(&jev);
    repo.write(
        "oss/reqfile/Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: FAIL_FAST, use: ./std, why: Reqfile must not hide failures. }\n",
    );
    let run = repo.run(&["check", "--changed", "main"]);
    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![pair("oss/reqfile/src/b.py", "Q1")]
    );
}

/// The root defines FAIL_FAST asking Q1, with units at the root, in
/// services/api/ and in other/; acme/reqs defines it asking Q2.
fn shadowing_repo(jev: &FakeJev) -> (reqfile_test_support::Remotes, Repo, String) {
    let remotes = remotes!();
    let acme = remotes.repo("acme/reqs");
    acme.write("Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    acme.write(".reqfile/FAIL_FAST/decision.yaml", &asking("Q2"));
    acme.commit("S2");
    let s2 = acme.head();
    let repo = repo!();
    remotes.serve(&repo);
    repo.write("Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    repo.write(".reqfile/FAIL_FAST/decision.yaml", &asking("Q1"));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def a():\n    pass\n");
    repo.write("services/api/b.py", "def b():\n    pass\n");
    repo.write("other/c.py", "def c():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");
    (remotes, repo, s2)
}

fn shadow(s2: &str) -> String {
    format!("reqfile: 1\ncode:\n  - {{ id: FAIL_FAST, use: acme/reqs@{s2} }}\n")
}

#[test]
fn adding_a_shadowing_block_rechecks_its_subtree() {
    let jev = FakeJev::by_marker();
    let (_remotes, repo, s2) = shadowing_repo(&jev);
    branch(&repo);
    repo.write("services/api/Reqfile.yaml", &shadow(&s2));

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![pair("services/api/b.py", "Q2")]
    );
}

#[test]
fn removing_a_shadowing_block_rechecks_its_subtree_with_the_ancestor_block() {
    let jev = FakeJev::by_marker();
    let (_remotes, repo, s2) = shadowing_repo(&jev);
    repo.write("services/api/Reqfile.yaml", &shadow(&s2));
    branch(&repo);
    repo.remove("services/api/Reqfile.yaml");

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![pair("services/api/b.py", "Q1")]
    );
}

#[test]
fn settings_next_to_a_local_source_only_affect_that_sources_own_scope() {
    let jev = FakeJev::by_marker();
    let repo = local_source_repo(&jev);
    branch(&repo);
    repo.write(
        "std/.reqfile/config.yaml",
        "decision:\n  model: typesafe/jev-std\n",
    );

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        asked(&jev),
        vec![(
            "std/tools/b.py".to_string(),
            "Q1".to_string(),
            "typesafe/jev-std".to_string()
        )],
        "the root block keeps the root's settings"
    );
}

#[test]
fn a_pinned_ref_change_rechecks_the_whole_scope() {
    let jev = FakeJev::by_marker();
    let remotes = remotes!();
    let acme = remotes.repo("acme/reqs");
    acme.write("Reqfile.yaml", &reqfile(&[("FAIL_FAST", "- decision")]));
    acme.write(".reqfile/FAIL_FAST/decision.yaml", &asking("Q1"));
    acme.commit("S1");
    let s1 = acme.head();
    acme.write(".reqfile/FAIL_FAST/decision.yaml", &asking("Q2"));
    acme.commit("S2");
    let s2 = acme.head();
    let repo = repo!();
    remotes.serve(&repo);
    let using = |commit: &str| {
        format!("reqfile: 1\ncode:\n  - {{ id: FAIL_FAST, use: acme/reqs@{commit} }}\n")
    };
    repo.write("Reqfile.yaml", &using(&s1));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def a():\n    pass\n");
    repo.write("lib/b.py", "def b():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");
    branch(&repo);
    repo.write("Reqfile.yaml", &using(&s2));

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        paths_and_questions(&jev),
        vec![pair("a.py", "Q2"), pair("lib/b.py", "Q2")]
    );
}

#[test]
fn a_settings_change_rechecks_the_decision_checks_it_applies_to() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("LIST", LIST_TXT)]),
    );
    repo.write(".reqfile/DECOMPLECT/decision.yaml", &asking("Q1"));
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("notes.txt", "");
    repo.write("app/Reqfile.yaml", &reqfile(&[("APP_RULE", "- decision")]));
    repo.write("app/.reqfile/APP_RULE/decision.yaml", &asking("QA"));
    // app/ sets its own model, so the root's model does not apply to it.
    repo.write(
        "app/.reqfile/config.yaml",
        "decision:\n  model: typesafe/jev-app\n",
    );
    repo.write("a.py", "def a():\n    pass\n");
    repo.write("app/b.py", "def b():\n    pass\n");
    repo.env("OPENROUTER_API_KEY", "key");
    branch(&repo);
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  model: typesafe/jev-root\n", jev.config()),
    );

    let run = repo.run(&["check", "--changed", "main"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        asked(&jev),
        vec![
            (
                "a.py".to_string(),
                "Q1".to_string(),
                "typesafe/jev-root".to_string()
            ),
            (
                "app/b.py".to_string(),
                "Q1".to_string(),
                "typesafe/jev-root".to_string()
            ),
        ],
        "only the root requirement, whose settings changed, is asked again"
    );
    assert!(
        !run.stdout.contains("LIST"),
        "a model change leaves command checks incremental: {}",
        run.stdout
    );
}
