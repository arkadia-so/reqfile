//! EVALS: `reqfile eval` measures each requirement's checks on its labeled examples,
//! `.reqfile/<ID>/examples/<name>/` with `example.yaml` and the case in `files/`.

use reqfile_test_support::{FakeJev, PYTHON_FUNCTIONS, Repo, repo, reqfile};

/// Flags any .txt file containing TODO, through a script of the requirement.
const NO_TODO: &str = r#"- command:
    run: $REQFILE_ASSETS/check.sh
    files: "**/*.txt"
    pass_files: true
    fix_hint: Remove the TODO."#;
const SCRIPT: &str = "#!/bin/sh\nif grep -l TODO \"$@\"; then exit 1; fi\n";

/// Writes one file of an example case: `path` is `…/examples/<case>/<file>`,
/// written as `…/examples/<case>/files/<file>`, with an `example.yaml`
/// expecting what the case name starts with (`violation-` or `ok-`).
fn write_case(repo: &Repo, path: &str, content: &str) {
    let (examples, rest) = path.split_once("/examples/").expect("an example path");
    let (case, file) = rest.split_once('/').expect("a file in the case");
    let expected = if case.starts_with("violation-") {
        "violation"
    } else {
        "ok"
    };
    repo.write(
        &format!("{examples}/examples/{case}/example.yaml"),
        &format!("expected: {expected}\n"),
    );
    repo.write(&format!("{examples}/examples/{case}/files/{file}"), content);
}

fn no_todo_repo() -> reqfile_test_support::Repo {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("NO_TODO", NO_TODO)]));
    repo.write(".reqfile/NO_TODO/check.sh", SCRIPT);
    std::process::Command::new("chmod")
        .args(["+x", ".reqfile/NO_TODO/check.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    repo
}

#[test]
fn examples_of_a_command_check_must_match_their_labels() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO: finish\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-nested/docs/plan.txt",
        "a TODO here\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/ok-done/notes.txt",
        "done\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "NO_TODO  asserted: 3 examples, 3 as labeled\n\n1 requirements: 1 asserted, 0 measured, 0 without evidence; 3 examples. 0 failing their labels.\n"
    );
}

#[test]
fn a_command_check_disagreeing_with_a_label_fails() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-lowercase/notes.txt",
        "todo: finish\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/ok-mentions-todo/notes.txt",
        "Never write TODO.\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-no-text-file/main.rs",
        "// TODO\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout
            .contains("NO_TODO  asserted: 3 examples, 0 as labeled\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  violation-lowercase: missed\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  ok-mentions-todo: false alarm\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("  violation-no-text-file: not selected\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn decision_checks_are_measured_rather_than_asserted() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    let examples = ".reqfile/DECOMPLECT/examples";
    write_case(
        &repo,
        &format!("{examples}/violation-caught/a.py"),
        "def f():\n    pass  # VIOLATION\n",
    );
    write_case(
        &repo,
        &format!("{examples}/violation-missed/a.py"),
        "def f():\n    pass\n",
    );
    write_case(
        &repo,
        &format!("{examples}/violation-unsure/a.py"),
        "def f():\n    pass  # UNSURE\n",
    );
    write_case(
        &repo,
        &format!("{examples}/violation-not-python/a.rs"),
        "fn f() {}\n",
    );
    write_case(
        &repo,
        &format!("{examples}/ok-clean/a.py"),
        "def f():\n    pass\n",
    );
    write_case(
        &repo,
        &format!("{examples}/ok-flagged/a.py"),
        "def f():\n    pass  # VIOLATION\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains(
            "DECOMPLECT  measured, no assertion: 1 of 4 violations caught (95% interval 5 to 70%; 1 missed, 1 not selected, 1 uncertain); 1 of 2 correct examples flagged (95% interval 9 to 91%; 0 uncertain)\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("  ok-flagged: false alarm\n"),
        "{}",
        run.stdout
    );
    assert_eq!(
        jev.questions_received().len(),
        5,
        "every selected example unit is asked"
    );
}

