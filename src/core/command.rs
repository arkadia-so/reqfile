//! What a finished command means: pass, violations, or a tool error.

use super::reqfile::{CommandCheck, OutputFormat};
use super::sarif;

/// Lines of output an exit-format violation carries.
const OUTPUT_TAIL_LINES: usize = 50;

pub enum Outcome {
    Exited {
        code: i32,
        stdout: String,
        stderr: String,
    },
    Signaled(i32),
    TimedOut,
    LaunchFailed(String),
}

#[derive(Debug, PartialEq)]
pub struct CommandViolation {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

/// The violations a command reported, or why its result cannot be trusted.
/// `cwd` is the repository folder it ran in and `repo_root` the absolute root.
pub fn judge(
    check: &CommandCheck,
    outcome: Outcome,
    cwd: &str,
    repo_root: &str,
) -> Result<Vec<CommandViolation>, String> {
    let (code, stdout, stderr) = match outcome {
        Outcome::Exited {
            code,
            stdout,
            stderr,
        } => (code, stdout, stderr),
        Outcome::Signaled(signal) => {
            return Err(format!("`{}` was killed by signal {signal}", check.run));
        }
        Outcome::TimedOut => {
            return Err(format!(
                "`{}` timed out after {} seconds",
                check.run, check.timeout_secs
            ));
        }
        Outcome::LaunchFailed(e) => {
            return Err(format!("`{}` could not be launched: {e}", check.run));
        }
    };
    if code != 0 && !check.violation_codes.contains(&code) {
        return Err(format!(
            "`{}` failed with exit code {code}\n{}",
            check.run,
            tail(&format!("{stdout}{stderr}"))
        ));
    }
    match check.format {
        OutputFormat::Exit if code == 0 => Ok(Vec::new()),
        OutputFormat::Exit => Ok(vec![CommandViolation {
            message: tail(&stdout),
            file: None,
            line: None,
        }]),
        OutputFormat::Sarif => {
            let parsed = sarif::parse(&stdout, cwd, repo_root)
                .map_err(|e| format!("`{}`: {e}", check.run))?;
            if code != 0 && !parsed.had_results {
                return Err(format!(
                    "`{}` exited with violation code {code} but its SARIF output lists no results",
                    check.run
                ));
            }
            Ok(parsed
                .violations
                .into_iter()
                .map(|r| CommandViolation {
                    message: r.message,
                    file: r.file,
                    line: r.line,
                })
                .collect())
        }
    }
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.trim_end().lines().collect();
    let start = lines.len().saturating_sub(OUTPUT_TAIL_LINES);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        "(no output)".to_string()
    } else {
        tail
    }
}
