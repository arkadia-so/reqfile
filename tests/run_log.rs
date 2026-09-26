//! RUN_LOG: `reqfile check --log FILE` records every judgment, clean ones included.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Repo, repo, reqfile};

fn lines(repo: &Repo, path: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(repo.path().join(path))
        .expect("the log exists")
        .lines()
        .map(|l| serde_json::from_str(l).expect("each line is JSON"))
        .collect()
}

fn decision_repo(jev: &FakeJev) -> Repo {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[
            ("DECOMPLECT", "- decision"),
            ("NO_TODO", "- command:\n    run: \"grep -n TODO \\\"$@\\\" && exit 1 || exit 0\"\n    files: \"**/*.py\"\n    pass_files: true\n    fix_hint: x"),
        ]),
    );
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write(
        "a.py",
        "def clean():\n    pass\n\n\ndef mixed():\n    pass  # VIOLATION TODO\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");
    repo
}

#[test]
fn every_judged_unit_is_logged_clean_ones_included() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);
    repo.commit("code");

    let run = repo.run(&["check", "--log", "logs/run.jsonl"]);

    let lines = lines(&repo, "logs/run.jsonl");
    let units: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|l| l["kind"] == "decision_unit")
        .collect();
    assert_eq!(units.len(), 2, "{lines:?}");
    let clean = units
        .iter()
        .find(|u| u["line"] == 1)
        .expect("the clean unit");
    assert_eq!(clean["verdict"], "pass");
    assert_eq!(clean["probability"], 0.05);
    assert_eq!(clean["unit_source"], "def clean():\n    pass");
    assert_eq!(clean["model"], "typesafe/jev-1.13-20260917");
    assert_eq!(clean["cached"], false);
    assert_eq!(clean["requirement"], "DECOMPLECT");
    assert_eq!(clean["language"], "python");
    let flagged = units
        .iter()
        .find(|u| u["line"] == 5)
        .expect("the flagged unit");
    assert_eq!(flagged["verdict"], "advisory");
    assert_ne!(clean["unit_fingerprint"], flagged["unit_fingerprint"]);
    assert_eq!(
        clean["question_fingerprint"],
        flagged["question_fingerprint"]
    );
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert!(
        lines
            .iter()
            .all(|l| l["git_head"] == head.trim() && l["run_id"] == lines[0]["run_id"])
    );
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:5"),
        "the log changes nothing else: {}",
        run.stdout
    );
}

#[test]
fn command_checks_and_their_findings_are_logged() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);

    let run = repo.run(&["check", "--log", "run.jsonl"]);

    assert_eq!(run.code, 1, "{}", run.output());
    let lines = lines(&repo, "run.jsonl");
    let check = lines
        .iter()
        .find(|l| l["kind"] == "command_check")
        .expect("a command check line");
    assert_eq!(check["requirement"], "NO_TODO");
    assert_eq!(check["status"], "violations");
    assert_eq!(check["findings"], 1);
    let finding = lines
        .iter()
        .find(|l| l["kind"] == "command_finding")
        .expect("a finding line");
    assert!(
        finding["message"]
            .as_str()
            .unwrap_or_default()
            .contains("TODO"),
        "{finding}"
    );
}

#[test]
fn runs_append_and_cached_answers_say_so() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);

    repo.run(&["check", "--log", "run.jsonl"]);
    repo.run(&["check", "--log", "run.jsonl"]);

    let lines = lines(&repo, "run.jsonl");
    let runs: std::collections::BTreeSet<String> =
        lines.iter().map(|l| l["run_id"].to_string()).collect();
    assert_eq!(runs.len(), 2);
    let second = lines.last().expect("a line");
    let cached: Vec<bool> = lines
        .iter()
        .filter(|l| l["run_id"] == second["run_id"] && l["kind"] == "decision_unit")
        .map(|l| l["cached"].as_bool().unwrap_or(false))
        .collect();
    assert_eq!(cached, [true, true]);
}

#[test]
fn tags_mark_every_line() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);

    repo.run(&[
        "check",
        "--fast",
        "--log",
        "run.jsonl",
        "--log-tag",
        "session_id=abc",
        "--log-tag",
        "hook=Stop",
    ]);

    let lines = lines(&repo, "run.jsonl");
    assert!(!lines.is_empty());
    for line in &lines {
        assert_eq!(line["tags"]["session_id"], "abc");
        assert_eq!(line["tags"]["hook"], "Stop");
        assert_eq!(line["fast"], true);
    }
}

#[test]
fn a_malformed_tag_is_a_config_error() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);

    let run = repo.run(&["check", "--log", "run.jsonl", "--log-tag", "no-equals"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("--log-tag must be KEY=VALUE"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_log_that_cannot_be_written_is_an_error() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);
    repo.write("blocked", "a file where a folder should be");

    let run = repo.run(&["check", "--log", "blocked/run.jsonl"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("cannot write the run log"),
        "{}",
        run.stdout
    );
}

#[test]
fn without_log_nothing_is_written() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev);

    repo.run(&["check"]);

    assert!(!repo.path().join("run.jsonl").exists());
}