#[test]
fn a_case_that_cannot_run_is_an_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    write_case(
        &repo,
        ".reqfile/DECOMPLECT/examples/violation-x/a.py",
        "def f():\n    pass\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("  violation-x: error: 1 of 1 units could not be judged, first at a.py:1: OPENROUTER_API_KEY is not set"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_folder_without_example_yaml_is_an_error_pointing_to_the_new_format() {
    let repo = no_todo_repo();
    // The 0.2 layout: the label in the folder name, the case files beside it.
    repo.write(
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr.contains(
            ".reqfile/NO_TODO/examples/violation-todo: an example holds example.yaml (`expected: violation` or `expected: ok`) and its case in files/; the folder name no longer carries the label"
        ),
        "{}",
        run.stderr
    );
}

#[test]
fn examples_are_never_checked_as_code() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );
    repo.write("clean.txt", "fine\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
}

#[test]
fn only_tests_the_listed_requirements() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let run = repo.run(&["eval", "--only", "OTHER"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stderr
            .contains("--only: no requirement has the id OTHER"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_violation_a_command_examined_is_missed_even_if_no_unit_was_selected() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    let checks =
        "- command:\n    run: \"true\"\n    files: \"**/*.rs\"\n    fix_hint: x\n- decision";
    repo.write("Reqfile.yaml", &reqfile(&[("MIXED", checks)]));
    repo.write(".reqfile/MIXED/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    write_case(
        &repo,
        ".reqfile/MIXED/examples/violation-rust/a.rs",
        "fn f() {}\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["eval"]);

    assert!(
        run.stdout.contains("  violation-rust: missed\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_decision_requirement_with_every_example_missed_is_reported_as_measured() {
    let jev = FakeJev::start(|_| reqfile_test_support::Reply::Probability(0.05));
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    for case in ["violation-a", "violation-b", "violation-c"] {
        write_case(
            &repo,
            &format!(".reqfile/DECOMPLECT/examples/{case}/a.py"),
            "def f():\n    pass\n",
        );
    }
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.starts_with(
            "DECOMPLECT  measured, no assertion: 0 of 3 violations caught (95% interval 0 to 56%; 3 missed, 0 not selected, 0 uncertain)"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.ends_with(
            "1 requirements: 0 asserted, 1 measured, 0 without evidence; 3 examples. 0 failing their labels.\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn a_requirement_without_examples_is_reported_as_no_evidence() {
    let repo = no_todo_repo();

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "NO_TODO  no evidence: no labeled examples\n\n1 requirements: 0 asserted, 0 measured, 1 without evidence; 0 examples. 0 failing their labels.\n"
    );
}

#[test]
fn a_command_requirement_with_a_mismatched_label_exits_1() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-clean/notes.txt",
        "done\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.starts_with(
            "NO_TODO  asserted: 1 examples, 0 as labeled\n  violation-clean: missed\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn examples_reach_assets_only_through_reqfile_assets() {
    let repo = no_todo_repo();
    // The case holds only its own files: no Reqfile, no .reqfile/.
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            &NO_TODO.replace(
                "run: $REQFILE_ASSETS/check.sh",
                "run: test ! -e Reqfile.yaml && test ! -e .reqfile && $REQFILE_ASSETS/check.sh",
            ),
        )]),
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/ok-done/notes.txt",
        "done\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .starts_with("NO_TODO  asserted: 2 examples, 2 as labeled\n"),
        "{}",
        run.stdout
    );

    // A command reaching its files by a path next to the Reqfile finds nothing there.
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            &NO_TODO.replace("$REQFILE_ASSETS/check.sh", ".reqfile/NO_TODO/check.sh"),
        )]),
    );
    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 3, "{}", run.output());
}

