//! Labeled examples of a requirement, `.reqfile/<ID>/examples/<case>/`,
//! and what running its checks on each one showed.

use super::report::{FindingKind, Report};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    /// A case the requirement's checks should flag.
    Violation,
    /// A correct case, legitimate exceptions included, that they should not flag.
    Ok,
}

/// The label of a case folder: `violation-…` or `ok-…`.
pub fn label(case: &str) -> Option<Label> {
    if case.starts_with("violation-") {
        Some(Label::Violation)
    } else if case.starts_with("ok-") {
        Some(Label::Ok)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A violation the checks flagged.
    Caught,
    /// A violation the checks looked at and did not flag.
    Missed,
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

    fn describe(&self) -> String {
        match self {
            Outcome::Caught => "caught".into(),
            Outcome::Missed => "missed".into(),
            Outcome::NotSelected => "not selected".into(),
            Outcome::Uncertain => "uncertain".into(),
            Outcome::FalseAlarm => "false alarm".into(),
            Outcome::Passed => "passed".into(),
            Outcome::Error(e) => format!("error: {e}"),
        }
    }
}

/// What the checks of requirement `id` showed on a case labeled `label`.
pub fn outcome(label: Label, id: &str, report: &Report) -> Outcome {
    if let Some(error) = report.errors.first() {
        return Outcome::Error(error.message.clone());
    }
    let kinds: Vec<FindingKind> = report
        .findings
        .iter()
        .filter(|f| f.requirement == id)
        .map(|f| f.kind)
        .collect();
    let flagged = kinds
        .iter()
        .any(|k| matches!(k, FindingKind::Violation | FindingKind::Advisory));
    let uncertain = kinds.contains(&FindingKind::Uncertain);
    // A check that found no file or code unit does not count as run, so any
    // check run means something examined the case.
    let looked = report.checks_run > 0;
    match (label, flagged, uncertain) {
        (Label::Violation, true, _) => Outcome::Caught,
        (Label::Violation, false, true) => Outcome::Uncertain,
        (Label::Violation, false, false) if !looked => Outcome::NotSelected,
        (Label::Violation, false, false) => Outcome::Missed,
        (Label::Ok, true, _) => Outcome::FalseAlarm,
        (Label::Ok, false, true) => Outcome::Uncertain,
        (Label::Ok, false, false) => Outcome::Passed,
    }
}

/// The examples of one requirement.
pub struct Tested {
    pub id: String,
    /// With a decision check, model judgments are measured rather than asserted.
    pub measured: bool,
    pub cases: Vec<(String, Label, Outcome)>,
}

impl Tested {
    /// Whether an asserted requirement disagreed with one of its labels.
    pub fn failed(&self) -> bool {
        !self.measured && self.cases.iter().any(|(_, _, o)| !o.as_labeled())
    }

    pub fn has_errors(&self) -> bool {
        self.cases
            .iter()
            .any(|(_, _, o)| matches!(o, Outcome::Error(_)))
    }

    fn render(&self) -> String {
        let count = |label: Label, outcome: &Outcome| {
            self.cases
                .iter()
                .filter(|(_, l, o)| *l == label && o == outcome)
                .count()
        };
        let total = |label: Label| self.cases.iter().filter(|(_, l, _)| *l == label).count();
        let mut out = if self.measured {
            format!(
                "{}  {} examples, measured: {} of {} violations caught ({} missed, {} not selected, {} uncertain); {} of {} correct examples flagged ({} uncertain)\n",
                self.id,
                self.cases.len(),
                count(Label::Violation, &Outcome::Caught),
                total(Label::Violation),
                count(Label::Violation, &Outcome::Missed),
                count(Label::Violation, &Outcome::NotSelected),
                count(Label::Violation, &Outcome::Uncertain),
                count(Label::Ok, &Outcome::FalseAlarm),
                total(Label::Ok),
                count(Label::Ok, &Outcome::Uncertain),
            )
        } else {
            let as_labeled = self.cases.iter().filter(|(_, _, o)| o.as_labeled()).count();
            format!(
                "{}  {} examples, {} as labeled\n",
                self.id,
                self.cases.len(),
                as_labeled
            )
        };
        for (case, _, outcome) in self.cases.iter().filter(|(_, _, o)| !o.as_labeled()) {
            out += &format!("  {case}: {}\n", outcome.describe());
        }
        out
    }
}

/// The report of `reqfile test`, and its exit code: 3 if a case could not
/// run, 1 if an asserted requirement disagreed with a label, 0 otherwise.
pub fn render(tested: &[Tested]) -> (String, i32) {
    let mut out: String = tested.iter().map(Tested::render).collect();
    let examples: usize = tested.iter().map(|t| t.cases.len()).sum();
    let failed = tested.iter().filter(|t| t.failed()).count();
    if !out.is_empty() {
        out.push('\n');
    }
    out += &format!(
        "{} requirements with examples, {examples} examples. {failed} failing their labels.\n",
        tested.len()
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
