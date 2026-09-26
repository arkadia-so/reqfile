//! DECISION_CHECK: decision.yaml units, the question asked to Jev, and thresholds.

use reqfile_test_support::{DEFAULT_MODEL, FakeJev, PYTHON_FUNCTIONS, Reply, Repo, repo, reqfile};

const CODE: &str = r#"def clean(x):
    return x.strip()


def sync(db):
    # VIOLATION
    rows = db.fetch()
    return merge(rows)


def unsure():
    # UNSURE
    pass
"#;

/// A config pinning the model, as blocking decision checks require.
fn pinned(jev: &FakeJev) -> String {
    format!("{}  model: typesafe/jev-1.13-20260917\n", jev.config())
}

fn decision_repo(jev: &FakeJev, spec: &str) -> Repo {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", spec);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.env("OPENROUTER_API_KEY", "test-key");
    repo
}

#[test]
fn each_unit_is_asked_about_and_judged_by_the_thresholds() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("app/worker.py", CODE);

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(jev.questions_received().len(), 3);
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  app/worker.py:5  p=0.95  It mixes several jobs.\n  fix: Split the function."),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("uncertain  DECOMPLECT  app/worker.py:11  p=0.50  Does this function do more than one job?\n  fix: Split the function."),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("worker.py:1 "), "{}", run.stdout);
    assert!(
        run.stdout.contains(
            "1 check run, 0 with nothing to check, 3 code units judged. 0 violations, 2 advisory findings, 0 errors."
        ),
        "{}",
        run.stdout
    );
}

#[test]
fn the_request_asks_a_yes_no_question_about_the_unit() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("worker.py", "def clean(x):\n    return x.strip()\n");

    repo.run(&["check"]);

    let received = jev.questions_received();
    assert_eq!(received.len(), 1);
    let request = &received[0];
    assert_eq!(request.path, "/api/v1/systemone");
    assert_eq!(request.authorization.as_deref(), Some("Bearer test-key"));
    let body = &request.body;
    assert_eq!(body["model"], DEFAULT_MODEL);
    assert_eq!(body["state"]["language"], "python");
    assert_eq!(body["state"]["path"], "worker.py");
    assert_eq!(body["state"]["unit"], "def clean(x):\n    return x.strip()");
    assert!(body["state"].get("enclosing_function").is_none());
    let question = &body["questions"]["DECOMPLECT"];
    assert_eq!(question["type"], "noul");
    assert_eq!(
        question["instructions"]["question"],
        "Does this function do more than one job?"
    );
    assert_eq!(question["criteria"]["true"], "It mixes several jobs.");
    assert_eq!(question["criteria"]["false"], "It does one job.");
}

#[test]
fn enclosing_context_sends_the_enclosing_function() {
    let jev = FakeJev::by_marker();
    let spec = r#"
units:
  - { language: python, rule: { kind: except_clause } }
context: enclosing
question: Does this error handler hide the failure?
violation_when: It swallows the error.
ok_when: It handles it visibly.
fix_hint: Let it propagate.
thresholds: { violation_above: 0.8, pass_below: 0.2 }
"#;
    let repo = decision_repo(&jev, spec);
    repo.write("load.py", "def load(path):\n    try:\n        return read(path)\n    except OSError:\n        return None\n");

    repo.run(&["check"]);

    let received = jev.questions_received();
    assert_eq!(received.len(), 1);
    let state = &received[0].body["state"];
    assert_eq!(state["unit"], "except OSError:\n        return None");
    assert!(
        state["enclosing_function"]
            .as_str()
            .unwrap_or_default()
            .starts_with("def load(path):"),
        "{state}"
    );
}