#[test]
fn an_imported_requirement_is_tested_with_its_definitions_examples_and_local_examples() {
    let repo = repo!();
    repo.write("std/Reqfile.yaml", &reqfile(&[("NO_TODO", NO_TODO)]));
    repo.write("std/.reqfile/NO_TODO/check.sh", SCRIPT);
    std::process::Command::new("chmod")
        .args(["+x", "std/.reqfile/NO_TODO/check.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    write_case(
        &repo,
        "std/.reqfile/NO_TODO/examples/violation-from-definition/a.txt",
        "TODO\n",
    );
    repo.write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - { id: NO_TODO, use: ./std }\n",
    );
    write_case(&repo, ".reqfile/NO_TODO/examples/ok-local/a.txt", "done\n");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.starts_with(
            "NO_TODO (Reqfile.yaml)  asserted: 2 examples, 2 as labeled\nNO_TODO (std/Reqfile.yaml)  asserted: 1 examples, 1 as labeled\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn eval_copies_only_the_files_folder_into_a_fresh_repository() {
    let repo = no_todo_repo();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            &NO_TODO.replace(
                "run: $REQFILE_ASSETS/check.sh",
                "run: test ! -e example.yaml && test ! -e files && $REQFILE_ASSETS/check.sh",
            ),
        )]),
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .starts_with("NO_TODO  asserted: 1 examples, 1 as labeled\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_flag_on_another_file_than_the_expected_finding_counts_as_missed() {
    let repo = repo!();
    // Reports each .txt file holding TODO, with its location.
    let script = r#"#!/bin/sh
printf '{"runs":[{"results":['
sep=''
for f in "$@"; do
  if grep -q TODO "$f"; then
    printf '%s{"message":{"text":"TODO"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"%s"}}}]}' "$sep" "$f"
    sep=','
  fi
done
printf ']}]}'
"#;
    repo.write(".reqfile/NO_TODO/check.sh", script);
    std::process::Command::new("chmod")
        .args(["+x", ".reqfile/NO_TODO/check.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            &NO_TODO.replace("\n    fix_hint", "\n    format: sarif\n    fix_hint"),
        )]),
    );
    let case = ".reqfile/NO_TODO/examples/todo-in-plan";
    repo.write(
        &format!("{case}/example.yaml"),
        "expected: violation\nfindings: [plan.txt]\n",
    );
    repo.write(&format!("{case}/files/plan.txt"), "todo, lowercase\n");
    repo.write(&format!("{case}/files/notes.txt"), "TODO\n");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout.contains(
            "  todo-in-plan: missed: flagged notes.txt instead of its expected findings\n"
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn a_known_miss_is_reported_but_does_not_fail_an_asserted_requirement() {
    let repo = no_todo_repo();
    let case = ".reqfile/NO_TODO/examples/lowercase";
    repo.write(
        &format!("{case}/example.yaml"),
        "expected: violation\nknown: miss\nrationale: The script only knows uppercase.\n",
    );
    repo.write(&format!("{case}/files/notes.txt"), "todo: finish\n");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("  lowercase: missed (known)\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_known_miss_the_checks_now_catch_asks_to_remove_known() {
    let repo = no_todo_repo();
    let case = ".reqfile/NO_TODO/examples/uppercase";
    repo.write(
        &format!("{case}/example.yaml"),
        "expected: violation\nknown: miss\n",
    );
    repo.write(&format!("{case}/files/notes.txt"), "TODO: finish\n");

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("  uppercase: caught, as labeled now: remove `known`\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn command_checks_reporting_probabilities_are_measured_not_asserted() {
    let repo = repo!();
    let sarif = r#"{"runs": [{"results": [{"message": {"text": "maybe"}, "locations": [{"physicalLocation": {"artifactLocation": {"uri": "a.txt"}}}], "properties": {"probability": 0.9}}]}]}"#;
    repo.write(".reqfile/GUESS/result.sarif", sarif);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("GUESS", "- command:\n    run: cat $REQFILE_ASSETS/result.sarif\n    format: sarif\n    fix_hint: x")]),
    );
    write_case(&repo, ".reqfile/GUESS/examples/ok-flagged/a.txt", "fine\n");

    let run = repo.run(&["eval"]);

    // A false alarm of a probabilistic check is measured, never failing.
    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("GUESS  measured, no assertion: 0 of 0 violations caught (0 missed, 0 not selected, 0 uncertain); 1 of 1 correct examples flagged (95% interval 21 to 100%; 0 uncertain)\n  ok-flagged: false alarm\n  no violation examples: catches are not measured\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn test_is_a_hidden_alias_of_eval_for_one_version() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let eval = repo.run(&["eval"]);
    let test = repo.run(&["test"]);

    assert_eq!(test.code, 0, "{}", test.output());
    assert_eq!(test.stdout, eval.stdout);
    let help = repo.run(&["--help"]);
    assert!(help.stdout.contains("eval"), "{}", help.stdout);
    assert!(!help.stdout.contains("  test "), "{}", help.stdout);
}

#[test]
fn example_add_turns_a_missed_file_into_a_violation_example_with_its_expected_finding() {
    let repo = no_todo_repo();
    repo.write("docs/plan.txt", "todo: lowercase\n");
    repo.write("docs/other.txt", "fine\n");
    repo.commit("real code");

    let run = repo.run(&[
        "example",
        "add",
        "NO_TODO",
        "lowercase-todo",
        "--expected",
        "violation",
        "--finding",
        "docs/plan.txt",
        "--rationale",
        "A lowercase todo is still a TODO.",
        "docs",
    ]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("Added .reqfile/NO_TODO/examples/lowercase-todo/ (2 files)."),
        "{}",
        run.stdout
    );
    let case = repo.path().join(".reqfile/NO_TODO/examples/lowercase-todo");
    let yaml = std::fs::read_to_string(case.join("example.yaml")).expect("example.yaml");
    assert!(yaml.starts_with("expected: violation\nfindings:\n  - \"docs/plan.txt\"\nrationale: \"A lowercase todo is still a TODO.\"\norigin: \"real:"), "{yaml}");
    assert!(case.join("files/docs/other.txt").is_file());

    // The check misses it until someone improves the check.
    let eval = repo.run(&["eval"]);
    assert_eq!(eval.code, 1, "{}", eval.output());
    assert!(
        eval.stdout.contains("  lowercase-todo: missed\n"),
        "{}",
        eval.stdout
    );
}

#[test]
fn example_add_turns_a_finding_into_a_false_alarm_example_with_the_files_it_needs() {
    let repo = no_todo_repo();
    repo.write("notes.txt", "Never write TODO in code.\n");

    let run = repo.run(&[
        "example",
        "add",
        "NO_TODO",
        "mentions-todo",
        "--expected",
        "ok",
        "notes.txt",
    ]);

    assert_eq!(run.code, 0, "{}", run.output());
    let eval = repo.run(&["eval"]);
    assert!(
        eval.stdout.contains("  mentions-todo: false alarm\n"),
        "{}",
        eval.stdout
    );

    let again = repo.run(&[
        "example",
        "add",
        "NO_TODO",
        "mentions-todo",
        "--expected",
        "ok",
        "notes.txt",
    ]);
    assert_eq!(again.code, 3, "{}", again.output());
    assert!(again.stderr.contains("already exists"), "{}", again.stderr);

    let finding_on_ok = repo.run(&[
        "example",
        "add",
        "NO_TODO",
        "x",
        "--expected",
        "ok",
        "--finding",
        "notes.txt",
        "notes.txt",
    ]);
    assert_eq!(finding_on_ok.code, 3, "{}", finding_on_ok.output());
}

/// Reports each .txt file holding TODO with a probability, so the
/// requirement is measured rather than asserted.
fn probable_todo_repo() -> Repo {
    let repo = repo!();
    let script = r#"#!/bin/sh
printf '{"runs":[{"results":['
sep=''
for f in "$@"; do
  if grep -q TODO "$f"; then
    printf '%s{"message":{"text":"TODO"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"%s"}}}],"properties":{"probability":0.9}}' "$sep" "$f"
    sep=','
  fi
done
printf ']}]}'
"#;
    repo.write(".reqfile/NO_TODO/check.sh", script);
    std::process::Command::new("chmod")
        .args(["+x", ".reqfile/NO_TODO/check.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "NO_TODO",
            &NO_TODO.replace("\n    fix_hint", "\n    format: sarif\n    fix_hint"),
        )]),
    );
    repo
}

