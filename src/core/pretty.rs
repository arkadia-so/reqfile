//! The report as people read it in a terminal: a table of requirements, the
//! details of what does not pass, and one summary line, in color. Agents,
//! hooks and CI read the plain rendering instead, which stays stable.

use super::report::{CheckRun, Finding, FindingKind, Report, RequirementRun};

/// How the report is drawn.
#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub color: bool,
    /// Every finding, and what each check ran.
    pub verbose: bool,
    /// Only what fails the run, and the summary line.
    pub quiet: bool,
}

/// Findings shown per requirement without `--verbose`.
const SHOWN: usize = 5;
/// The widest location column before messages wrap to their own alignment.
const MAX_LOCATION: usize = 48;

impl Style {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.color && !text.is_empty() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
    pub fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }
    pub fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }
    pub fn yellow(&self, text: &str) -> String {
        self.paint("33", text)
    }
    pub fn magenta(&self, text: &str) -> String {
        self.paint("35", text)
    }
    pub fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }
}

/// Where a requirement stands after the run, most severe first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Violations,
    Error,
    Advisory,
    Uncertain,
    Pass,
    Nothing,
}

struct Row<'r> {
    run: &'r RequirementRun,
    findings: Vec<&'r Finding>,
    /// Error messages, and whether each blocks the run.
    errors: Vec<(&'r str, bool)>,
    status: Status,
}

impl<'r> Row<'r> {
    fn new(report: &'r Report, run: &'r RequirementRun) -> Self {
        let findings: Vec<&Finding> = report
            .findings
            .iter()
            .filter(|f| f.requirement == run.id)
            .collect();
        let errors: Vec<(&str, bool)> = report
            .errors
            .iter()
            .filter(|e| e.requirement.as_deref() == Some(run.id.as_str()))
            .map(|e| (e.message.as_str(), e.blocking))
            .collect();
        let count = |kind: FindingKind| findings.iter().filter(|f| f.kind == kind).count();
        let status = if count(FindingKind::Violation) > 0 {
            Status::Violations
        } else if !errors.is_empty() {
            Status::Error
        } else if count(FindingKind::Advisory) > 0 {
            Status::Advisory
        } else if count(FindingKind::Uncertain) > 0 {
            Status::Uncertain
        } else if run.checks.is_empty() {
            // Nothing ran: no file to look at, or every check left for a full run.
            Status::Nothing
        } else {
            Status::Pass
        };
        Self {
            run,
            findings,
            errors,
            status,
        }
    }

    fn count(&self, kind: FindingKind) -> usize {
        self.findings.iter().filter(|f| f.kind == kind).count()
    }

    fn symbol(&self, style: &Style) -> String {
        match self.status {
            Status::Violations => style.red("✗"),
            Status::Error => style.magenta("!"),
            Status::Advisory => style.yellow("●"),
            Status::Uncertain => style.yellow("?"),
            Status::Pass => style.green("✓"),
            Status::Nothing => style.dim("–"),
        }
    }

    /// `2 violations · 1 advisory`, `pass · 316 units judged`, …
    fn describe(&self, style: &Style) -> String {
        let mut parts: Vec<String> = Vec::new();
        let n = |count: usize, one: &str, many: &str| plural(count, one, many);
        let violations = self.count(FindingKind::Violation);
        let advisory = self.count(FindingKind::Advisory);
        let uncertain = self.count(FindingKind::Uncertain);
        if violations > 0 {
            parts.push(style.red(&n(violations, "violation", "violations")));
        }
        if let Some((first, blocking)) = self.errors.first() {
            // Errors read `context: cause`; the cause says what to do.
            let line = first.lines().next().unwrap_or_default();
            let cause = line.rsplit_once(": ").map_or(line, |(_, cause)| cause);
            // `X is not set; …` says all it needs in its first clause.
            let cause = cause.split_once("; ").map_or(cause, |(first, _)| first);
            let line = cut(cause, 60);
            // An advisory check that could not run blocks nothing: it did not run.
            if *blocking {
                parts.push(style.magenta(&format!("error · {line}")));
            } else {
                parts.push(style.magenta(&format!("not run · {line}")));
            }
        }
        if advisory > 0 {
            parts.push(style.yellow(&format!("{advisory} advisory")));
        }
        if uncertain > 0 {
            parts.push(style.yellow(&format!("{uncertain} uncertain")));
        }
        match self.status {
            Status::Pass => parts.push(style.green("pass")),
            Status::Nothing if self.run.deferred > 0 => {}
            Status::Nothing => parts.push(style.dim("nothing to check")),
            _ => {}
        }
        let units: usize = self
            .run
            .checks
            .iter()
            .map(|c| match c {
                CheckRun::Decision { units, .. } => *units,
                CheckRun::Command { .. } => 0,
            })
            .sum();
        if units > 0 {
            parts.push(style.dim(&n(units, "unit judged", "units judged")));
        }
        if self.run.deferred > 0 {
            parts.push(style.dim(&format!("{} left for a full run", self.run.deferred)));
        }
        parts.join(&style.dim(" · "))
    }

