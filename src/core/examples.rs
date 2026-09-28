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
    /// `split: holdout`: kept out of tuning, so its rates say whether a
    /// change to the checks generalizes or only fits the examples it was
    /// made for.
    pub holdout: bool,
}

/// Parses `example.yaml`: `expected` (`violation` or `ok`), and optionally
/// `findings`, `rationale`, `origin`, `known` and `split`.
pub fn parse(path: &str, text: &str) -> Result<Example, ConfigError> {
    let root = yaml::parse(path, text)?;
    let mut fields = root.fields(
        path,
        EXAMPLE_FILE,
        &[
            "expected",
            "findings",
            "rationale",
            "origin",
            "known",
            "split",
        ],
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
    let holdout = match fields.optional("split") {
        None => false,
        Some(node) => {
            let line = node.line;
            match node.text(path, "split")?.as_str() {
                "tune" => false,
                "holdout" => true,
                other => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!("unknown `split: {other}`; expected `tune` or `holdout`"),
                    ));
                }
            }
        }
    };
    Ok(Example {
        expected,
        findings,
        known,
        holdout,
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

/// One example and what the checks showed on it, run after run.
pub struct Case {
    pub name: String,
    pub label: Label,
    pub known: Option<Known>,
    pub holdout: bool,
    /// One outcome per run, at least one.
    pub outcomes: Vec<Outcome>,
}

impl Case {
    /// As labeled, or failing the way `known` says, on every run.
    fn accepted(&self) -> bool {
        self.outcomes
            .iter()
            .all(|o| o.as_labeled() || self.known.is_some_and(|k| o.is(k)))
    }

    /// Whether every run gave the same outcome.
    fn stable(&self) -> bool {
        self.outcomes.windows(2).all(|w| w[0] == w[1])
    }

    /// The outcome counted in the rates: the most frequent, and on a tie
    /// the one that disagrees with the label, so noise never reads as a catch.
    fn outcome(&self) -> &Outcome {
        let times = |o: &Outcome| self.outcomes.iter().filter(|p| *p == o).count();
        self.outcomes
            .iter()
            .max_by_key(|o| (times(o), !o.as_labeled()))
            .expect("a case has at least one outcome")
    }

    /// The outcomes of an unstable case, most frequent first:
    /// `caught 2, missed 1`.
    fn spread(&self) -> String {
        let mut seen: Vec<(String, usize)> = Vec::new();
        for outcome in &self.outcomes {
            let described = outcome.describe();
            match seen.iter_mut().find(|(d, _)| *d == described) {
                Some((_, n)) => *n += 1,
                None => seen.push((described, 1)),
            }
        }
        seen.sort_by(|a, b| b.1.cmp(&a.1));
        seen.iter()
            .map(|(d, n)| format!("{d} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render(&self) -> Option<String> {
        let name = if self.holdout {
            format!("{} (held out)", self.name)
        } else {
            self.name.clone()
        };
        let known = if self.accepted() { " (known)" } else { "" };
        if !self.stable() {
            return Some(format!(
                "  {name}: unstable over {} runs: {}{known}\n",
                self.outcomes.len(),
                self.spread()
            ));
        }
        let outcome = &self.outcomes[0];
        match (outcome.as_labeled(), self.known.is_some()) {
            (false, _) => Some(format!("  {name}: {}{known}\n", outcome.describe())),
            // Known to fail, yet as labeled: the check improved.
            (true, true) => Some(format!(
                "  {name}: {}, as labeled now: remove `known`\n",
                outcome.describe()
            )),
            (true, false) => None,
        }
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
            .any(|c| c.outcomes.iter().any(|o| matches!(o, Outcome::Error(_))))
    }

    fn unstable(&self) -> usize {
        self.cases.iter().filter(|c| !c.stable()).count()
    }

    fn render(&self) -> String {
        let mut out = match self.evidence {
            Evidence::None => format!("{}  no evidence: no labeled examples\n", self.label),
            // Held-out examples are reported apart: a change that raises the
            // tuning rates alone fits its examples rather than the requirement.
            Evidence::Measured if self.cases.iter().any(|c| c.holdout) => {
                let (held, tuned): (Vec<&Case>, Vec<&Case>) =
                    self.cases.iter().partition(|c| c.holdout);
                format!(
                    "{}  measured, no assertion\n  tuning: {}\n  held out: {}\n",
                    self.label,
                    rates(&tuned),
                    rates(&held)
                )
            }
            Evidence::Measured => format!(
                "{}  measured, no assertion: {}\n",
                self.label,
                rates(&self.cases.iter().collect::<Vec<_>>())
            ),
            Evidence::Asserted => {
                let as_labeled = self
                    .cases
                    .iter()
                    .filter(|c| c.outcomes.iter().all(Outcome::as_labeled))
                    .count();
                format!(
                    "{}  asserted: {} examples, {} as labeled\n",
                    self.label,
                    self.cases.len(),
                    as_labeled
                )
            }
        };
        out.extend(self.cases.iter().filter_map(Case::render));
        // Examples of one class only measure half of what a check does.
        let has = |label: Label| self.cases.iter().any(|c| c.label == label);
        if has(Label::Violation) && !has(Label::Ok) {
            out += "  no correct examples: false alarms are not measured\n";
        }
        if has(Label::Ok) && !has(Label::Violation) {
            out += "  no violation examples: catches are not measured\n";
        }
        out
    }
}

/// Detection rates on some cases: `4 of 5 violations caught (…); 0 of 6
/// correct examples flagged (…)`.
fn rates(cases: &[&Case]) -> String {
    let count = |label: Label, outcome: fn(&Outcome) -> bool| {
        cases
            .iter()
            .filter(|c| c.label == label && outcome(c.outcome()))
            .count()
    };
    let total = |label: Label| cases.iter().filter(|c| c.label == label).count();
    let caught = count(Label::Violation, |o| *o == Outcome::Caught);
    let flagged = count(Label::Ok, |o| *o == Outcome::FalseAlarm);
    format!(
        "{caught} of {} violations caught ({}{} missed, {} not selected, {} uncertain); {flagged} of {} correct examples flagged ({}{} uncertain)",
        total(Label::Violation),
        interval(caught, total(Label::Violation)),
        count(Label::Violation, |o| matches!(
            o,
            Outcome::Missed | Outcome::WrongFile(_)
        )),
        count(Label::Violation, |o| *o == Outcome::NotSelected),
        count(Label::Violation, |o| *o == Outcome::Uncertain),
        total(Label::Ok),
        interval(flagged, total(Label::Ok)),
        count(Label::Ok, |o| *o == Outcome::Uncertain),
    )
}

/// The 95% Wilson interval of a rate of `k` in `n`, as `95% interval 41 to
/// 93%; `, or nothing without cases: a handful of examples says little, and a
/// change to the checks that stays inside it may be chance.
fn interval(k: usize, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let (k, n) = (k as f64, n as f64);
    let z2 = 1.96_f64 * 1.96;
    let rate = k / n;
    let center = (rate + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let half = (z2 * (rate * (1.0 - rate) / n + z2 / (4.0 * n * n))).sqrt() / (1.0 + z2 / n);
    let percent = |x: f64| (x.clamp(0.0, 1.0) * 100.0).round();
    format!(
        "95% interval {} to {}%; ",
        percent(center - half),
        percent(center + half)
    )
}

/// The report of `reqfile eval` over `runs` runs of each case, and its exit
/// code: 3 if a case could not run, 1 if an asserted requirement disagreed
/// with a label on any run, 0 otherwise. Measured and absent evidence never
/// fail: exit 0 does not mean a decision check works.
pub fn render(tested: &[Tested], runs: usize) -> (String, i32) {
    let mut out: String = tested.iter().map(Tested::render).collect();
    let examples: usize = tested.iter().map(|t| t.cases.len()).sum();
    let failed = tested.iter().filter(|t| t.failed()).count();
    let with = |evidence: Evidence| tested.iter().filter(|t| t.evidence == evidence).count();
    if !out.is_empty() {
        out.push('\n');
    }
    // Cases whose outcome changes from run to run: the noise any change
    // to the checks must exceed before it counts as an improvement.
    let repeated = if runs > 1 {
        let unstable: usize = tested.iter().map(Tested::unstable).sum();
        format!(", {runs} runs each, {unstable} unstable")
    } else {
        String::new()
    };
    out += &format!(
        "{} requirements: {} asserted, {} measured, {} without evidence; {examples} examples{repeated}. {failed} failing their labels.\n",
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
