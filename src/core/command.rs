//! What a finished command means: pass, violations, or a tool error.

use super::junit;
use super::reqfile::{CommandCheck, OutputFormat, Thresholds};
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
    /// The probability of violation the command gave, if it gave one.
    pub probability: Option<f64>,
    /// Between the thresholds: an advisory finding, not a violation.
    pub uncertain: bool,
}

impl CommandViolation {
    fn certain(message: String, file: Option<String>, line: Option<usize>) -> Self {
        Self {
            message,
            file,
            line,
            probability: None,
            uncertain: false,
        }
    }
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
        OutputFormat::Exit => Ok(vec![CommandViolation::certain(tail(&stdout), None, None)]),
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
                .filter_map(|r| judge_probability(r, check.thresholds))
                .collect())
        }
        OutputFormat::Junit => {
            let parsed = junit::parse(&stdout, cwd, repo_root)
                .map_err(|e| format!("`{}`: {e}", check.run))?;
            // A run that reports no test at all proved nothing.
            if !parsed.had_tests {
                return Err(format!("`{}`: the JUnit report lists no test", check.run));
            }
            if code != 0 && parsed.failures.is_empty() {
                return Err(format!(
                    "`{}` exited with violation code {code} but its JUnit report lists no failed test",
                    check.run
                ));
            }
            Ok(parsed
                .failures
                .into_iter()
                .map(|f| {
                    let message = match f.message {
                        Some(m) => format!("test {} failed: {m}", f.test),
                        None => format!("test {} failed", f.test),
                    };
                    CommandViolation::certain(message, f.file, f.line)
                })
                .collect())
        }
    }
}

/// A SARIF result as a finding: without a probability, a violation; with
/// one, a violation above `violation_above`, nothing below `pass_below`, and
/// an uncertain finding between.
fn judge_probability(
    result: sarif::SarifResult,
    thresholds: Thresholds,
) -> Option<CommandViolation> {
    let uncertain = match result.probability {
        None => false,
        Some(p) if p > thresholds.violation_above => false,
        Some(p) if p < thresholds.pass_below => return None,
        Some(_) => true,
    };
    Some(CommandViolation {
        message: result.message,
        file: result.file,
        line: result.line,
        probability: result.probability,
        uncertain,
    })
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