#[test]
fn units_can_come_from_several_languages_and_patterns() {
    let jev = FakeJev::by_marker();
    let spec = r#"
units:
  - { language: rust, rule: { pattern: $X.unwrap_or_default() } }
  - { language: typescript, rule: { kind: catch_clause } }
question: Does this hide a failure?
violation_when: It hides it.
ok_when: It does not.
fix_hint: Surface the error.
thresholds: { violation_above: 0.8, pass_below: 0.2 }
"#;
    let repo = decision_repo(&jev, spec);
    repo.write(
        "src/lib.rs",
        "fn f() -> u32 {\n    parse_VIOLATION().unwrap_or_default()\n}\n",
    );
    repo.write(
        "web/app.ts",
        "try {\n  run();\n} catch (e) {\n  // VIOLATION\n}\n",
    );
    repo.write("web/other.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout
            .contains("advisory  DECOMPLECT  src/lib.rs:2  p=0.95"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("advisory  DECOMPLECT  web/app.ts:3  p=0.95"),
        "{}",
        run.stdout
    );
    assert_eq!(jev.questions_received().len(), 2);
}

#[test]
fn unit_files_and_ignores_select_where_units_come_from() {
    let jev = FakeJev::by_marker();
    let spec = PYTHON_FUNCTIONS.replace(
        "rule: { kind: function_definition } }",
        "rule: { kind: function_definition }, files: \"**/core/**\", ignores: [\"**/test_*.py\", \"**/*_test.py\"] }",
    );
    let repo = decision_repo(&jev, &spec);
    repo.write("core/step.py", "def step():\n    pass  # VIOLATION\n");
    repo.write(
        "core/test_step.py",
        "def test_step():\n    pass  # VIOLATION\n",
    );
    repo.write("shell/io.py", "def run():\n    pass  # VIOLATION\n");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("DECOMPLECT  core/step.py:1"),
        "{}",
        run.stdout
    );
    assert_eq!(jev.questions_received().len(), 1, "{}", run.stdout);
}

#[test]
fn a_decision_without_matching_files_does_not_run() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("main.rs", "fn main() {}\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout
            .contains("0 checks run, 1 with nothing to check, 0 code units judged."),
        "{}",
        run.stdout
    );
    assert!(jev.questions_received().is_empty());
}

#[test]
fn the_decision_yaml_lives_next_to_its_reqfile() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision")]),
    );
    repo.write("app/.reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.env("OPENROUTER_API_KEY", "test-key");
    repo.write("app/a.py", "def f():\n    pass  # VIOLATION\n")
        .write("b.py", "def g():\n    pass  # VIOLATION\n");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("DECOMPLECT  app/a.py:1"),
        "{}",
        run.stdout
    );
    assert_eq!(jev.questions_received().len(), 1, "{}", run.stdout);
}

#[test]
fn a_missing_decision_yaml_is_a_config_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/config.yaml", &jev.config());

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 3, "{}", run.output());
    assert!(
        run.stdout.contains("Reqfile.yaml:8: the decision check of DECOMPLECT needs .reqfile/DECOMPLECT/decision.yaml"),
        "{}",
        run.stdout
    );
}

#[test]
fn an_invalid_decision_yaml_names_its_file_and_line() {
    let jev = FakeJev::by_marker();
    for (spec, expected) in [
        (
            PYTHON_FUNCTIONS.replace("ok_when:", "fine_when:"),
            ".reqfile/DECOMPLECT/decision.yaml:6: unknown key `fine_when`",
        ),
        (
            PYTHON_FUNCTIONS.replace("violation_above: 0.8", "violation_above: 0.1"),
            ".reqfile/DECOMPLECT/decision.yaml:8: thresholds must satisfy 0 <= pass_below <= violation_above <= 1",
        ),
        (
            PYTHON_FUNCTIONS.replace("language: python", "language: cobol"),
            ".reqfile/DECOMPLECT/decision.yaml:3: invalid ast-grep rule",
        ),
        (
            PYTHON_FUNCTIONS.replace("kind: function_definition", "kinds: function_definition"),
            "decision.yaml:3: invalid ast-grep rule",
        ),
        (
            PYTHON_FUNCTIONS.replace("question: Does this function do more than one job?\n", ""),
            "decision.yaml is missing the required field `question`",
        ),
    ] {
        let repo = decision_repo(&jev, &spec);
        repo.write("a.py", "def f():\n    pass\n");
        let run = repo.run(&["check"]);
        assert_eq!(run.code, 3, "{spec}: {}", run.output());
        assert!(
            run.stdout.contains(expected),
            "expected {expected:?} in:\n{}",
            run.stdout
        );
    }
}

#[test]
fn a_failed_jev_call_is_an_error() {
    let jev = FakeJev::start(|_| Reply::Status(500, "{\"error\": \"boom\"}".into()));
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n\ndef g():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout.contains("error  DECOMPLECT  2 of 2 units could not be judged, first at a.py:1: Jev returned HTTP 500: {\"error\": \"boom\"}"),
        "{}",
        run.stdout
    );
}

#[test]
fn rate_limited_calls_are_retried() {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let jev = FakeJev::start(move |_| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::Status(429, "slow down".into())
        } else {
            Reply::Probability(0.9)
        }
    });
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:1  p=0.90"),
        "{}",
        run.stdout
    );
    assert_eq!(
        jev.received().len(),
        3,
        "the probe, its retry, then the question"
    );
}

