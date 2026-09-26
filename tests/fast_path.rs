//! FAST_PATH: `reqfile check --fast` runs decision checks and the commands
//! declared fast, and says what it left for a full run.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, repo, reqfile};

const FAST: &str = "- command:\n    run: echo fast; exit 1\n    fast: true\n    fix_hint: x";
const SLOW: &str = "- command:\n    run: echo slow; exit 1\n    fix_hint: x";

#[test]
fn fast_runs_only_declared_fast_commands_and_counts_the_rest() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("QUICK", FAST), ("TESTS", SLOW)]),
    );

    let run = repo.run(&["check", "--fast"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("QUICK  fast"), "{}", run.stdout);
    assert!(!run.stdout.contains("TESTS"), "{}", run.stdout);
    assert!(
        run.stdout.ends_with(
            "1 check run, 0 with nothing to check, 1 left for a full run. 1 violation, 0 advisory findings, 0 errors.\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn a_full_run_is_the_default() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("QUICK", FAST), ("TESTS", SLOW)]),
    );

    let run = repo.run(&["check"]);

    assert!(run.stdout.contains("QUICK  fast"), "{}", run.stdout);
    assert!(run.stdout.contains("TESTS  slow"), "{}", run.stdout);
    assert!(
        !run.stdout.contains("left for a full run"),
        "{}",
        run.stdout
    );
}

#[test]
fn decision_checks_are_on_the_fast_path() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("TESTS", SLOW)]),
    );
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["check", "--fast"]);

    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:1"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("1 left for a full run"),
        "{}",
        run.stdout
    );
}

#[test]
fn json_output_counts_the_checks_left_for_a_full_run() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("QUICK", FAST), ("TESTS", SLOW)]),
    );

    let run = repo.run(&["check", "--fast", "--format", "json"]);

    assert_eq!(run.json()["summary"]["checks_left_for_full_run"], 1);
}

#[test]
fn fast_must_be_a_boolean() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("QUICK", &FAST.replace("fast: true", "fast: yes please"))]),
    );

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("`fast` must be true or false"),
        "{}",
        run.stdout
    );
}

#[test]
fn skipped_checks_do_not_need_a_base() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "QUICK",
            &FAST.replace(
                "run: echo fast; exit 1",
                "run: echo fast; exit 1\n    files: \"**/*.txt\"",
            ),
        )]),
    );
    repo.write(".reqfile/config.yaml", "base: main\n");
    repo.write("slow/Reqfile.yaml", &reqfile(&[("TESTS", SLOW)]));
    repo.write("slow/.reqfile/config.yaml", "base: origin/nowhere\n")
        .commit("setup");
    repo.write("new.txt", "");

    let run = repo.run(&["check", "--fast", "--changed"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("QUICK  fast"), "{}", run.stdout);
}
