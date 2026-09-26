//! The result of a run: findings, errors, counts, how they read, and the exit code.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingKind {
    /// A violation that fails the run.
    Violation,
    /// A violation reported by an advisory decision check.
    Advisory,
    /// A decision the model was unsure about; always advisory.
    Uncertain,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub requirement: String,
    pub kind: FindingKind,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
    pub fix_hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<f64>,
    /// The exact model that judged a decision finding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// For a requirement taken with `use`, the use block, `path:line`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// For a requirement taken with `use`, where its definition was resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub requirement: Option<String>,
    pub message: String,
    /// Whether the error fails the run: an advisory check that could not
    /// run could never have blocked, so its error is reported without
    /// changing the exit code.
    pub blocking: bool,
}

impl Error {
    /// An error that fails the run.
    pub fn blocking(requirement: Option<String>, message: String) -> Self {
        Self {
            requirement,
            message,
            blocking: true,
        }
    }
}

/// A git source of use blocks, with the commit its ref resolved to.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub location: String,
    pub commit: String,
    /// The ref could not be resolved again; the last cached resolution was used.
    pub offline: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub checks_run: usize,
    /// Checks with no file or code unit to look at in the selection.
    pub checks_with_nothing_to_check: usize,
    /// Checks `--fast` left for a full run.
    pub checks_left_for_full_run: usize,
    /// Code units decision checks got an answer about; absent without decision checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units_judged: Option<usize>,
    pub violations: usize,
    pub advisory_findings: usize,
    pub errors: usize,
    /// Errors of advisory checks, counted in `errors`, which leave the exit code unchanged.
    pub advisory_errors: usize,
}

#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub errors: Vec<Error>,
    pub sources: Vec<Source>,
    pub checks_run: usize,
    pub checks_with_nothing_to_check: usize,
    pub checks_left_for_full_run: usize,
    pub units_judged: Option<usize>,
}

pub const EXIT_PASS: i32 = 0;
pub const EXIT_VIOLATIONS: i32 = 1;
pub const EXIT_ERROR: i32 = 3;

impl Report {
    pub fn from_errors(messages: impl IntoIterator<Item = String>) -> Self {
        Self {
            errors: messages
                .into_iter()
                .map(|message| Error::blocking(None, message))
                .collect(),
            ..Self::default()
        }
    }

    pub fn summary(&self) -> Summary {
        let violations = self
            .findings
            .iter()
            .filter(|f| f.kind == FindingKind::Violation)
            .count();
        Summary {
            checks_run: self.checks_run,
            checks_with_nothing_to_check: self.checks_with_nothing_to_check,
            checks_left_for_full_run: self.checks_left_for_full_run,
            units_judged: self.units_judged,
            violations,
            advisory_findings: self.findings.len() - violations,
            errors: self.errors.len(),
            advisory_errors: self.errors.iter().filter(|e| !e.blocking).count(),
        }
    }

    /// 3 on any blocking error, which takes precedence; 1 on blocking
    /// violations; 0 otherwise.
    pub fn exit_code(&self) -> i32 {
        let summary = self.summary();
        if summary.errors > summary.advisory_errors {
            EXIT_ERROR
        } else if summary.violations > 0 {
            EXIT_VIOLATIONS
        } else {
            EXIT_PASS
        }
    }

    pub fn render_summary(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            let mut columns: Vec<String> = Vec::new();
            match finding.kind {
                FindingKind::Violation => {}
                FindingKind::Advisory => columns.push("advisory".into()),
                FindingKind::Uncertain => columns.push("uncertain".into()),
            }
            columns.push(finding.requirement.clone());
            match (&finding.file, finding.line) {
                (Some(file), Some(line)) => columns.push(format!("{file}:{line}")),
                (Some(file), None) => columns.push(file.clone()),
                _ => {}
            }
            if let Some(p) = finding.probability {
                columns.push(format!("p={p:.2}"));
            }
            let (first, rest) = split_first_line(&finding.message);
            columns.push(first.to_string());
            out += &format!("{}\n", columns.join("  "));
            for line in rest.lines() {
                out += &format!("    {line}\n");
            }
            out += &format!("  fix: {}\n", finding.fix_hint);
            if let (Some(block), Some(source)) = (&finding.block, &finding.source) {
                out += &format!("  from: {block}, use {source}\n");
            }
        }
        for error in &self.errors {
            let (first, rest) = split_first_line(&error.message);
            let label = if error.blocking {
                "error"
            } else {
                "advisory error"
            };
            match &error.requirement {
                Some(id) => {
                    out += &format!("{label}  {id}  {first}\n");
                }
                None => {
                    out += &format!("{label}  {first}\n");
                }
            }
            for line in rest.lines() {
                out += &format!("    {line}\n");
            }
        }
        for source in &self.sources {
            let offline = if source.offline {
                " (cannot be reached: using the last resolution cached here)"
            } else {
                ""
            };
            out += &format!(
                "source  {} resolved to commit {}{offline}\n",
                source.location, source.commit
            );
        }
        if !out.is_empty() {
            out.push('\n');
        }
        let s = self.summary();
        let units = s
            .units_judged
            .map(|n| format!(", {}", plural(n, "code unit judged", "code units judged")))
            .unwrap_or_default();
        let deferred = match s.checks_left_for_full_run {
            0 => String::new(),
            n => format!(", {n} left for a full run"),
        };
        let advisory_errors = match s.advisory_errors {
            0 => String::new(),
            n => format!(" ({n} advisory)"),
        };
        out += &format!(
            "{} run, {} with nothing to check{units}{deferred}. {}, {}, {}{advisory_errors}.\n",
            plural(s.checks_run, "check", "checks"),
            s.checks_with_nothing_to_check,
            plural(s.violations, "violation", "violations"),
            plural(s.advisory_findings, "advisory finding", "advisory findings"),
            plural(s.errors, "error", "errors"),
        );
        out
    }

    pub fn render_json(&self) -> String {
        #[derive(Serialize)]
        struct Json<'r> {
            findings: &'r [Finding],
            errors: &'r [Error],
            #[serde(skip_serializing_if = "<[Source]>::is_empty")]
            sources: &'r [Source],
            summary: Summary,
            exit_code: i32,
        }
        let json = Json {
            findings: &self.findings,
            errors: &self.errors,
            sources: &self.sources,
            summary: self.summary(),
            exit_code: self.exit_code(),
        };
        serde_json::to_string_pretty(&json).expect("the report serializes to JSON") + "\n"
    }
}

fn split_first_line(text: &str) -> (&str, &str) {
    let text = text.trim_end();
    match text.split_once('\n') {
        Some((first, rest)) => (first, rest),
        None => (text, ""),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