#[test]
fn held_out_examples_are_measured_apart_from_those_the_checks_were_tuned_on() {
    let repo = probable_todo_repo();
    let examples = ".reqfile/NO_TODO/examples";
    write_case(
        &repo,
        &format!("{examples}/violation-todo/notes.txt"),
        "TODO\n",
    );
    write_case(&repo, &format!("{examples}/ok-done/notes.txt"), "done\n");
    for (case, content, expected) in [
        ("violation-lowercase", "todo: finish\n", "violation"),
        ("ok-mentions-todo", "Never write TODO.\n", "ok"),
    ] {
        write_case(&repo, &format!("{examples}/{case}/notes.txt"), content);
        repo.write(
            &format!("{examples}/{case}/example.yaml"),
            &format!("expected: {expected}\nsplit: holdout\n"),
        );
    }

    let run = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.stdout,
        "NO_TODO  measured, no assertion\n  tuning: 1 of 1 violations caught (95% interval 21 to 100%; 0 missed, 0 not selected, 0 uncertain); 0 of 1 correct examples flagged (95% interval 0 to 79%; 0 uncertain)\n  held out: 0 of 1 violations caught (95% interval 0 to 79%; 1 missed, 0 not selected, 0 uncertain); 1 of 1 correct examples flagged (95% interval 21 to 100%; 0 uncertain)\n  ok-mentions-todo (held out): false alarm\n  violation-lowercase (held out): missed\n\n1 requirements: 0 asserted, 1 measured, 0 without evidence; 4 examples. 0 failing their labels.\n"
    );
}

