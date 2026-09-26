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
        #[arg(long, value_enum, default_value_t = Format::Summary)]
        format: Format,
    },
    /// List the requirements that apply to a path.
    Explain { path: PathBuf },
    /// List every requirement of the repository, with its type and checks.
    List {
        /// Only list requirements of this type.
        #[arg(long, value_enum)]
        kind: Option<KindArg>,
    },
    /// Run each requirement's checks on its labeled examples,
    /// `.reqfile/<ID>/examples/violation-…/` and `ok-…/`.
    Test {
        /// Only test the listed requirements.
        #[arg(long, value_name = "ID,...", value_delimiter = ',')]
        only: Option<Vec<String>>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum KindArg {
    Product,
    Code,
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
        Command::Test { only } => match shell::examples::run(&cwd, only.as_deref()) {
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
        Command::List { kind } => {
            let kind = kind.map(|k| match k {
                KindArg::Product => Kind::Product,
                KindArg::Code => Kind::Code,
            });
            match shell::list::run(&cwd, kind) {
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