#[test]
fn an_unparseable_jev_answer_is_an_error() {
    let jev = FakeJev::start(|_| {
        Reply::Status(
            200,
            "{\"model\": \"typesafe/jev-1.13-20260917\", \"answers\": {}}".into(),
        )
    });
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout
            .contains("Jev response has no probability for DECOMPLECT"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_missing_api_key_is_an_error() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/config.yaml", &jev.config());
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout.contains("OPENROUTER_API_KEY is not set"),
        "{}",
        run.stdout
    );
    assert!(jev.questions_received().is_empty());
}

#[test]
fn the_api_key_variable_is_configurable() {
    let jev = FakeJev::by_marker();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("DECOMPLECT", "- decision")]));
    repo.write(".reqfile/DECOMPLECT/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  api_key_env: MY_JEV_KEY\n", jev.config()),
    );
    repo.env("MY_JEV_KEY", "other-key");
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(
        jev.questions_received()[0].authorization.as_deref(),
        Some("Bearer other-key")
    );
}

#[test]
fn calls_run_in_parallel_up_to_the_concurrency_limit() {
    let in_flight = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (current, max) = (in_flight.clone(), peak.clone());
    let jev = FakeJev::start(move |_| {
        let now = current.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        max.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        current.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        Reply::Probability(0.0)
    });
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        ".reqfile/config.yaml",
        &format!("{}  concurrency: 2\n", jev.config()),
    );
    let code: String = (0..6)
        .map(|i| format!("def f{i}():\n    pass\n\n"))
        .collect();
    repo.write("a.py", &code);

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert_eq!(jev.questions_received().len(), 6);
    assert!(peak.load(std::sync::atomic::Ordering::SeqCst) <= 2);
}

#[test]
fn decision_violations_are_advisory_by_default() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:1  p=0.95"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("0 violations, 1 advisory finding, 0 errors."),
        "{}",
        run.stdout
    );
}

#[test]
fn blocking_decisions_fail_the_run() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision: { mode: blocking }")]),
    );
    repo.write(".reqfile/config.yaml", &pinned(&jev));
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(
        run.stdout
            .contains("DECOMPLECT  a.py:1  p=0.95  It mixes several jobs."),
        "{}",
        run.stdout
    );
    assert!(
        !run.stdout.contains("advisory  DECOMPLECT"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("1 violation, 0 advisory findings, 0 errors."),
        "{}",
        run.stdout
    );
}

#[test]
fn uncertain_findings_stay_advisory_in_blocking_mode() {
    let jev = FakeJev::start(|_| Reply::Probability(0.5));
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision: { mode: blocking }")]),
    );
    repo.write(".reqfile/config.yaml", &pinned(&jev));
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    assert!(
        run.stdout.contains("uncertain  DECOMPLECT  a.py:1  p=0.50"),
        "{}",
        run.stdout
    );
}

#[test]
fn the_model_is_the_latest_jev_unless_a_config_sets_one() {
    for (config, model) in [
        ("", "~typesafe/jev-latest"),
        ("  model: jev-1.13.0\n", "jev-1.13.0"),
    ] {
        let jev = FakeJev::by_marker();
        let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
        repo.write(".reqfile/config.yaml", &format!("{}{config}", jev.config()));
        repo.write("a.py", "def f():\n    pass\n");

        repo.run(&["check"]);

        let received = jev.received();
        assert_eq!(received.len(), 2, "the probe, then the question");
        assert!(
            received.iter().all(|r| r.body["model"] == model),
            "{received:?}"
        );
    }
}