    fn millis(&self) -> u64 {
        self.run
            .checks
            .iter()
            .map(|c| match c {
                CheckRun::Command { millis, .. } => *millis,
                CheckRun::Decision { .. } => 0,
            })
            .sum()
    }
}

/// `reqfile check` for people.
pub fn render(report: &Report, style: &Style) -> String {
    let rows: Vec<Row> = report
        .requirements
        .iter()
        .map(|run| Row::new(report, run))
        .collect();
    let mut out = String::new();
    // Errors of no requirement in particular: configuration, Jev, the cache.
    let general: Vec<&str> = report
        .errors
        .iter()
        .filter(|e| e.requirement.is_none())
        .map(|e| e.message.as_str())
        .collect();
    if !style.quiet {
        out += &style.bold(&format!(
            "reqfile check · {} · {}",
            plural(rows.len(), "requirement", "requirements"),
            plural(report.checks_run, "check", "checks")
        ));
        out += "\n";
        for source in &report.sources {
            let offline = if source.offline {
                " (offline: last resolution cached)"
            } else {
                ""
            };
            out += &style.dim(&format!(
                "source {} → {}{offline}",
                source.location,
                &source.commit[..source.commit.len().min(12)]
            ));
            out += "\n";
        }
        out += "\n";
        out += &setup_banner(report, style);
        let width = rows.iter().map(|r| r.run.id.len()).max().unwrap_or(0);
        let described: Vec<String> = rows.iter().map(|r| r.describe(style)).collect();
        // Times align on the rows that ran; an error's cause is not widened for them.
        let column = rows
            .iter()
            .zip(&described)
            .filter(|(row, _)| row.millis() > 0 && row.errors.is_empty())
            .map(|(_, d)| visible(d))
            .max()
            .unwrap_or(0);
        // Requirements grouped by Reqfile, the module they belong to, in the
        // order Reqfiles come; within one, product, then code, then process.
        let mut files: Vec<&str> = Vec::new();
        for row in &rows {
            if !files.contains(&row.run.reqfile.as_str()) {
                files.push(&row.run.reqfile);
            }
        }
        let rank = |kind: &str| {
            ["product", "code", "process"]
                .iter()
                .position(|k| *k == kind)
        };
        // The type says something only where types mix; then every row shows
        // it, so columns stay aligned across groups.
        let mixed = files.iter().any(|file| {
            let mut kinds = rows
                .iter()
                .filter(|r| r.run.reqfile == *file)
                .map(|r| r.run.kind);
            let first = kinds.next();
            kinds.any(|k| Some(k) != first)
        });
        for (n, file) in files.iter().enumerate() {
            let mut group: Vec<(&Row, &String)> = rows
                .iter()
                .zip(&described)
                .filter(|(row, _)| row.run.reqfile == *file)
                .collect();
            group.sort_by_key(|(row, _)| rank(row.run.kind));
            if files.len() > 1 {
                if n > 0 {
                    out += "\n";
                }
                // A Reqfile stands for its folder: the root, or its path.
                let folder = file.rsplit_once('/').map_or("root", |(dir, _)| dir);
                out += &format!("  {}\n", style.bold(folder));
            }
            for (row, text) in group {
                let millis = row.millis();
                let time = if millis > 0 {
                    let pad = " ".repeat(column.saturating_sub(visible(text)));
                    style.dim(&format!("{pad}  {:>5}", seconds(millis)))
                } else {
                    String::new()
                };
                let kind = if mixed {
                    style.dim(&format!("{:7} ", row.run.kind))
                } else {
                    String::new()
                };
                out += &format!(
                    "  {} {kind}{:width$}  {text}{time}\n",
                    row.symbol(style),
                    row.run.id
                );
            }
        }
    }
    if style.quiet {
        out += &setup_banner(report, style);
    }
    // What the banner already says is not repeated for each requirement.
    let unset: Vec<&str> = report.setup.iter().map(|s| s.variable.as_str()).collect();
    let only_setup = |row: &Row| {
        row.findings.is_empty()
            && !row.errors.is_empty()
            && row.errors.iter().all(|(message, _)| {
                unset
                    .iter()
                    .any(|v| message.contains(&format!("{v} is not set")))
            })
    };
    let mut details: Vec<&Row> = rows
        .iter()
        .filter(|r| !r.findings.is_empty() || !r.errors.is_empty())
        .filter(|r| !only_setup(r))
        .filter(|r| !style.quiet || matches!(r.status, Status::Violations | Status::Error))
        .collect();
    details.sort_by_key(|r| r.status);
    // The details stand apart from the table that sums them up.
    if !details.is_empty() && !style.quiet {
        out += &format!("\n{}\n", style.dim(&"─".repeat(64)));
    }
    for row in details {
        out += "\n";
        out += &section(row, style);
    }
    if !general.is_empty() {
        out += "\n";
        for message in general {
            out += &error_block(&style.magenta("!"), message, style);
        }
    }
    let decisions = report.decisions();
    if !decisions.is_empty() {
        out += &format!("\n{}\n", style.magenta("┌ decide"));
        for d in &decisions {
            out += &format!(
                "{} {} vs {}  {}\n{}   {}\n{}   {}\n",
                style.magenta("│"),
                style.bold(&d.requirement),
                style.bold(&d.breaks),
                style.dim(&format!(
                    "p={:.2} · fixing {} in {} would break {}",
                    d.probability,
                    d.requirement,
                    plural(d.files.len(), "file", "files"),
                    d.breaks
                )),
                style.magenta("│"),
                d.files.join(", "),
                style.magenta("│"),
                style.dim(
                    "Reword one of the two requirements, or say which one wins where they meet."
                ),
            );
        }
        out += &format!("{}\n", style.magenta("└"));
    }
    if style.verbose {
        out += &checks_ran(&rows, style);
    }
    if let Some(reason) = report
        .conflicts_not_checked
        .as_ref()
        .filter(|reason| !unset.iter().any(|v| reason.contains(v)))
    {
        out += &format!(
            "\n{}\n",
            style.dim(&format!(
                "Conflicts between requirements not checked: {reason}."
            ))
        );
    }
    out += &footer(report, style);
    out
}

