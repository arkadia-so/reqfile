//! The setup a newcomer, most often an agent, goes through: from a repository
//! without requirements, guided only by what reqfile prints and what its
//! README and skill say, to a working `reqfile check`. The steps are read from
//! the message and the documents themselves, so they cannot drift from what
//! the binary accepts.

use std::collections::BTreeMap;

use reqfile_test_support::{Remotes, Repo, remotes, repo};

const README: &str = include_str!("../README.md");
const SKILL: &str = include_str!("../skills/reqfile/SKILL.md");

/// A small project whose tests pass while `ok` holds `yes`.
fn project(repo: &Repo) {
    repo.write("app.sh", "echo hello\n")
        .write("test.sh", "grep -qx yes ok\n")
        .write("ok", "yes\n")
        .commit("a small project");
}

/// The Reqfile the no-Reqfile message proposes, as printed, with its one
/// placeholder replaced by `command`.
fn proposed_reqfile(message: &str, command: &str) -> String {
    let start = message
        .find("for example:\n")
        .unwrap_or_else(|| panic!("the message shows an example Reqfile:\n{message}"))
        + "for example:\n".len();
    let end = message[start..]
        .find("\nThen run")
        .or_else(|| message[start..].find("\n    Then run"))
        .unwrap_or_else(|| panic!("the message says what to run next:\n{message}"))
        + start;
    let lines: Vec<&str> = message[start..end].lines().collect();
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .expect("the example has lines");
    let example: String = lines
        .iter()
        .map(|l| format!("{}\n", l.get(indent..).unwrap_or("")))
        .collect();
    let open = example.matches('<').count();
    assert_eq!(open, 1, "one placeholder to fill:\n{example}");
    let from = example.find('<').expect("a placeholder");
    let to = example[from..].find('>').expect("a closed placeholder") + from;
    format!("{}{command}{}", &example[..from], &example[to + 1..])
}

/// The fenced blocks of `language` in a Markdown document.
fn fenced<'d>(document: &'d str, language: &str) -> Vec<&'d str> {
    let open = format!("```{language}\n");
    let mut blocks = Vec::new();
    let mut rest = document;
    while let Some(start) = rest.find(&open) {
        let body = &rest[start + open.len()..];
        let end = body.find("```").expect("a closed fence");
        blocks.push(&body[..end]);
        rest = &body[end..];
    }
    blocks
}

/// The id and location of every `use: owner/repo@ref` block in a Reqfile.
fn remote_uses(reqfile: &str) -> Vec<(String, String, String)> {
    let mut uses = Vec::new();
    let mut id = String::new();
    for line in reqfile.lines() {
        if let Some(at) = line.find("id: ") {
            id = line[at + 4..]
                .split([',', ' ', '}'])
                .next()
                .expect("an id")
                .to_string();
        }
        if let Some(at) = line.find("use: ") {
            let location = line[at + 5..].split([' ', '}']).next().expect("a location");
            if let Some((repository, reference)) = location.split_once('@') {
                uses.push((id.clone(), repository.to_string(), reference.to_string()));
            }
        }
    }
    uses
}

/// A repository holding `reqfile`, with every remote requirement it uses
/// served from `remotes`: a 40-character commit, which a document can only
/// make up, is replaced by the served commit; any other reference is kept,
/// so the check sees it as written.
fn with_served_uses(remotes: &Remotes, reqfile: &str) -> Repo {
    let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, repository, _) in remote_uses(reqfile) {
        ids.entry(repository).or_default().push(id);
    }
    let mut written = reqfile.to_string();
    for (repository, ids) in &ids {
        let source = remotes.repo(repository);
        let definitions: String = ids
            .iter()
            .map(|id| {
                format!(
                    "  - id: {id}\n    must: Must {id}.\n    why: Because.\n    checks:\n      - command:\n          run: \"true\"\n          fix_hint: x\n"
                )
            })
            .collect();
        source
            .write("Reqfile.yaml", &format!("reqfile: 1\ncode:\n{definitions}"))
            .commit("definitions");
        let commit = source.head();
        for (_, used, reference) in remote_uses(reqfile) {
            let is_commit =
                reference.len() == 40 && reference.chars().all(|c| c.is_ascii_hexdigit());
            if used == *repository && is_commit {
                written =
                    written.replace(&format!("{used}@{reference}"), &format!("{used}@{commit}"));
            }
        }
    }
    let repo = repo!();
    remotes.serve(&repo);
    repo.write("Reqfile.yaml", &written);
    repo
}

/// Makes `origin/HEAD` point to the current commit, as in a clone.
fn with_origin(repo: &Repo, remotes: &Remotes) {
    let origin = remotes.base().join("origin.git");
    let origin = origin.to_string_lossy();
    repo.git(&["init", "-q", "--bare", &origin]);
    repo.git(&["remote", "add", "origin", &origin]);
    repo.git(&["push", "-q", "origin", "main"]);
    repo.git(&["remote", "set-head", "origin", "main"]);
}