#[test]
fn each_unit_is_sent_once_with_the_questions_of_every_check_that_selects_it() {
    let jev =
        FakeJev::start(|_| Reply::ByQuestion(vec![("DECOMPLECT", 0.9), ("DEEP_MODULES", 0.1)]));
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(".reqfile/DEEP_MODULES/decision.yaml", DEEP_MODULES_SPEC);
    repo.write("a.py", "def f():\n    pass\n\ndef g():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let received = jev.questions_received();
    assert_eq!(received.len(), 2, "one request per unit");
    for request in &received {
        let questions = request.body["questions"].as_object().expect("questions");
        assert_eq!(questions.len(), 2);
        assert_eq!(
            questions["DECOMPLECT"]["instructions"]["question"],
            "Does this function do more than one job?"
        );
        assert_eq!(
            questions["DEEP_MODULES"]["instructions"]["question"],
            "Does this function only forward its arguments?"
        );
    }
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:1  p=0.90"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("advisory  DECOMPLECT  a.py:4  p=0.90"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("DEEP_MODULES  a.py"), "{}", run.stdout);
    assert!(run.stdout.contains("2 checks run"), "{}", run.stdout);
}

#[test]
fn units_sent_with_their_enclosing_function_are_asked_separately() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(
        ".reqfile/DEEP_MODULES/decision.yaml",
        &DEEP_MODULES_SPEC.replace("question:", "context: enclosing\nquestion:"),
    );
    repo.write(
        "a.py",
        "def f():\n    def g():\n        pass\n    return g\n",
    );

    repo.run(&["check"]);

    // f once for both checks (it has no enclosing function); g once alone and
    // once with f as its enclosing function.
    let received = jev.questions_received();
    assert_eq!(received.len(), 3);
    let with_enclosing: Vec<_> = received
        .iter()
        .filter(|r| r.body["state"].get("enclosing_function").is_some())
        .collect();
    assert_eq!(with_enclosing.len(), 1);
    assert_eq!(
        with_enclosing[0].body["questions"]
            .as_object()
            .expect("questions")
            .len(),
        1
    );
}

#[test]
fn a_failed_batched_call_is_an_error_for_every_check_in_it() {
    let jev = FakeJev::start(|body| {
        if body["questions"].get("probe").is_some() {
            Reply::Probability(0.0)
        } else {
            Reply::Status(500, "boom".into())
        }
    });
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(".reqfile/DEEP_MODULES/decision.yaml", DEEP_MODULES_SPEC);
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout
            .contains("error  DECOMPLECT  1 of 1 units could not be judged"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("error  DEEP_MODULES  1 of 1 units could not be judged"),
        "{}",
        run.stdout
    );
    assert_eq!(jev.questions_received().len(), 1);
}

#[test]
fn a_missing_answer_for_one_question_is_an_error_for_that_check() {
    let jev = FakeJev::start(|_| Reply::ByQuestion(vec![("DECOMPLECT", 0.1)]));
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(".reqfile/DEEP_MODULES/decision.yaml", DEEP_MODULES_SPEC);
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout
            .contains("Jev response has no probability for DEEP_MODULES"),
        "{}",
        run.stdout
    );
}

const TWO_FUNCTIONS: &str = "def f():\n    pass  # VIOLATION\n\n\ndef g():\n    pass\n";

#[test]
fn unchanged_code_is_answered_from_the_cache() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", TWO_FUNCTIONS);

    let first = repo.run(&["check"]);
    let second = repo.run(&["check"]);

    assert_eq!(first.stdout, second.stdout);
    assert!(
        second
            .stdout
            .contains("advisory  DECOMPLECT  a.py:1  p=0.95"),
        "{}",
        second.stdout
    );
    assert_eq!(jev.questions_received().len(), 2, "only the first run asks");
    assert_eq!(jev.received().len(), 4, "each run starts with a probe");
}

#[test]
fn changed_code_is_asked_again() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", TWO_FUNCTIONS);
    repo.run(&["check"]);

    repo.write(
        "a.py",
        &TWO_FUNCTIONS.replace("def g():\n    pass", "def g():\n    return 1"),
    );
    repo.run(&["check"]);

    let asked = jev.questions_received();
    assert_eq!(asked.len(), 3);
    assert_eq!(asked[2].body["state"]["unit"], "def g():\n    return 1");
}

#[test]
fn a_changed_question_is_asked_again() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", TWO_FUNCTIONS);
    repo.run(&["check"]);

    repo.write(
        ".reqfile/DECOMPLECT/decision.yaml",
        &PYTHON_FUNCTIONS.replace("It mixes several jobs.", "It mixes jobs."),
    );
    repo.run(&["check"]);

    assert_eq!(jev.questions_received().len(), 4);
}

#[test]
fn only_uncached_questions_are_asked() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");
    repo.run(&["check"]);

    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision"), ("DEEP_MODULES", "- decision")]),
    );
    repo.write(".reqfile/DEEP_MODULES/decision.yaml", DEEP_MODULES_SPEC);
    repo.run(&["check"]);

    let asked = jev.questions_received();
    assert_eq!(asked.len(), 2);
    let questions = asked[1].body["questions"].as_object().expect("questions");
    assert_eq!(questions.keys().collect::<Vec<_>>(), ["DEEP_MODULES"]);
}