#[test]
fn repeat_reruns_each_example_and_reports_the_cases_whose_outcome_changes() {
    let asked = std::sync::atomic::AtomicUsize::new(0);
    // A model that changes its mind on every question, probes aside.
    let jev = FakeJev::start(move |body| {
        if body["questions"].get("probe").is_some() {
            return reqfile_test_support::Reply::Probability(0.05);
        }
        let n = asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        reqfile_test_support::Reply::Probability(if n.is_multiple_of(2) { 0.95 } else { 0.05 })
    });
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    write_case(
        &repo,
        ".reqfile/DECOMPLECT/examples/violation-flaky/a.py",
        "def f():\n    pass\n",
    );
    repo.env("OPENROUTER_API_KEY", "key");

    let run = repo.run(&["eval", "--repeat", "2"]);

    assert_eq!(run.code, 0, "{}", run.output());
    // A tie counts against the label: noise never reads as a catch.
    assert_eq!(
        run.stdout,
        "DECOMPLECT  measured, no assertion: 0 of 1 violations caught (95% interval 0 to 79%; 1 missed, 0 not selected, 0 uncertain); 0 of 0 correct examples flagged (0 uncertain)\n  violation-flaky: unstable over 2 runs: caught 1, missed 1\n  no correct examples: false alarms are not measured\n\n1 requirements: 0 asserted, 1 measured, 0 without evidence; 1 examples, 2 runs each, 1 unstable. 0 failing their labels.\n"
    );
    assert_eq!(
        jev.questions_received().len(),
        2,
        "each run asks the model again rather than reading a cached answer"
    );
}

