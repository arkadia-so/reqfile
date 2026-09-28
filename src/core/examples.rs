//! Labeled examples of a requirement, `.reqfile/<ID>/examples/<name>/`:
//! `example.yaml` says what the checks should report, and `files/` holds the
//! case, the only part the checks see, so no check can read its answer. And
//! what running the checks on each case showed.

use super::error::ConfigError;
use super::report::{FindingKind, Report};
use super::yaml;

/// The file of an example that holds its label.
pub const EXAMPLE_FILE: &str = "example.yaml";
/// The folder of an example that holds its case.
pub const FILES_DIR: &str = "files";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    /// A case the requirement's checks should flag.
    Violation,
    /// A correct case, legitimate exceptions included, that they should not flag.
    Ok,
}

/// A failure the checks are known to have on an example, tracked without
/// failing an asserted requirement, like Semgrep's `todoruleid`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Known {
    Miss,
    FalseAlarm,
}

/// What `example.yaml` says.
#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    pub expected: Label,
    /// Files, relative to `files/`, the checks should flag on a violation.
    pub findings: Vec<String>,
    pub known: Option<Known>,
}

/// Parses `example.yaml`: `expected` (`violation` or `ok`), and optionally
/// `findings`, `rationale`, `origin` and `known`.
pub fn parse(path: &str, text: &str) -> Result<Example, ConfigError> {
    let root = yaml::parse(path, text)?;
    let mut fields = root.fields(
        path,
        EXAMPLE_FILE,
        &["expected", "findings", "rationale", "origin", "known"],
    )?;
    let node = fields.required("expected")?;
    let line = node.line;
    let expected = match node.text(path, "expected")?.as_str() {
        "violation" => Label::Violation,
        "ok" => Label::Ok,
        other => {
            return Err(ConfigError::at(
                path,
                line,
                format!("unknown `expected: {other}`; expected `violation` or `ok`"),
            ));
        }
    };
    let findings = match fields.optional("findings") {
        None => Vec::new(),
        Some(node) => {
            let line = node.line;
            let findings = node
                .list(path, "findings")?
                .into_iter()
                .map(|f| f.text(path, "findings"))
                .collect::<Result<Vec<_>, _>>()?;
            if expected == Label::Ok && !findings.is_empty() {
                return Err(ConfigError::at(
                    path,
                    line,
                    "`findings` name where a violation is flagged; an `ok` example has none",
                ));
            }
            findings
        }
    };
    for key in ["rationale", "origin"] {
        if let Some(node) = fields.optional(key) {
            node.text(path, key)?;
        }
    }
    let known = match fields.optional("known") {
        None => None,
        Some(node) => {
            let line = node.line;
            let known = match (node.text(path, "known")?.as_str(), expected) {
                ("miss", Label::Violation) => Known::Miss,
                ("false_alarm", Label::Ok) => Known::FalseAlarm,
                (other, _) => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!(
                            "`known: {other}` does not fit `expected: {}`; a violation can be a known `miss`, an ok example a known `false_alarm`",
                            match expected {
                                Label::Violation => "violation",
                                Label::Ok => "ok",
                            }
                        ),
                    ));
                }
            };
            Some(known)
        }
    };
    Ok(Example {
        expected,
        findings,
        known,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A violation the checks flagged, where expected.
    Caught,
    /// A violation the checks looked at and did not flag.
    Missed,
    /// A violation the checks flagged elsewhere than its expected findings.
    WrongFile(Vec<String>),
    /// A violation no check looked at: no file matched, or no code unit was selected.
    NotSelected,
    /// The model was unsure, violation or not.
    Uncertain,
    /// A correct case the checks flagged.
    FalseAlarm,
    /// A correct case the checks did not flag.
    Passed,
    /// The checks could not run.
    Error(String),
}

impl Outcome {
    fn as_labeled(&self) -> bool {
        matches!(self, Outcome::Caught | Outcome::Passed)
    }

    /// Whether this is the failure `known` says the checks have.
    fn is(&self, known: Known) -> bool {
        match known {
            Known::Miss => matches!(
                self,
                Outcome::Missed | Outcome::WrongFile(_) | Outcome::NotSelected | Outcome::Uncertain
            ),
            Known::FalseAlarm => matches!(self, Outcome::FalseAlarm | Outcome::Uncertain),
        }
    }

    fn describe(&self) -> String {
        match self {
            Outcome::Caught => "caught".into(),
            Outcome::Missed => "missed".into(),
            Outcome::WrongFile(flagged) if flagged.is_empty() => {
                "missed: flagged without a file, so its expected findings cannot be confirmed"
                    .into()
            }
            Outcome::WrongFile(flagged) => format!(
                "missed: flagged {} instead of its expected findings",
                flagged.join(", ")
            ),
            Outcome::NotSelected => "not selected".into(),
            Outcome::Uncertain => "uncertain".into(),
            Outcome::FalseAlarm => "false alarm".into(),
            Outcome::Passed => "passed".into(),
            Outcome::Error(e) => format!("error: {e}"),
        }
    }
}

/// What the checks of requirement `id` showed on a case.
pub fn outcome(example: &Example, id: &str, report: &Report) -> Outcome {
    if let Some(error) = report.errors.first() {
        return Outcome::Error(error.message.clone());
    }
    let own: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.requirement == id)
        .collect();
    let flags: Vec<_> = own
        .iter()
        .filter(|f| matches!(f.kind, FindingKind::Violation | FindingKind::Advisory))
        .collect();
    let uncertain = own.iter().any(|f| f.kind == FindingKind::Uncertain);
    // A check that found no file or code unit does not count as run, so any
    // check run means something examined the case.
    let looked = report.checks_run > 0;
    match (example.expected, !flags.is_empty(), uncertain) {
        (Label::Violation, true, _) => {
            let flagged: Vec<String> = flags.iter().filter_map(|f| f.file.clone()).collect();
            if example.findings.iter().all(|f| flagged.contains(f)) {
                Outcome::Caught
            } else {
                let mut flagged = flagged;
                flagged.dedup();
                Outcome::WrongFile(flagged)
            }
        }
        (Label::Violation, false, true) => Outcome::Uncertain,
        (Label::Violation, false, false) if !looked => Outcome::NotSelected,
        (Label::Violation, false, false) => Outcome::Missed,
        (Label::Ok, true, _) => Outcome::FalseAlarm,
        (Label::Ok, false, true) => Outcome::Uncertain,
        (Label::Ok, false, false) => Outcome::Passed,
    }
}

