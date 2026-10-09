//! Labeled examples of a requirement, `.reqfile/<ID>/examples/<name>/`:
//! `example.yaml` says what the checks should report, and `files/` holds the
//! case, the only part the checks see, so no check can read its answer. And
//! what running the checks on each case showed.

use super::error::ConfigError;
use super::pretty::{Style, plural};
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
    /// The split it belongs to, other than the default `tune`: `holdout`,
    /// kept out of tuning, so its rates say whether a change to the checks
    /// generalizes or only fits the examples it was made for, or any other
    /// name, such as a confirmation set, reported apart by that name.
    pub split: Option<String>,
}

/// The split of examples a change to the checks is tuned on.
pub const DEFAULT_SPLIT: &str = "tune";
/// The split of examples kept out of tuning.
pub const HOLDOUT: &str = "holdout";

/// Whether `name` can name a split: lowercase letters, digits and dashes.
fn valid_split(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
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
    let split = match fields.optional("split") {
        None => None,
        Some(node) => {
            let line = node.line;
            match node.text(path, "split")?.as_str() {
                DEFAULT_SPLIT => None,
                name if valid_split(name) => Some(name.to_string()),
                other => {
                    return Err(ConfigError::at(
                        path,
                        line,
                        format!(
                            "invalid `split: {other}`; expected `tune` (the default), `holdout` or another name of lowercase letters, digits and dashes"
                        ),
                    ));
                }
            }
        }
    };
    Ok(Example {
        expected,
        findings,
        known,
        split,
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

    /// Its name in JSON output.
    fn key(&self) -> &'static str {
        match self {
            Outcome::Caught => "caught",
            Outcome::Missed => "missed",
            Outcome::WrongFile(_) => "flagged_elsewhere",
            Outcome::NotSelected => "not_selected",
            Outcome::Uncertain => "uncertain",
            Outcome::FalseAlarm => "false_alarm",
            Outcome::Passed => "passed",
            Outcome::Error(_) => "error",
        }
    }
}

/// The files the checks of requirement `id` flagged on a case, violations
/// and advisory findings, sorted and without duplicates.
pub fn flagged(id: &str, report: &Report) -> Vec<String> {
    let mut files: Vec<String> = report
        .findings
        .iter()
        .filter(|f| {
            f.requirement == id && matches!(f.kind, FindingKind::Violation | FindingKind::Advisory)
        })
        .filter_map(|f| f.file.clone())
        .collect();
    files.sort();
    files.dedup();
    files
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
    /// Its split, `None` for the default one, `tune`.
    pub split: Option<String>,
    /// One outcome per run, at least one.
    pub outcomes: Vec<Outcome>,
    /// The files flagged on each run, in the order of `outcomes`.
    pub flagged: Vec<Vec<String>>,
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
        seen.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        seen.iter()
            .map(|(d, n)| format!("{d} {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render(&self) -> Option<String> {
        let name = match self.split.as_deref() {
            None => self.name.clone(),
            Some(HOLDOUT) => format!("{} (held out)", self.name),
            Some(split) => format!("{} ({split})", self.name),
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
    pub id: String,
    /// The Reqfile of its block.
    pub reqfile: String,
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

    /// Its cases by split: the default one first, even when empty, then
    /// `holdout`, then the others by name.
    fn splits(&self) -> Vec<(Option<&str>, Vec<&Case>)> {
        let mut names: Vec<&str> = self
            .cases
            .iter()
            .filter_map(|c| c.split.as_deref())
            .collect();
        names.sort_by_key(|name| (*name != HOLDOUT, *name));
        names.dedup();
        std::iter::once(None)
            .chain(names.into_iter().map(Some))
            .map(|split| {
                let cases = self
                    .cases
                    .iter()
                    .filter(|c| c.split.as_deref() == split)
                    .collect();
                (split, cases)
            })
            .collect()
    }

    fn render(&self) -> String {
        let mut out = match self.evidence {
            Evidence::None => format!("{}  no evidence: no labeled examples\n", self.label),
            // Held-out examples, and any other split, are reported apart: a
            // change that raises the tuning rates alone fits its examples
            // rather than the requirement.
            Evidence::Measured if self.cases.iter().any(|c| c.split.is_some()) => {
                let mut out = format!("{}  measured, no assertion\n", self.label);
                for (split, cases) in self.splits() {
                    out += &format!("  {}: {}\n", split_label(split), rates(&cases));
                }
                out
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

/// How a split is named in reports for people: `tuning`, `held out`, or its name.
fn split_label(split: Option<&str>) -> &str {
    match split {
        None => "tuning",
        Some(HOLDOUT) => "held out",
        Some(name) => name,
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

/// `6/8 (41–93%)`: a rate and its 95% interval, compact.
fn compact(k: usize, n: usize) -> String {
    let interval = interval(k, n);
    let bounds = interval
        .strip_prefix("95% interval ")
        .and_then(|rest| rest.strip_suffix("%; "))
        .map(|b| format!(" ({}%)", b.replace(" to ", "–")))
        .unwrap_or_default();
    format!("{k}/{n}{bounds}")
}

/// `caught 6/8 (41–93%) · false alarms 1/9 (1–43%) · 1 uncertain`.
fn compact_rates(cases: &[&Case]) -> String {
    let count = |label: Label, outcome: fn(&Outcome) -> bool| {
        cases
            .iter()
            .filter(|c| c.label == label && outcome(c.outcome()))
            .count()
    };
    let total = |label: Label| cases.iter().filter(|c| c.label == label).count();
    let mut parts = vec![
        format!(
            "caught {}",
            compact(
                count(Label::Violation, |o| *o == Outcome::Caught),
                total(Label::Violation)
            )
        ),
        format!(
            "false alarms {}",
            compact(
                count(Label::Ok, |o| *o == Outcome::FalseAlarm),
                total(Label::Ok)
            )
        ),
    ];
    let uncertain = count(Label::Violation, |o| *o == Outcome::Uncertain)
        + count(Label::Ok, |o| *o == Outcome::Uncertain);
    if uncertain > 0 {
        parts.push(format!("{uncertain} uncertain"));
    }
    parts.join(" · ")
}

/// `reqfile eval` for people: one row per requirement, the cases that do
/// not match their label under it, and a summary line. The exit code is
/// the plain rendering's.
pub fn render_pretty(tested: &[Tested], runs: usize, style: &Style) -> (String, i32) {
    let (_, code) = render(tested, runs);
    let width = tested
        .iter()
        .filter(|t| t.evidence != Evidence::None || style.verbose)
        .map(|t| t.label.len())
        .max()
        .unwrap_or(0);
    let examples: usize = tested.iter().map(|t| t.cases.len()).sum();
    let mut out = style.bold(&format!(
        "reqfile eval · {} · {}",
        plural(tested.len(), "requirement", "requirements"),
        plural(examples, "example", "examples")
    ));
    out += "\n\n";
    let without: Vec<&str> = tested
        .iter()
        .filter(|t| t.evidence == Evidence::None)
        .map(|t| t.label.as_str())
        .collect();
    for t in tested {
        if t.evidence == Evidence::None && !style.verbose {
            continue;
        }
        let all: Vec<&Case> = t.cases.iter().collect();
        // Cases that could not run usually fail for one reason, such as a
        // missing key: said once, the list stays readable.
        let causes: Vec<&str> = t
            .cases
            .iter()
            .flat_map(|c| &c.outcomes)
            .filter_map(|o| match o {
                Outcome::Error(e) => {
                    Some(e.rsplit_once(": ").map_or(e.as_str(), |(_, cause)| cause))
                }
                _ => None,
            })
            .collect();
        let one_cause = !causes.is_empty() && causes.iter().all(|c| *c == causes[0]);
        let (mark, text) = if t.has_errors() {
            let text = if one_cause {
                format!("not run · {}", causes[0])
            } else {
                "cases could not run".to_string()
            };
            (style.magenta("!"), style.magenta(&text))
        } else {
            match t.evidence {
                Evidence::None => (style.dim("–"), style.dim("no examples")),
                Evidence::Asserted => {
                    let labeled = t.cases.iter().filter(|c| c.accepted()).count();
                    let text = format!("asserted · {labeled}/{} as labeled", t.cases.len());
                    if t.failed() {
                        (style.red("✗"), style.red(&text))
                    } else {
                        (style.green("✓"), style.green(&text))
                    }
                }
                Evidence::Measured => (
                    style.yellow("≈"),
                    format!("{} · {}", style.yellow("measured"), compact_rates(&all)),
                ),
            }
        };
        out += &format!("  {mark} {:width$}  {text}\n", t.label);
        if t.evidence == Evidence::Measured && t.cases.iter().any(|c| c.split.is_some()) {
            let splits = t.splits();
            let width = splits
                .iter()
                .map(|(split, _)| split_label(*split).len())
                .max()
                .unwrap_or(0);
            for (split, cases) in splits {
                out += &format!(
                    "    {}\n",
                    style.dim(&format!(
                        "{:width$}  {}",
                        split_label(split),
                        compact_rates(&cases)
                    ))
                );
            }
        }
        let lines: Vec<String> = if one_cause && !style.verbose {
            Vec::new()
        } else {
            t.cases.iter().filter_map(Case::render).collect()
        };
        let shown = if style.verbose {
            lines.len()
        } else {
            lines.len().min(8)
        };
        for line in &lines[..shown] {
            let line = line.trim_end().trim_start();
            let painted = if t.evidence == Evidence::Asserted && !line.contains("(known)") {
                style.red(line)
            } else {
                style.dim(line)
            };
            out += &format!("      {painted}\n");
        }
        if shown < lines.len() {
            out += &format!(
                "      {}\n",
                style.dim(&format!("… {} more (reqfile eval -v)", lines.len() - shown))
            );
        }
    }
    if !without.is_empty() && !style.verbose {
        out += &format!(
            "  {} {}\n",
            style.dim("–"),
            style.dim(&format!("no examples: {}", without.join(", ")))
        );
    }
    let failed = tested.iter().filter(|t| t.failed()).count();
    let with = |evidence: Evidence| tested.iter().filter(|t| t.evidence == evidence).count();
    let verdict = if code == 3 {
        style.magenta("! some cases could not run")
    } else if failed > 0 {
        style.red(&format!(
            "✗ {} failing their labels",
            plural(failed, "requirement", "requirements")
        ))
    } else {
        style.green("✓ every asserted requirement matches its labels")
    };
    let mut context = vec![
        format!("{} asserted", with(Evidence::Asserted)),
        format!("{} measured", with(Evidence::Measured)),
        format!("{} without examples", with(Evidence::None)),
    ];
    if runs > 1 {
        let unstable: usize = tested.iter().map(Tested::unstable).sum();
        context.push(format!("{runs} runs each, {unstable} unstable"));
    }
    out += &format!(
        "\n{}\n{verdict}   {}\n",
        style.dim(&"─".repeat(64)),
        style.dim(&context.join(" · "))
    );
    (out, code)
}

/// `reqfile eval --format json`: for each requirement, its counts per split
/// and every case with its outcome, as one JSON document, and the exit code
/// of the plain rendering.
pub fn render_json(tested: &[Tested], runs: usize) -> (String, i32) {
    use serde_json::{Value, json};
    let (_, code) = render(tested, runs);
    let label = |label: Label| match label {
        Label::Violation => "violation",
        Label::Ok => "ok",
    };
    let evidence = |evidence: Evidence| match evidence {
        Evidence::Asserted => "asserted",
        Evidence::Measured => "measured",
        Evidence::None => "none",
    };
    let counts = |split: Option<&str>, cases: &[&Case]| -> Value {
        let count = |label: Label, keys: &[&str]| {
            cases
                .iter()
                .filter(|c| c.label == label && keys.contains(&c.outcome().key()))
                .count()
        };
        let total = |label: Label| {
            cases
                .iter()
                .filter(|c| c.label == label && !matches!(c.outcome(), Outcome::Error(_)))
                .count()
        };
        json!({
            "split": split.unwrap_or(DEFAULT_SPLIT),
            "violations": {
                "total": total(Label::Violation),
                "caught": count(Label::Violation, &["caught"]),
                "missed": count(Label::Violation, &["missed", "flagged_elsewhere"]),
                "not_selected": count(Label::Violation, &["not_selected"]),
                "uncertain": count(Label::Violation, &["uncertain"]),
            },
            "ok": {
                "total": total(Label::Ok),
                "passed": count(Label::Ok, &["passed"]),
                "flagged": count(Label::Ok, &["false_alarm"]),
                "uncertain": count(Label::Ok, &["uncertain"]),
            },
            "errors": cases.iter().filter(|c| matches!(c.outcome(), Outcome::Error(_))).count(),
            "unstable": cases.iter().filter(|c| !c.stable()).count(),
        })
    };
    let case = |case: &Case| -> Value {
        let outcome = case.outcome();
        let run = case
            .outcomes
            .iter()
            .position(|o| o == outcome)
            .expect("the counted outcome is one of the runs");
        let mut value = json!({
            "name": case.name,
            "split": case.split.as_deref().unwrap_or(DEFAULT_SPLIT),
            "expected": label(case.label),
            "outcome": outcome.key(),
            "accepted": case.accepted(),
            "flagged": case.flagged[run],
        });
        if let Some(known) = case.known {
            value["known"] = json!(match known {
                Known::Miss => "miss",
                Known::FalseAlarm => "false_alarm",
            });
        }
        if let Outcome::Error(message) = outcome {
            value["error"] = json!(message);
        }
        if runs > 1 {
            value["stable"] = json!(case.stable());
            value["outcomes"] = json!(case.outcomes.iter().map(Outcome::key).collect::<Vec<_>>());
        }
        value
    };
    let requirements: Vec<Value> = tested
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "reqfile": t.reqfile,
                "evidence": evidence(t.evidence),
                "failed": t.failed(),
                "splits": t
                    .splits()
                    .iter()
                    .filter(|(split, cases)| split.is_some() || !cases.is_empty())
                    .map(|(split, cases)| counts(*split, cases))
                    .collect::<Vec<_>>(),
                "cases": t.cases.iter().map(case).collect::<Vec<_>>(),
            })
        })
        .collect();
    let with = |e: Evidence| tested.iter().filter(|t| t.evidence == e).count();
    let document = json!({
        "requirements": requirements,
        "summary": {
            "requirements": tested.len(),
            "asserted": with(Evidence::Asserted),
            "measured": with(Evidence::Measured),
            "without_evidence": with(Evidence::None),
            "examples": tested.iter().map(|t| t.cases.len()).sum::<usize>(),
            "runs": runs,
            "unstable": tested.iter().map(Tested::unstable).sum::<usize>(),
            "failing": tested.iter().filter(|t| t.failed()).count(),
        },
        "exit_code": code,
    });
    (
        serde_json::to_string_pretty(&document).expect("the eval serializes to JSON") + "\n",
        code,
    )
}