/// Credentials the selected checks need and the environment lacks, first,
/// in red when a blocking check needs them.
fn setup_banner(report: &Report, style: &Style) -> String {
    let mut out = String::new();
    for setup in &report.setup {
        let checks_line = format!(
            "The decision checks of {} will not judge any code, nor will conflicts between requirements be looked for.",
            setup.requirements.join(", ")
        );
        let paint = |text: &str| {
            if setup.blocking {
                style.red(text)
            } else {
                style.yellow(text)
            }
        };
        out += &format!(
            "{} {}\n  {}\n  {}\n\n",
            paint("!"),
            paint(&format!("Jev is not set up: {} is not set", setup.variable)),
            checks_line,
            style.dim(&format!(
                "Set {}, or name another variable in decision.api_key_env of .reqfile/config.yaml.",
                setup.variable
            )),
        );
    }
    out
}

/// A requirement's findings and errors, grouped, each fix hint once.
fn section(row: &Row, style: &Style) -> String {
    let mut out = format!(
        "{} {}  {}",
        row.symbol(style),
        style.bold(&row.run.id),
        row.describe(style)
    );
    if let Some(source) = &row.run.source {
        out += &style.dim(&format!(" · from {source}"));
    }
    out += "\n";
    if !row.run.must.is_empty() {
        out += &format!("  {}\n", style.dim(first_sentence(&row.run.must)));
    }
    if !row.findings.is_empty() {
        out += "\n";
    }
    // A checker judging whole files reports line 1: the line adds nothing.
    let file_level = row.findings.iter().all(|f| f.line.is_none_or(|l| l == 1));
    let location = |f: &Finding| match (&f.file, f.line) {
        (Some(file), Some(line)) if !file_level => format!("{file}:{line}"),
        (Some(file), _) => file.clone(),
        (None, _) => String::new(),
    };
    let width = row
        .findings
        .iter()
        .map(|f| location(f).chars().count())
        .max()
        .unwrap_or(0)
        .min(MAX_LOCATION);
    let shown = if style.verbose {
        row.findings.len()
    } else {
        SHOWN.min(row.findings.len())
    };
    for finding in &row.findings[..shown] {
        let at = location(finding);
        let message = short_message(finding);
        let (first, rest) = message.split_once('\n').unwrap_or((&message, ""));
        let mark = match finding.kind {
            FindingKind::Violation => style.red("✗"),
            FindingKind::Advisory => style.yellow("●"),
            FindingKind::Uncertain => style.yellow("?"),
        };
        let probability = match (finding.probability, finding.kind) {
            (Some(p), _) if finding.model.is_some() || finding.kind == FindingKind::Uncertain => {
                style.dim(&format!("  p={p:.2}"))
            }
            _ => String::new(),
        };
        if at.is_empty() {
            out += &format!("  {mark} {first}{probability}\n");
        } else if at.chars().count() > width {
            out += &format!("  {mark} {at}\n  {:width$}   {first}{probability}\n", "");
        } else {
            out += &format!("  {mark} {at:width$}  {first}{probability}\n");
        }
        if style.verbose {
            for line in rest.lines() {
                out += &format!("  {:width$}     {}\n", "", style.dim(line));
            }
        }
        for conflict in &finding.conflicts {
            out += &format!(
                "  {:width$}     {}\n",
                "",
                style.magenta(&format!(
                    "↳ breaks {} (p={:.2})",
                    conflict.requirement, conflict.probability
                ))
            );
        }
    }
    if shown < row.findings.len() {
        out += &format!(
            "  {}\n",
            style.dim(&format!(
                "… {} more (reqfile check -v)",
                row.findings.len() - shown
            ))
        );
    }
    let mut hints: Vec<&str> = Vec::new();
    for finding in &row.findings {
        if !hints.contains(&finding.fix_hint.as_str()) {
            hints.push(&finding.fix_hint);
        }
    }
    if !hints.is_empty() {
        out += "\n";
    }
    for hint in hints {
        out += &format!("  {}  {hint}\n", style.dim("fix"));
    }
    for (message, _) in &row.errors {
        out += "\n";
        out += &error_block(&style.magenta("!"), message, style);
    }
    out
}