#[test]
fn a_new_jev_version_empties_the_cache() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", TWO_FUNCTIONS);
    repo.run(&["check"]);

    jev.answer_as("typesafe/jev-1.14-20261101");
    repo.run(&["check"]);
    repo.run(&["check"]);

    assert_eq!(
        jev.questions_received().len(),
        4,
        "asked again once, then cached for the new version"
    );
}

#[test]
fn failed_answers_are_not_cached() {
    let questions = std::sync::atomic::AtomicUsize::new(0);
    let jev = FakeJev::start(move |body| {
        let is_probe = body["questions"].get("probe").is_some();
        if !is_probe && questions.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Reply::Status(500, "boom".into())
        } else {
            Reply::Probability(0.1)
        }
    });
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");

    let first = repo.run(&["check"]);
    assert!(
        first.stdout.contains("advisory error  DECOMPLECT"),
        "{}",
        first.stdout
    );
    let second = repo.run(&["check"]);

    assert_eq!(second.code, 0, "{}", second.output());
    assert_eq!(jev.questions_received().len(), 2);
}

#[test]
fn answers_are_cached_locally_and_never_in_tracked_files() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "app/Reqfile.yaml",
        &reqfile(&[("DEEP_MODULES", "- decision")]),
    );
    repo.write("app/.reqfile/DEEP_MODULES/decision.yaml", DEEP_MODULES_SPEC);
    repo.write("app/a.py", "def f():\n    pass\n")
        .commit("code");

    repo.run(&["check"]);

    let cache = std::fs::read_to_string(repo.path().join(".git/reqfile/jev-cache")).expect("cache");
    assert_eq!(cache.lines().count(), 2, "{cache}");
    assert!(cache.lines().all(|l| l.ends_with(" 0.05")), "{cache}");
    assert_eq!(
        repo.git(&["status", "--porcelain"]),
        "",
        "checking never modifies the repository"
    );
    assert_eq!(
        jev.questions_received().len(),
        1,
        "one request carries both questions"
    );
}

#[test]
fn a_full_run_drops_answers_no_unit_uses_any_more() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        ".reqfile/config.yaml",
        &format!("base: main\n{}", jev.config()),
    );
    repo.write("a.py", TWO_FUNCTIONS);
    repo.run(&["check"]);
    repo.commit("code");
    let cache =
        || std::fs::read_to_string(repo.path().join(".git/reqfile/jev-cache")).expect("cache");
    assert_eq!(cache().lines().count(), 2);

    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");
    repo.run(&["check", "--changed"]);
    assert_eq!(
        cache().lines().count(),
        2,
        "a partial run keeps what it did not see"
    );

    repo.run(&["check"]);
    assert_eq!(cache().lines().count(), 1);
}

#[test]
fn a_corrupt_cache_is_an_error() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");
    repo.write(".git/reqfile/jev-cache", "0123456789abcdef not-a-number\n");

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout.contains("reqfile/jev-cache: invalid line 1"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("delete the file to rebuild it"),
        "{}",
        run.stdout
    );
}

/// A second decision check selecting the same Python functions as `PYTHON_FUNCTIONS`.
const DEEP_MODULES_SPEC: &str = "units:\n  - { language: python, rule: { kind: function_definition } }\nquestion: Does this function only forward its arguments?\nviolation_when: It only forwards.\nok_when: It adds logic.\nfix_hint: Remove the pass-through.\nthresholds: { violation_above: 0.8, pass_below: 0.2 }\n";

#[test]
fn a_unit_too_large_for_jev_is_an_error_not_a_pass() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    let body: String = (0..3000)
        .map(|i| format!("    value_{i} = {i}  # padding\n"))
        .collect();
    repo.write("a.py", &format!("def huge():\n{body}    # VIOLATION\n"));

    let run = repo.run(&["check"]);

    assert_eq!(
        run.code,
        0,
        "an advisory check that cannot run never blocks: {}",
        run.output()
    );
    assert!(
        run.stdout.contains("error  DECOMPLECT  1 of 1 units could not be judged, first at a.py:1: the unit is too large for Jev to judge"),
        "{}",
        run.stdout
    );
    assert!(jev.questions_received().is_empty());
}

