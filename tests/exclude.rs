//! EXCLUDE: targets come from git, minus .reqfile/ folders and the `exclude` globs of configs.

use reqfile_test_support::{repo, reqfile};

const LIST_TXT: &str = r#"- command:
    run: "fail() { echo \"got $*\"; exit 1; }; fail"
    files: "**/*.txt"
    pass_files: true
    fix_hint: None."#;

#[test]
fn targets_are_tracked_and_untracked_files_but_never_ignored_ones() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write(".gitignore", "build/\n*.log.txt\n");
    repo.write("tracked.txt", "").commit("track");
    repo.write("untracked.txt", "")
        .write("build/out.txt", "")
        .write("debug.log.txt", "");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("LIST  got tracked.txt untracked.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn reqfile_folders_are_not_targets() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write("a.txt", "")
        .write(".reqfile/LIST/notes.txt", "")
        .write("app/.reqfile/X/data.txt", "");

    let run = repo.run(&["check"]);

    assert!(run.stdout.contains("LIST  got a.txt\n"), "{}", run.stdout);
}

#[test]
fn config_globs_exclude_targets() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write(
        ".reqfile/config.yaml",
        "exclude: [\"vendor/**\", \"**/*.generated.txt\"]\n",
    );
    repo.write("a.txt", "")
        .write("vendor/lib.txt", "")
        .write("src/api.generated.txt", "")
        .write("src/b.txt", "");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("LIST  got a.txt src/b.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn excluded_paths_are_not_searched_for_reqfiles() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write(".reqfile/config.yaml", "exclude: [\"tests/fixtures/**\"]\n");
    repo.write("tests/fixtures/bad/Reqfile.yaml", "not: a reqfile\n");
    repo.write("tests/fixtures/bad/.reqfile/config.yaml", "not: a config\n");
    repo.write("tests/fixtures/.reqfile/config.yml", "exclude: []\n");
    repo.write("tests/fixtures/reqfile.yml", "reqfile: 1\n");
    repo.write(".gitignore", "ignored/\n");
    repo.write("ignored/Reqfile.yaml", "not: a reqfile\n");
    repo.write(".reqfile/LIST/Reqfile.yaml", "not: a reqfile\n");
    repo.write("a.txt", "");

    let run = repo.run(&["check"]);

    assert_eq!(run.code, 1, "{}", run.output());
    assert!(run.stdout.contains("LIST  got a.txt\n"), "{}", run.stdout);
}

#[test]
fn a_nested_config_excludes_paths_relative_to_its_folder() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write("app/.reqfile/config.yaml", "exclude: [\"gen/**\"]\n");
    repo.write("app/gen/out.txt", "")
        .write("app/src.txt", "")
        .write("gen/top.txt", "");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("LIST  got app/src.txt gen/top.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn exclusions_add_up_down_the_tree() {
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write(".reqfile/config.yaml", "exclude: [\"**/vendor/**\"]\n");
    repo.write("app/.reqfile/config.yaml", "exclude: [\"gen/**\"]\n");
    repo.write("app/vendor/lib.txt", "")
        .write("app/gen/out.txt", "")
        .write("app/src.txt", "");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("LIST  got app/src.txt\n"),
        "{}",
        run.stdout
    );
}

#[test]
fn symlinks_are_never_targets() {
    let outside = tempfile_outside();
    let repo = repo!();
    repo.write("Reqfile.yaml", &reqfile(&[("LIST", LIST_TXT)]));
    repo.write("real.txt", "");
    std::os::unix::fs::symlink(&outside, repo.path().join("secret.txt")).expect("symlink");
    std::os::unix::fs::symlink("real.txt", repo.path().join("alias.txt")).expect("symlink");

    let run = repo.run(&["check"]);

    assert!(
        run.stdout.contains("LIST  got real.txt\n"),
        "{}",
        run.stdout
    );
}

/// A file outside any repository, like a credential a symlink could point to.
fn tempfile_outside() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("reqfile-outside-{}.txt", std::process::id()));
    std::fs::write(&path, "secret").expect("write outside file");
    path
}