fn error_block(mark: &str, message: &str, style: &Style) -> String {
    let mut lines = message.trim_end().lines();
    let mut out = format!("  {mark} {}\n", lines.next().unwrap_or_default());
    let rest: Vec<&str> = lines.collect();
    let shown = if style.verbose {
        rest.len()
    } else {
        rest.len().min(3)
    };
    for line in &rest[..shown] {
        out += &format!("    {}\n", style.dim(line));
    }
    if shown < rest.len() {
        out += &format!(
            "    {}\n",
            style.dim(&format!("… {} more lines (-v)", rest.len() - shown))
        );
    }
    out
}

/// The message without what the location already says: a leading file path
/// and a trailing `(ID)` the SARIF rule adds.
fn short_message(finding: &Finding) -> String {
    let mut message = finding.message.as_str();
    if let Some(file) = &finding.file
        && let Some(rest) = message.strip_prefix(file.as_str())
    {
        message = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    }
    let suffix = format!(" ({})", finding.requirement);
    message.strip_suffix(&suffix).unwrap_or(message).to_string()
}

/// `-v`: what each check ran.
fn checks_ran(rows: &[Row], style: &Style) -> String {
    let mut out = format!("\n{}\n", style.bold("Checks"));
    let width = rows.iter().map(|r| r.run.id.len()).max().unwrap_or(0);
    for row in rows {
        if row.run.checks.is_empty() {
            let why = if row.run.deferred > 0 {
                "left for a full run"
            } else {
                "nothing to check"
            };
            out += &format!("  {:width$}  {}\n", row.run.id, style.dim(why));
        }
        for check in &row.run.checks {
            let line = match check {
                CheckRun::Command { run, files, millis } => format!(
                    "command  {run}{}{}",
                    style.dim(&if *files > 0 {
                        format!(" · {}", plural(*files, "file", "files"))
                    } else {
                        String::new()
                    }),
                    style.dim(&format!(" · {}", seconds(*millis)))
                ),
                CheckRun::Decision {
                    units,
                    cached,
                    model,
                } => format!(
                    "decision{}",
                    style.dim(&format!(
                        " · {} ({cached} cached){}",
                        plural(*units, "unit", "units"),
                        model
                            .as_deref()
                            .map(|m| format!(" · {m}"))
                            .unwrap_or_default()
                    ))
                ),
            };
            out += &format!("  {:width$}  {line}\n", row.run.id);
        }
    }
    out
}