#[test]
fn a_newcomer_following_the_no_reqfile_message_gets_a_passing_check() {
    let repo = repo!();
    project(&repo);

    let first = repo.run(&["check"]);
    assert_eq!(first.code, 3, "{}", first.output());
    assert!(first.stdout.contains("no Reqfile.yaml"), "{}", first.stdout);

    repo.write(
        "Reqfile.yaml",
        &proposed_reqfile(&first.stdout, "sh test.sh"),
    );
    for args in [&["list"][..], &["explain", "."]] {
        let run = repo.run(args);
        assert_eq!(run.code, 0, "reqfile {args:?}: {}", run.output());
        assert!(
            run.stdout.contains("TESTS_PASS"),
            "reqfile {args:?}: {}",
            run.stdout
        );
    }
    let check = repo.run(&["check"]);
    assert_eq!(check.code, 0, "{}", check.output());
    assert!(check.stdout.contains("1 check run"), "{}", check.stdout);

    // The requirement checks something: breaking the tests is a violation.
    repo.write("ok", "no\n");
    let broken = repo.run(&["check"]);
    assert_eq!(broken.code, 1, "{}", broken.output());
}

#[test]
fn a_newcomer_can_add_published_requirements_to_that_setup() {
    let remotes = remotes!();
    let source = remotes.repo("acme/reqs");
    source
        .write(
            "Reqfile.yaml",
            "reqfile: 1\ncode:\n  - id: NO_TODO\n    must: No TODO is left in shell scripts.\n    why: A TODO is work nobody owns.\n    checks:\n      - command:\n          run: \"! grep -l TODO *.sh\"\n          fix_hint: Do it or file it.\n",
        )
        .commit("v1");
    source.git(&["tag", "v1.0.0"]);
    let repo = repo!();
    remotes.serve(&repo);
    project(&repo);
    let first = repo.run(&["check"]);
    repo.write(
        "Reqfile.yaml",
        &proposed_reqfile(&first.stdout, "sh test.sh"),
    );

    let add = repo.run(&["add", "acme/reqs@v1.0.0"]);
    assert_eq!(add.code, 0, "{}", add.output());
    let list = repo.run(&["list"]);
    assert!(
        list.stdout.contains("TESTS_PASS") && list.stdout.contains("NO_TODO"),
        "{}",
        list.output()
    );
    assert_eq!(repo.run(&["check"]).code, 0);

    repo.write("app.sh", "echo hello # TODO say more\n");
    let run = repo.run(&["check"]);
    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("NO_TODO"), "{}", run.stdout);
}

#[test]
fn every_reqfile_the_readme_and_the_skill_show_is_accepted_as_written() {
    let remotes = remotes!();
    let examples: Vec<&str> = [README, SKILL]
        .iter()
        .flat_map(|document| fenced(document, "yaml"))
        .filter(|block| block.lines().any(|l| l == "reqfile: 1"))
        .collect();
    assert!(
        examples.len() >= 2,
        "the README and the skill show a Reqfile"
    );
    for example in examples {
        let repo = with_served_uses(&remotes, example);
        let run = repo.run(&["list"]);
        assert_eq!(run.code, 0, "{example}\n{}", run.output());
    }
}

#[test]
fn the_config_the_readme_shows_is_accepted_as_written() {
    let config = fenced(README, "yaml")
        .into_iter()
        .find(|block| block.contains("\ndecision:"))
        .expect("the README shows a config");
    let repo = repo!();
    repo.write(".reqfile/config.yaml", config).write(
        "Reqfile.yaml",
        "reqfile: 1\ncode:\n  - id: A\n    must: M.\n    why: W.\n    checks:\n      - command:\n          run: \"true\"\n          fix_hint: x\n",
    );
    let run = repo.run(&["list"]);
    assert_eq!(run.code, 0, "{config}\n{}", run.output());
}

#[test]
fn the_workflow_the_skill_adds_to_agent_instructions_runs_in_a_set_up_repository() {
    let remotes = remotes!();
    let repo = repo!();
    project(&repo);
    let first = repo.run(&["check"]);
    repo.write(
        "Reqfile.yaml",
        &proposed_reqfile(&first.stdout, "sh test.sh"),
    )
    .commit("set up reqfile");
    with_origin(&repo, &remotes);

    let workflow = fenced(SKILL, "markdown")
        .into_iter()
        .find(|block| block.contains("reqfile check"))
        .expect("the skill gives the workflow for agent instructions");
    let commands: Vec<String> = workflow
        .split('`')
        .skip(1)
        .step_by(2)
        .filter(|code| code.starts_with("reqfile "))
        .map(|code| code.replace("<folder>", "."))
        .collect();
    assert!(commands.len() >= 3, "{workflow}");
    for command in commands {
        let args: Vec<&str> = command.split_whitespace().skip(1).collect();
        let run = repo.run(&args);
        assert_eq!(run.code, 0, "{command}: {}", run.output());
    }
}

#[test]
fn the_skill_installs_the_version_being_built() {
    let version = env!("CARGO_PKG_VERSION");
    for (install, expected) in [
        ("\"github:arkadia-so/reqfile\" = ", format!("\"{version}\"")),
        ("releases/download/", format!("v{version}/")),
        (
            "cargo install --git https://github.com/arkadia-so/reqfile --tag ",
            format!("v{version} --locked"),
        ),
    ] {
        let at = SKILL
            .find(install)
            .unwrap_or_else(|| panic!("the skill installs with {install}"))
            + install.len();
        assert!(
            SKILL[at..].starts_with(&expected),
            "the skill installs {} with {install}, not {version}",
            &SKILL[at..at + expected.len()]
        );
    }
}