#[test]
fn an_asserted_check_whose_outcome_changes_between_runs_fails() {
    let repo = no_todo_repo();
    // Flags TODO on odd runs only, counting runs in its assets folder.
    repo.write(
        ".reqfile/NO_TODO/check.sh",
        "#!/bin/sh\nn=$(cat \"$REQFILE_ASSETS/runs\" 2>/dev/null || echo 0)\necho $((n + 1)) > \"$REQFILE_ASSETS/runs\"\nif [ $((n % 2)) -eq 0 ] && grep -l TODO \"$@\"; then exit 1; fi\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );

    let once = repo.run(&["eval"]);
    repo.remove(".reqfile/NO_TODO/runs");
    let twice = repo.run(&["eval", "--repeat", "2"]);

    assert_eq!(once.code, 0, "{}", once.output());
    assert_eq!(twice.code, 1, "{}", twice.output());
    assert!(
        twice.stdout.contains(
            "NO_TODO  asserted: 1 examples, 0 as labeled\n  violation-todo: unstable over 2 runs: caught 1, missed 1\n"
        ),
        "{}",
        twice.stdout
    );
}

#[test]
fn example_add_holdout_keeps_the_example_out_of_tuning() {
    let repo = no_todo_repo();
    repo.write("docs/plan.txt", "TODO\n");
    repo.commit("real code");

    let run = repo.run(&[
        "example",
        "add",
        "NO_TODO",
        "plan",
        "--expected",
        "violation",
        "--holdout",
        "docs",
    ]);

    assert_eq!(run.code, 0, "{}", run.output());
    let yaml = std::fs::read_to_string(
        repo.path()
            .join(".reqfile/NO_TODO/examples/plan/example.yaml"),
    )
    .expect("example.yaml");
    assert!(yaml.ends_with("split: holdout\n"), "{yaml}");
}

#[test]
fn pretty_eval_is_a_table_of_requirements_with_their_rates() {
    let repo = no_todo_repo();
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-todo/notes.txt",
        "TODO\n",
    );
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-lowercase/notes.txt",
        "todo\n",
    );

    let run = repo.run(&["eval", "--format", "pretty"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout
            .starts_with("reqfile eval · 1 requirement · 2 examples\n\n"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains(
            "  ✗ NO_TODO  asserted · 1/2 as labeled\n      violation-lowercase: missed\n"
        ),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("✗ 1 requirement failing their labels"),
        "{}",
        run.stdout
    );
}

#[test]
fn eval_json_gives_each_requirement_its_counts_per_split_and_every_case_with_its_outcome() {
    let repo = probable_todo_repo();
    let examples = ".reqfile/NO_TODO/examples";
    write_case(
        &repo,
        &format!("{examples}/violation-todo/notes.txt"),
        "TODO\n",
    );
    write_case(
        &repo,
        &format!("{examples}/violation-lowercase/notes.txt"),
        "todo\n",
    );
    repo.write(
        &format!("{examples}/violation-lowercase/example.yaml"),
        "expected: violation\nknown: miss\nsplit: confirmation-1\n",
    );
    write_case(
        &repo,
        &format!("{examples}/violation-elsewhere/a.txt"),
        "fine\n",
    );
    repo.write(
        &format!("{examples}/violation-elsewhere/files/b.txt"),
        "TODO\n",
    );
    repo.write(
        &format!("{examples}/violation-elsewhere/example.yaml"),
        "expected: violation\nfindings: [a.txt]\nsplit: holdout\n",
    );
    write_case(
        &repo,
        &format!("{examples}/ok-mentions-todo/notes.txt"),
        "Never write TODO.\n",
    );

    let run = repo.run(&["eval", "--format", "json"]);
    let plain = repo.run(&["eval"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        run.code, plain.code,
        "the exit code does not depend on the format"
    );
    let json = run.json();
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["summary"]["examples"], 4);
    assert_eq!(json["summary"]["failing"], 0);
    let requirement = &json["requirements"][0];
    assert_eq!(requirement["id"], "NO_TODO");
    assert_eq!(requirement["evidence"], "measured");
    assert_eq!(requirement["failed"], false);
    let case = |name: &str| {
        requirement["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("no case {name}: {requirement}"))
            .clone()
    };
    assert_eq!(
        case("violation-todo"),
        serde_json::json!({"name": "violation-todo", "split": "tune", "expected": "violation", "outcome": "caught", "accepted": true, "flagged": ["notes.txt"]})
    );
    assert_eq!(
        case("violation-lowercase"),
        serde_json::json!({"name": "violation-lowercase", "split": "confirmation-1", "expected": "violation", "outcome": "missed", "accepted": true, "flagged": [], "known": "miss"})
    );
    assert_eq!(case("violation-elsewhere")["outcome"], "flagged_elsewhere");
    assert_eq!(
        case("violation-elsewhere")["flagged"],
        serde_json::json!(["b.txt"])
    );
    assert_eq!(case("ok-mentions-todo")["outcome"], "false_alarm");
    assert_eq!(
        requirement["splits"],
        serde_json::json!([
            {"split": "tune", "violations": {"total": 1, "caught": 1, "missed": 0, "not_selected": 0, "uncertain": 0}, "ok": {"total": 1, "passed": 0, "flagged": 1, "uncertain": 0}, "errors": 0, "unstable": 0},
            {"split": "holdout", "violations": {"total": 1, "caught": 0, "missed": 1, "not_selected": 0, "uncertain": 0}, "ok": {"total": 0, "passed": 0, "flagged": 0, "uncertain": 0}, "errors": 0, "unstable": 0},
            {"split": "confirmation-1", "violations": {"total": 1, "caught": 0, "missed": 1, "not_selected": 0, "uncertain": 0}, "ok": {"total": 0, "passed": 0, "flagged": 0, "uncertain": 0}, "errors": 0, "unstable": 0}
        ])
    );
}

#[test]
fn eval_json_gives_the_error_of_a_case_that_cannot_run_and_each_run_with_repeat() {
    let repo = repo!();
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[(
            "RULE",
            "- command:\n    run: echo broken >&2; exit 2\n    fix_hint: x",
        )]),
    );
    write_case(&repo, ".reqfile/RULE/examples/ok-any/notes.txt", "text\n");

    let run = repo.run(&["eval", "--format", "json", "--repeat", "2"]);

    assert_eq!(run.code, 3, "{}", run.output());
    let json = run.json();
    let case = &json["requirements"][0]["cases"][0];
    assert_eq!(case["outcome"], "error");
    assert!(
        case["error"].as_str().is_some_and(|e| e.contains("broken")),
        "{case}"
    );
    assert_eq!(case["stable"], true);
    assert_eq!(case["outcomes"], serde_json::json!(["error", "error"]));
    assert_eq!(json["requirements"][0]["splits"][0]["errors"], 1);
    assert_eq!(json["summary"]["runs"], 2);
}

