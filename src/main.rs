mod core;
mod shell;

use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use crate::core::report::EXIT_ERROR;
use crate::core::reqfile::Kind;

/// Check that a codebase meets its requirements.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the checks of every Reqfile.yaml in the repository.
    Check {
        /// Only check files changed since the merge-base of BASE and HEAD
        /// (default: the nearest `base` in .reqfile/config.yaml, then origin/HEAD).
        #[arg(long, value_name = "BASE", num_args = 0..=1)]
        changed: Option<Option<String>>,
        /// Only run the listed requirements.
        #[arg(long, value_name = "ID,...", value_delimiter = ',')]
        only: Option<Vec<String>>,
        /// Only run decision checks and commands declared `fast: true`, for
        /// edit hooks; the summary counts the checks left for a full run.
        #[arg(long)]
        fast: bool,
        /// Append one JSON line per judged code unit, command check and
        /// command finding to FILE, for evaluation and replay.
        #[arg(long, value_name = "FILE")]
        log: Option<PathBuf>,
        /// Add KEY=VALUE to every line of the log, such as a session id.
        #[arg(long, value_name = "KEY=VALUE", requires = "log")]
        log_tag: Vec<String>,
        /// Try the code and process requirements of a folder (./std) or a
        /// repository (owner/repo@v1.0.0) for this run, without writing any
        /// Reqfile; repeatable, restricted by --only.
        #[arg(long = "use", value_name = "LOCATION")]
        uses: Vec<String>,
        #[arg(long, value_enum, default_value_t = Format::Summary)]
        format: Format,
    },
    /// List the requirements that apply to a path, and where each comes from.
    Explain { path: PathBuf },
    /// List every requirement of the repository, with its type, checks and source.
    List {
        /// Only list requirements of this type.
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
        #[arg(long, value_enum, default_value_t = Format::Summary)]
        format: Format,
    },
    /// Write use blocks taking requirements from a folder (./std) or a
    /// repository (owner/repo@v1.0.0, pinned to the commit of the tag) into
    /// the Reqfile of this folder, and show what their checks run.
    Add {
        location: String,
        /// Only these requirements (default: every code and process requirement there).
        ids: Vec<String>,
        /// Show the use blocks without writing them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Move every repository pinned in a Reqfile to the commit of its
    /// latest release tag.
    Update {
        /// Show what would move without writing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Measure how well each requirement's checks classify its labeled
    /// examples, `.reqfile/<ID>/examples/<name>/` (`example.yaml` and `files/`).
    #[command(alias = "test")]
    Eval {
        /// Only evaluate the listed requirements.
        #[arg(long, value_name = "ID,...", value_delimiter = ',')]
        only: Option<Vec<String>>,
        /// Evaluate the requirements of a folder or repository before
        /// adopting them, as `check --use` does.
        #[arg(long = "use", value_name = "LOCATION")]
        uses: Vec<String>,
    },
    /// Manage a requirement's labeled examples.
    Example {
        #[command(subcommand)]
        command: ExampleCommand,
    },
}

#[derive(Subcommand)]
enum ExampleCommand {
    /// Turn files of this repository into a labeled example of a
    /// requirement, such as a false alarm or a violation its checks missed.
    Add {
        /// The requirement, as it applies to the first file.
        id: String,
        /// The example's folder name under .reqfile/<ID>/examples/.
        name: String,
        /// What the requirement's checks should say about these files.
        #[arg(long, value_enum)]
        expected: ExpectedArg,
        /// A file the checks should flag, for a violation; repeatable.
        #[arg(long = "finding", value_name = "FILE")]
        findings: Vec<PathBuf>,
        /// Why the example has its label.
        #[arg(long)]
        rationale: Option<String>,
        /// The files and folders the checks need to judge the case.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ExpectedArg {
    Violation,
    Ok,
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Product,
    Code,
    Process,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Summary,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("error: cannot read the current folder: {e}");
            return ExitCode::from(EXIT_ERROR as u8);
        }
    };
    match cli.command {
        Command::Check {
            changed,
            only,
            fast,
            log,
            log_tag,
            uses,
            format,
        } => {
            let report = shell::check::run(
                &cwd,
                &shell::check::Options {
                    changed,
                    only,
                    fast,
                    log,
                    log_tags: log_tag,
                    uses,
                },
            );
            let text = match format {
                Format::Summary => report.render_summary(),
                Format::Json => report.render_json(),
            };
            match emit(&text) {
                Ok(()) => ExitCode::from(report.exit_code() as u8),
                Err(()) => ExitCode::from(EXIT_ERROR as u8),
            }
        }
        Command::Example {
            command:
                ExampleCommand::Add {
                    id,
                    name,
                    expected,
                    findings,
                    rationale,
                    files,
                },
        } => {
            let added = shell::example_add::run(
                &cwd,
                &shell::example_add::Request {
                    id,
                    name,
                    violation: matches!(expected, ExpectedArg::Violation),
                    findings,
                    rationale,
                    files,
                },
            );
            match added {
                Ok(text) => match emit(&text) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(()) => ExitCode::from(EXIT_ERROR as u8),
                },
                Err(errors) => {
                    for error in errors {
                        eprintln!("error: {error}");
                    }
                    ExitCode::from(EXIT_ERROR as u8)
                }
            }
        }
        Command::Eval { only, uses } => match shell::examples::run(&cwd, only.as_deref(), &uses) {
            Ok((text, code)) => match emit(&text) {
                Ok(()) => ExitCode::from(code as u8),
                Err(()) => ExitCode::from(EXIT_ERROR as u8),
            },
            Err(errors) => {
                for error in errors {
                    eprintln!("error: {error}");
                }
                ExitCode::from(EXIT_ERROR as u8)
            }
        },
        Command::Explain { path } => match shell::explain::run(&cwd, &path) {
            Ok(text) => match emit(&text) {
                Ok(()) => ExitCode::SUCCESS,
                Err(()) => ExitCode::from(EXIT_ERROR as u8),
            },
            Err(errors) => {
                for error in errors {
                    eprintln!("error: {error}");
                }
                ExitCode::from(EXIT_ERROR as u8)
            }
        },
        Command::Update { dry_run } => match shell::update::run(&cwd, dry_run) {
            Ok(text) => match emit(&text) {
                Ok(()) => ExitCode::SUCCESS,
                Err(()) => ExitCode::from(EXIT_ERROR as u8),
            },
            Err(errors) => {
                for error in errors {
                    eprintln!("error: {error}");
                }
                ExitCode::from(EXIT_ERROR as u8)
            }
        },
        Command::Add {
            location,
            ids,
            dry_run,
        } => match shell::add::run(&cwd, &location, &ids, dry_run) {
            Ok(text) => match emit(&text) {
                Ok(()) => ExitCode::SUCCESS,
                Err(()) => ExitCode::from(EXIT_ERROR as u8),
            },
            Err(errors) => {
                for error in errors {
                    eprintln!("error: {error}");
                }
                ExitCode::from(EXIT_ERROR as u8)
            }
        },
        Command::List { kind, format } => {
            let kind = kind.map(|k| match k {
                KindArg::Product => Kind::Product,
                KindArg::Code => Kind::Code,
                KindArg::Process => Kind::Process,
            });
            match shell::list::run(&cwd, kind, matches!(format, Format::Json)) {
                Ok(text) => match emit(&text) {
                    Ok(()) => ExitCode::SUCCESS,
                    Err(()) => ExitCode::from(EXIT_ERROR as u8),
                },
                Err(errors) => {
                    for error in errors {
                        eprintln!("error: {error}");
                    }
                    ExitCode::from(EXIT_ERROR as u8)
                }
            }
        }
    }
}

/// Writes to stdout. A reader that stops early, as in `reqfile check | head`,
/// is not an error; any other write failure is reported.
fn emit(text: &str) -> Result<(), ()> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(e) => {
            eprintln!("error: cannot write the output: {e}");
            Err(())
        }
    }
}
