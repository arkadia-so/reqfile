//! SELECTION: `--changed [base]` and `--only ID,...`.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Repo, repo, reqfile};

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