fn footer(report: &Report, style: &Style) -> String {
    let s = report.summary();
    let count = |kind: FindingKind| report.findings.iter().filter(|f| f.kind == kind).count();
    let mut parts: Vec<String> = Vec::new();
    if s.violations > 0 {
        parts.push(style.red(&format!(
            "✗ {}",
            plural(s.violations, "violation", "violations")
        )));
    }
    if count(FindingKind::Advisory) > 0 {
        parts.push(style.yellow(&format!("● {} advisory", count(FindingKind::Advisory))));
    }
    if count(FindingKind::Uncertain) > 0 {
        parts.push(style.yellow(&format!("? {} uncertain", count(FindingKind::Uncertain))));
    }
    if s.errors > 0 {
        let advisory = if s.advisory_errors > 0 {
            format!(" ({} advisory)", s.advisory_errors)
        } else {
            String::new()
        };
        parts.push(style.magenta(&format!(
            "! {}{advisory}",
            plural(s.errors, "error", "errors")
        )));
    }
    if s.decisions > 0 {
        parts.push(style.magenta(&format!(
            "◆ {} to make",
            plural(s.decisions, "decision", "decisions")
        )));
    }
    if parts.is_empty() {
        parts.push(style.green("✓ every check passes"));
    }
    let mut context = vec![plural(s.checks_run, "check", "checks")];
    if let Some(units) = s.units_judged {
        context.push(plural(units, "unit judged", "units judged"));
    }
    if s.checks_left_for_full_run > 0 {
        context.push(format!(
            "{} left for a full run",
            s.checks_left_for_full_run
        ));
    }
    context.push(seconds(report.millis));
    format!(
        "\n{}\n{}   {}\n",
        style.dim(&"─".repeat(64)),
        parts.join("   "),
        style.dim(&context.join(" · "))
    )
}

/// The width of text on screen, without its color codes.
fn visible(text: &str) -> usize {
    let mut width = 0;
    let mut escape = false;
    for c in text.chars() {
        match (escape, c) {
            (false, '\x1b') => escape = true,
            (true, 'm') => escape = false,
            (true, _) => {}
            (false, _) => width += 1,
        }
    }
    width
}

/// At most `max` characters of `text`, with an ellipsis when cut.
fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        None => text.to_string(),
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
    }
}

/// The first sentence of a requirement's `must`, enough to recall it.
fn first_sentence(text: &str) -> &str {
    match text.find(". ") {
        Some(end) => &text[..=end],
        None => text,
    }
}

fn seconds(millis: u64) -> String {
    format!("{:.1}s", millis as f64 / 1000.0)
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(file: &str, message: &str) -> Finding {
        Finding {
            requirement: "COLOCATION".into(),
            kind: FindingKind::Advisory,
            file: Some(file.into()),
            line: Some(1),
            message: message.into(),
            fix_hint: "Move it.".into(),
            probability: Some(0.9),
            model: None,
            block: None,
            source: None,
            conflicts: Vec::new(),
        }
    }

    #[test]
    fn messages_drop_what_the_location_already_says() {
        let f = finding("src/a.rs", "src/a.rs is used only from shell/ (COLOCATION)");
        assert_eq!(short_message(&f), "is used only from shell/");
    }

    #[test]
    fn findings_of_a_requirement_are_grouped_with_their_fix_once() {
        let style = Style {
            color: false,
            verbose: false,
            quiet: false,
        };
        let run = RequirementRun {
            id: "COLOCATION".into(),
            must: "Code lives with its users.".into(),
            source: Some("gabsn/reqfile-colocation@8ec6007".into()),
            checks: vec![CheckRun::Command {
                run: "x".into(),
                files: 0,
                millis: 400,
            }],
            ..RequirementRun::default()
        };
        let report = Report {
            findings: (0..7)
                .map(|i| finding(&format!("src/f{i}.rs"), "is used only from shell/"))
                .collect(),
            requirements: vec![run],
            checks_run: 1,
            millis: 400,
            ..Report::default()
        };
        let text = render(&report, &style);
        assert!(
            text.contains("  ● COLOCATION  7 advisory   0.4s\n"),
            "{text}"
        );
        assert!(text.contains("● COLOCATION  7 advisory · from gabsn/reqfile-colocation@8ec6007\n  Code lives with its users.\n"), "{text}");
        assert!(
            text.contains("  ● src/f0.rs  is used only from shell/\n"),
            "{text}"
        );
        assert!(text.contains("  … 2 more (reqfile check -v)\n"), "{text}");
        assert_eq!(text.matches("fix  Move it.").count(), 1, "{text}");
        assert!(text.ends_with("● 7 advisory   1 check · 0.4s\n"), "{text}");
    }
}