#[test]
fn a_blocking_decision_check_requires_a_pinned_model() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("DECOMPLECT", "- decision: { mode: blocking }")]),
    );
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n");

    let unpinned = repo.run(&["check"]);

    assert_eq!(unpinned.code, 3, "{}", unpinned.output());
    assert!(
        unpinned.stdout.contains(
            "Reqfile.yaml:8: the decision check of DECOMPLECT is blocking, so its model must be pinned, but the repository root follows `~typesafe/jev-latest`"
        ),
        "{}",
        unpinned.stdout
    );
    assert!(jev.received().is_empty(), "nothing is asked");

    repo.write(".reqfile/config.yaml", &pinned(&jev));
    let pinned = repo.run(&["check"]);

    assert_eq!(pinned.code, 1, "{}", pinned.output());
}

#[test]
fn a_unit_is_asked_at_most_once_per_requirement_id() {
    let jev = FakeJev::by_marker();
    // Two unit rules selecting the same function.
    let spec = PYTHON_FUNCTIONS.replace(
        "units:\n",
        "units:\n  - { language: python, rule: { pattern: 'def $F(): $$$BODY' } }\n",
    );
    let repo = decision_repo(&jev, &spec);
    repo.write("a.py", "def f():\n    pass\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let received = jev.questions_received();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(
        received[0].body["questions"]
            .as_object()
            .expect("questions")
            .len(),
        1
    );
    assert!(run.stdout.contains("1 code unit judged"), "{}", run.stdout);
}

#[test]
fn two_requirements_with_the_same_question_get_separate_answers() {
    let jev = FakeJev::start(|_| Reply::ByQuestion(vec![("FIRST", 0.95), ("SECOND", 0.05)]));
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write(
        "Reqfile.yaml",
        &reqfile(&[("FIRST", "- decision"), ("SECOND", "- decision")]),
    );
    repo.write(".reqfile/FIRST/decision.yaml", PYTHON_FUNCTIONS);
    repo.write(".reqfile/SECOND/decision.yaml", PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass\n");

    let first = repo.run(&["check"]);
    let cached = repo.run(&["check"]);

    for run in [&first, &cached] {
        assert!(
            run.stdout.contains("advisory  FIRST  a.py:1  p=0.95"),
            "{}",
            run.stdout
        );
        assert!(!run.stdout.contains("SECOND  a.py"), "{}", run.stdout);
    }
    let received = jev.questions_received();
    assert_eq!(
        received.len(),
        1,
        "both questions in one request, then cached"
    );
    assert_eq!(
        received[0].body["questions"]
            .as_object()
            .expect("questions")
            .len(),
        2
    );
}

#[test]
fn a_unit_rule_selects_only_files_of_its_own_language() {
    let jev = FakeJev::by_marker();
    let spec = PYTHON_FUNCTIONS.replace(
        "  - { language: python, rule: { kind: function_definition } }",
        "  - { language: typescript, rule: { kind: function_declaration } }",
    );
    let repo = decision_repo(&jev, &spec);
    repo.write("a.ts", "function f() {}\n");
    repo.write("b.tsx", "function g() { return <div />; }\n");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 0, "{}", run.output());
    let paths: Vec<String> = jev
        .questions_received()
        .iter()
        .map(|r| {
            r.body["state"]["path"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(
        paths,
        ["a.ts"],
        "a typescript rule never selects .tsx files"
    );
}

#[test]
fn pruning_the_shared_cache_changes_cost_but_never_results() {
    let jev = FakeJev::by_marker();
    let repo = decision_repo(&jev, PYTHON_FUNCTIONS);
    repo.write("a.py", "def f():\n    pass  # VIOLATION\n")
        .commit("code");
    let other = repo.path().join("..").join(format!(
        "{}-other",
        repo.path().file_name().unwrap().to_string_lossy()
    ));
    repo.git(&[
        "worktree",
        "add",
        "-q",
        "-b",
        "other",
        &other.to_string_lossy(),
    ]);
    std::fs::write(other.join("a.py"), "def g():\n    pass\n").expect("write");

    let before = repo.run(&["check"]);
    // A full run in the other worktree keeps only the answers it used.
    repo.run_in(
        &format!("../{}", other.file_name().unwrap().to_string_lossy()),
        &["check"],
    );
    let asked = jev.questions_received().len();
    let after = repo.run(&["check"]);

    assert_eq!(after.stdout, before.stdout);
    assert_eq!(
        jev.questions_received().len(),
        asked + 1,
        "the pruned answer is asked again"
    );
    std::fs::remove_dir_all(&other).expect("remove the other worktree");
}