#[test]
fn every_named_split_is_measured_apart_by_its_name() {
    let repo = probable_todo_repo();
    let examples = ".reqfile/NO_TODO/examples";
    write_case(
        &repo,
        &format!("{examples}/violation-todo/notes.txt"),
        "TODO\n",
    );
    write_case(&repo, &format!("{examples}/ok-done/notes.txt"), "done\n");
    for (case, content, split) in [
        ("violation-lowercase", "todo: finish\n", "holdout"),
        ("violation-second", "TODO again\n", "confirmation-1"),
    ] {
        write_case(&repo, &format!("{examples}/{case}/notes.txt"), content);
        repo.write(
            &format!("{examples}/{case}/example.yaml"),
            &format!("expected: violation\nsplit: {split}\n"),
        );
    }

    let plain = repo.run(&["eval"]);
    let pretty = repo.run(&["eval", "--format", "pretty"]);

    assert_eq!(plain.code, 0, "{}", plain.output());
    assert!(
        plain.stdout.starts_with(
            "NO_TODO  measured, no assertion\n  tuning: 1 of 1 violations caught (95% interval 21 to 100%; 0 missed, 0 not selected, 0 uncertain); 0 of 1 correct examples flagged (95% interval 0 to 79%; 0 uncertain)\n  held out: 0 of 1 violations caught (95% interval 0 to 79%; 1 missed, 0 not selected, 0 uncertain); 0 of 0 correct examples flagged (0 uncertain)\n  confirmation-1: 1 of 1 violations caught (95% interval 21 to 100%; 0 missed, 0 not selected, 0 uncertain); 0 of 0 correct examples flagged (0 uncertain)\n  violation-lowercase (held out): missed\n"
        ),
        "{}",
        plain.stdout
    );
    assert!(
        pretty.stdout.contains(
            "    tuning          caught 1/1 (21–100%) · false alarms 0/1 (0–79%)\n    held out        caught 0/1 (0–79%) · false alarms 0/0\n    confirmation-1  caught 1/1 (21–100%) · false alarms 0/0\n"
        ),
        "{}",
        pretty.stdout
    );
}

#[test]
fn command_override_measures_another_command_for_this_run_without_writing_any_reqfile() {
    let repo = no_todo_repo();
    // The working copy of the check also flags lowercase todo.
    repo.write(
        ".reqfile/NO_TODO/check-dev.sh",
        "#!/bin/sh\nif grep -il TODO \"$@\"; then exit 1; fi\n",
    );
    std::process::Command::new("chmod")
        .args(["+x", ".reqfile/NO_TODO/check-dev.sh"])
        .current_dir(repo.path())
        .status()
        .expect("chmod");
    write_case(
        &repo,
        ".reqfile/NO_TODO/examples/violation-lowercase/notes.txt",
        "todo\n",
    );
    let reqfile_before =
        std::fs::read_to_string(repo.path().join("Reqfile.yaml")).expect("Reqfile.yaml");

    let published = repo.run(&["eval"]);
    let working = repo.run(&[
        "eval",
        "--command-override",
        "$REQFILE_ASSETS/check.sh=>$REQFILE_ASSETS/check-dev.sh",
    ]);
    let typo = repo.run(&["eval", "--command-override", "check-typo.sh=>check-dev.sh"]);

    assert_eq!(published.code, 1, "{}", published.output());
    assert_eq!(working.code, 0, "{}", working.output());
    assert!(
        working
            .stdout
            .contains("NO_TODO  asserted: 1 examples, 1 as labeled\n"),
        "{}",
        working.stdout
    );
    assert_eq!(typo.code, 3, "{}", typo.output());
    assert!(
        typo.stderr
            .contains("--command-override: no command check runs `check-typo.sh`"),
        "{}",
        typo.stderr
    );
    assert_eq!(
        std::fs::read_to_string(repo.path().join("Reqfile.yaml")).expect("Reqfile.yaml"),
        reqfile_before
    );
}