/// One example and what the checks showed on it.
pub struct Case {
    pub name: String,
    pub label: Label,
    pub known: Option<Known>,
    pub outcome: Outcome,
}

impl Case {
    /// As labeled, or failing the way `known` says.
    fn accepted(&self) -> bool {
        self.outcome.as_labeled() || self.known.is_some_and(|k| self.outcome.is(k))
    }
}

/// What a requirement's labeled examples say about its checks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Evidence {
    /// Deterministic checks only: every label must match.
    Asserted,
    /// A decision check, or a command reporting probabilities: judgments
    /// vary, so they are measured against the labels, never verified by them.
    Measured,
    /// No labeled examples: nothing is known about the checks.
    None,
}

/// The examples of one requirement.
pub struct Tested {
    /// The requirement id, with its Reqfile when several blocks share it.
    pub label: String,
    pub evidence: Evidence,
    pub cases: Vec<Case>,
}

impl Tested {
    /// Whether an asserted requirement disagreed with one of its labels.
    pub fn failed(&self) -> bool {
        self.evidence == Evidence::Asserted && self.cases.iter().any(|c| !c.accepted())
    }

    pub fn has_errors(&self) -> bool {
        self.cases
            .iter()
            .any(|c| matches!(c.outcome, Outcome::Error(_)))
    }

    fn render(&self) -> String {
        let count = |label: Label, outcome: fn(&Outcome) -> bool| {
            self.cases
                .iter()
                .filter(|c| c.label == label && outcome(&c.outcome))
                .count()
        };
        let total = |label: Label| self.cases.iter().filter(|c| c.label == label).count();
        let mut out = match self.evidence {
            Evidence::None => format!("{}  no evidence: no labeled examples\n", self.label),
            Evidence::Measured => format!(
                "{}  measured, no assertion: {} of {} violations caught ({} missed, {} not selected, {} uncertain); {} of {} correct examples flagged ({} uncertain)\n",
                self.label,
                count(Label::Violation, |o| *o == Outcome::Caught),
                total(Label::Violation),
                count(Label::Violation, |o| matches!(
                    o,
                    Outcome::Missed | Outcome::WrongFile(_)
                )),
                count(Label::Violation, |o| *o == Outcome::NotSelected),
                count(Label::Violation, |o| *o == Outcome::Uncertain),
                count(Label::Ok, |o| *o == Outcome::FalseAlarm),
                total(Label::Ok),
                count(Label::Ok, |o| *o == Outcome::Uncertain),
            ),
            Evidence::Asserted => {
                let as_labeled = self.cases.iter().filter(|c| c.outcome.as_labeled()).count();
                format!(
                    "{}  asserted: {} examples, {} as labeled\n",
                    self.label,
                    self.cases.len(),
                    as_labeled
                )
            }
        };
        for case in &self.cases {
            match (case.outcome.as_labeled(), case.accepted()) {
                (false, false) => out += &format!("  {}: {}\n", case.name, case.outcome.describe()),
                (false, true) => {
                    out += &format!("  {}: {} (known)\n", case.name, case.outcome.describe())
                }
                // Known to fail, yet as labeled: the check improved.
                (true, _) if case.known.is_some() => {
                    out += &format!(
                        "  {}: {}, as labeled now: remove `known`\n",
                        case.name,
                        case.outcome.describe()
                    )
                }
                (true, _) => {}
            }
        }
        out
    }
}

/// The report of `reqfile eval`, and its exit code: 3 if a case could not
/// run, 1 if an asserted requirement disagreed with a label, 0 otherwise.
/// Measured and absent evidence never fail: exit 0 does not mean a decision
/// check works.
pub fn render(tested: &[Tested]) -> (String, i32) {
    let mut out: String = tested.iter().map(Tested::render).collect();
    let examples: usize = tested.iter().map(|t| t.cases.len()).sum();
    let failed = tested.iter().filter(|t| t.failed()).count();
    let with = |evidence: Evidence| tested.iter().filter(|t| t.evidence == evidence).count();
    if !out.is_empty() {
        out.push('\n');
    }
    out += &format!(
        "{} requirements: {} asserted, {} measured, {} without evidence; {examples} examples. {failed} failing their labels.\n",
        tested.len(),
        with(Evidence::Asserted),
        with(Evidence::Measured),
        with(Evidence::None),
    );
    let code = if tested.iter().any(Tested::has_errors) {
        3
    } else if failed > 0 {
        1
    } else {
        0
    };
    (out, code)
}
