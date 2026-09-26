//! `reqfile check`: plans every check, runs commands and Jev calls, and reports.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::ask;
use super::pool;
use super::process;
use super::workspace::Workspace;
use crate::core::command::{self, CommandViolation};
use crate::core::config::{self, DecisionConfig};
use crate::core::decision::{self, DecisionSpec, Unit, Verdict};
use crate::core::paths;
use crate::core::plan::{self, Changed, CommandPlan, Selection};
use crate::core::report::{Error, Finding, FindingKind, Report};
use crate::core::reqfile::{Check, CommandCheck, OutputFormat, Reqfile, Requirement};
use crate::core::runlog;

pub struct Options {
    /// `--changed`, with its optional base.
    pub changed: Option<Option<String>>,
    pub only: Option<Vec<String>>,
    /// `--fast`: only decision checks and commands declared `fast`.
    pub fast: bool,
    /// `--log`: the JSON lines file to append this run's decisions to.
    pub log: Option<PathBuf>,
    /// `--log-tag KEY=VALUE`, repeated.
    pub log_tags: Vec<String>,
}

/// The lines of the run log, while the run goes.
struct Log {
    context: runlog::Context,
    lines: Vec<serde_json::Value>,
}

/// Adds a line to the log, if the run keeps one.
fn log_line(log: &mut Option<Log>, kind: &str, fields: serde_json::Value) {
    if let Some(log) = log {
        let line = log.context.line(kind, fields);
        log.lines.push(line);
    }
}

struct CommandJob<'w> {
    requirement: &'w Requirement,
    dir: &'w str,
    check: &'w CommandCheck,
    args: Vec<String>,
}

struct DecisionJob<'w> {
    requirement: &'w Requirement,
    /// How to call Jev, from the settings of that folder.
    jev: DecisionConfig,
    spec: DecisionSpec,
    blocking: bool,
    units: Vec<Unit>,
}

pub fn run(cwd: &Path, options: &Options) -> Report {
    let workspace = match Workspace::load(cwd) {
        Ok(workspace) => workspace,
        Err(errors) => return Report::from_errors(errors),
    };
    let selected = match select(&workspace.reqfiles, options.only.as_deref()) {
        Ok(selected) => selected,
        Err(errors) => return Report::from_errors(errors),
    };
    let specs = match load_specs(&workspace, &selected) {
        Ok(specs) => specs,
        Err(errors) => return Report::from_errors(errors),
    };
    let selection = match selection(&workspace, &selected, options) {
        Ok(selection) => selection,
        Err(error) => return Report::from_errors([error]),
    };
    let mut log = match open_log(&workspace, options) {
        Ok(log) => log,
        Err(error) => return Report::from_errors([error]),
    };
    let mut report = Report::default();
    let plan = plan(
        &workspace,
        &selected,
        specs,
        &selection,
        options.fast,
        &mut report,
    );
    report.checks_run += plan.commands.len() + plan.decisions.len();
    run_commands(&workspace, &plan.commands, &mut report, &mut log);
    if selected
        .iter()
        .any(|(_, r)| r.checks.iter().any(|c| matches!(c, Check::Decision(_))))
    {
        // Only a run that saw every unit knows which cached answers are obsolete.
        let prune = options.changed.is_none() && options.only.is_none();
        run_decisions(&workspace, &plan.decisions, prune, &mut report, &mut log);
    }
    if let (Some(path), Some(log)) = (&options.log, log)
        && let Err(message) = append_log(path, &log.lines)
    {
        report.errors.push(Error {
            requirement: None,
            message,
        });
    }
    // Blocking violations first, then advisory ones, then uncertain ones, each in Reqfile order.
    report.findings.sort_by_key(|f| f.kind);
    report
}

/// The run log, if `--log` asked for one.
fn open_log(workspace: &Workspace, options: &Options) -> Result<Option<Log>, String> {
    if options.log.is_none() {
        return Ok(None);
    }
    let time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("the system clock is before 1970: {e}"))?
        .as_millis();
    let context = runlog::Context {
        run_id: format!("{time_ms:x}-{:x}", std::process::id()),
        time_ms,
        git_head: super::git::head(&workspace.root),
        fast: options.fast,
        changed: options.changed.clone(),
        tags: runlog::parse_tags(&options.log_tags)?,
    };
    Ok(Some(Log {
        context,
        lines: Vec::new(),
    }))
}

/// Appends the lines to the log file, creating it and its folders if needed.
fn append_log(path: &Path, lines: &[serde_json::Value]) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot write the run log {}: {e}", path.display());
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).map_err(fail)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(fail)?;
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    file.write_all(text.as_bytes()).map_err(fail)
}

/// The checks to run, in Reqfile order.
struct Plan<'w> {
    commands: Vec<CommandJob<'w>>,
    decisions: Vec<DecisionJob<'w>>,
}

/// Plans every selected check against the selection; checks with nothing to
/// check and units that cannot be read are recorded in `report`.
fn plan<'w>(
    workspace: &'w Workspace,
    selected: &[(&'w Reqfile, &'w Requirement)],
    specs: Vec<DecisionSpec>,
    selection: &Selection,
    fast: bool,
    report: &mut Report,
) -> Plan<'w> {
    let mut plan = Plan {
        commands: Vec::new(),
        decisions: Vec::new(),
    };
    let mut specs = specs.into_iter();
    for &(reqfile, requirement) in selected {
        let full = selection.redefined(reqfile, requirement);
        for check in &requirement.checks {
            match check {
                Check::Command(check) if fast && !check.fast => {
                    report.checks_left_for_full_run += 1
                }
                Check::Command(check) => {
                    match plan::plan_command(&reqfile.dir, check, selection, full) {
                        CommandPlan::Run(args) => plan.commands.push(CommandJob {
                            requirement,
                            dir: &reqfile.dir,
                            check,
                            args,
                        }),
                        CommandPlan::NoMatchingFiles => report.checks_with_nothing_to_check += 1,
                    }
                }
                Check::Decision(check) => {
                    let spec = specs
                        .next()
                        .expect("a spec was loaded for every decision check");
                    let files = plan::decision_files(&reqfile.dir, &spec, selection, full);
                    match extract_units(&workspace.root, &spec, &files) {
                        // No file, or no code in them is a unit: nothing would be judged.
                        Ok(units) if units.is_empty() => report.checks_with_nothing_to_check += 1,
                        Ok(units) => plan.decisions.push(DecisionJob {
                            requirement,
                            jev: workspace.settings.decision(&reqfile.dir),
                            spec,
                            blocking: check.blocking,
                            units,
                        }),
                        Err(message) => {
                            report.checks_run += 1;
                            report.errors.push(Error {
                                requirement: Some(requirement.id.clone()),
                                message,
                            });
                        }
                    }
                }
            }
        }
    }
    plan
}

/// Runs the command checks in parallel and records what they found.
fn run_commands(
    workspace: &Workspace,
    commands: &[CommandJob],
    report: &mut Report,
    log: &mut Option<Log>,
) {
    let root = workspace.root.to_string_lossy();
    let parallelism = thread::available_parallelism().map_or(1, |n| n.get());
    let outcomes = pool::map(commands, parallelism, |job| {
        let outcome = process::run(
            &workspace.root.join(job.dir),
            &job.check.run,
            &job.args,
            job.check.format == OutputFormat::Exit,
            Duration::from_secs(job.check.timeout_secs),
        );
        command::judge(job.check, outcome, job.dir, &root)
    });
    for (job, outcome) in commands.iter().zip(outcomes) {
        let id = &job.requirement.id;
        match outcome {
            Ok(violations) => {
                let status = if violations.is_empty() {
                    "pass"
                } else {
                    "violations"
                };
                log_line(
                    log,
                    "command_check",
                    json!({ "requirement": id, "command": job.check.run, "files": job.args.len(), "status": status, "findings": violations.len() }),
                );
                for v in &violations {
                    log_line(
                        log,
                        "command_finding",
                        json!({ "requirement": id, "file": v.file, "line": v.line, "message": v.message }),
                    );
                }
                report
                    .findings
                    .extend(violations.into_iter().map(|v| command_finding(job, v)));
            }
            Err(message) => {
                log_line(
                    log,
                    "command_check",
                    json!({ "requirement": id, "command": job.check.run, "files": job.args.len(), "status": "error", "error": message }),
                );
                report.errors.push(Error {
                    requirement: Some(id.clone()),
                    message,
                });
            }
        }
    }
}

/// Asks Jev about the units of the decision checks and records its verdicts.
fn run_decisions(
    workspace: &Workspace,
    decisions: &[DecisionJob],
    prune: bool,
    report: &mut Report,
    log: &mut Option<Log>,
) {
    let checks: Vec<ask::Check> = decisions
        .iter()
        .map(|job| ask::Check {
            id: &job.requirement.id,
            spec: &job.spec,
            jev: &job.jev,
            units: &job.units,
        })
        .collect();
    let (answers, errors) = ask::answers(&workspace.root, &checks, prune);
    report
        .errors
        .extend(errors.into_iter().map(|message| Error {
            requirement: None,
            message,
        }));
    report.units_judged = Some(answers.iter().flatten().filter(|a| a.is_ok()).count());
    for (job, answers) in decisions.iter().zip(answers) {
        judge_decisions(report, job, answers, log);
    }
}

/// The requirements to check, in Reqfile order, restricted by `--only`.
fn select<'w>(
    reqfiles: &'w [Reqfile],
    only: Option<&[String]>,
) -> Result<Vec<(&'w Reqfile, &'w Requirement)>, Vec<String>> {
    let all = reqfiles
        .iter()
        .flat_map(|r| r.requirements.iter().map(move |req| (r, req)));
    let Some(only) = only else {
        return Ok(all.collect());
    };
    let unknown: Vec<String> = only
        .iter()
        .filter(|id| {
            !reqfiles
                .iter()
                .any(|r| r.requirements.iter().any(|req| &req.id == *id))
        })
        .map(|id| format!("--only: no requirement has the id {id}"))
        .collect();
    if unknown.is_empty() {
        Ok(all.filter(|(_, req)| only.contains(&req.id)).collect())
    } else {
        Err(unknown)
    }
}

/// The decision.yaml of every selected decision check, in order.
fn load_specs(
    workspace: &Workspace,
    selected: &[(&Reqfile, &Requirement)],
) -> Result<Vec<DecisionSpec>, Vec<String>> {
    let mut specs = Vec::new();
    let mut errors = Vec::new();
    for (reqfile, requirement) in selected {
        for check in &requirement.checks {
            let Check::Decision(check) = check else {
                continue;
            };
            let path = decision::spec_path(&reqfile.dir, &requirement.id);
            match fs::read_to_string(workspace.root.join(&path)) {
                Ok(text) => match decision::parse_spec(&path, &text) {
                    Ok(spec) => specs.push(spec),
                    Err(e) => errors.push(e.to_string()),
                },
                Err(e) => errors.push(format!(
                    "{}:{}: the decision check of {} needs {path}: {e}",
                    reqfile.path, check.line, requirement.id
                )),
            }
        }
    }
    if errors.is_empty() {
        Ok(specs)
    } else {
        Err(errors)
    }
}

/// The files checks may target; with `--changed`, a base is resolved only
/// for the folders of the selected requirements.
fn selection(
    workspace: &Workspace,
    selected: &[(&Reqfile, &Requirement)],
    options: &Options,
) -> Result<Selection, String> {
    let Some(cli_base) = &options.changed else {
        return Ok(Selection {
            targets: workspace.targets.clone(),
            changed: None,
        });
    };
    let default_base = super::git::default_base(&workspace.root)?;
    let mut by_base: BTreeMap<String, Changed> = BTreeMap::new();
    let mut by_dir = BTreeMap::new();
    // Only folders with a check that will run need a base.
    let runs = |check: &Check| match check {
        Check::Command(command) => !options.fast || command.fast,
        Check::Decision(_) => true,
    };
    let dirs: BTreeSet<&str> = selected
        .iter()
        .filter(|(_, requirement)| requirement.checks.iter().any(runs))
        .map(|(r, _)| r.dir.as_str())
        .collect();
    for dir in dirs {
        let base = cli_base
            .clone()
            .or_else(|| workspace.settings.base(dir).map(String::from))
            .or_else(|| default_base.clone())
            .ok_or_else(|| {
                format!(
                    "--changed needs a base for {}: pass one (reqfile check --changed origin/main), set `base` in {}, or set origin/HEAD (git remote set-head origin --auto)",
                    paths::display_dir(dir),
                    config::path(dir)
                )
            })?;
        if !by_base.contains_key(&base) {
            let all: BTreeSet<String> = super::git::changed(&workspace.root, &base)?
                .into_iter()
                .collect();
            let targets = all
                .iter()
                .filter(|p| !workspace.settings.is_excluded(p))
                .cloned()
                .collect();
            by_base.insert(base.clone(), Changed { targets, all });
        }
        by_dir.insert(dir.to_string(), by_base[&base].clone());
    }
    Ok(Selection {
        targets: workspace.targets.clone(),
        changed: Some(by_dir),
    })
}

fn extract_units(
    root: &Path,
    spec: &DecisionSpec,
    files: &[(&str, &str)],
) -> Result<Vec<Unit>, String> {
    let mut units = Vec::new();
    for (path, relative) in files {
        let source =
            fs::read_to_string(root.join(path)).map_err(|e| format!("cannot read {path}: {e}"))?;
        units.extend(spec.units(path, relative, &source));
    }
    Ok(units)
}

fn command_finding(job: &CommandJob, violation: CommandViolation) -> Finding {
    Finding {
        requirement: job.requirement.id.clone(),
        kind: FindingKind::Violation,
        file: violation.file,
        line: violation.line,
        message: violation.message,
        fix_hint: job.check.fix_hint.clone(),
        probability: None,
        model: None,
    }
}

fn judge_decisions(
    report: &mut Report,
    job: &DecisionJob,
    answers: Vec<ask::Answer>,
    log: &mut Option<Log>,
) {
    let question = job.spec.fingerprint();
    let unit_fields = |unit: &Unit| {
        json!({
            "requirement": job.requirement.id,
            "file": unit.file,
            "line": unit.line,
            "language": unit.language,
            "unit_fingerprint": runlog::fingerprint(&unit.source),
            "unit_source": unit.source,
            "has_enclosing": unit.enclosing.is_some(),
            "question_fingerprint": question,
            "blocking": job.blocking,
        })
    };
    let id = &job.requirement.id;
    let mut failures = Vec::new();
    for (unit, answer) in job.units.iter().zip(answers) {
        let judged = match answer {
            Ok(judged) => judged,
            Err(e) => {
                let mut fields = unit_fields(unit);
                fields["verdict"] = json!("error");
                fields["error"] = json!(e);
                log_line(log, "decision_unit", fields);
                failures.push(format!("{}:{}: {e}", unit.file, unit.line));
                continue;
            }
        };
        let verdict = job.spec.verdict(judged.probability);
        let mut fields = unit_fields(unit);
        fields["probability"] = json!(judged.probability);
        fields["model"] = json!(judged.model);
        fields["cached"] = json!(judged.cached);
        fields["verdict"] = json!(match verdict {
            Verdict::Pass => "pass",
            Verdict::Violation if job.blocking => "violation",
            Verdict::Violation => "advisory",
            Verdict::Uncertain => "uncertain",
        });
        log_line(log, "decision_unit", fields);
        let (kind, message) = match verdict {
            Verdict::Pass => continue,
            Verdict::Violation if job.blocking => {
                (FindingKind::Violation, job.spec.violation_message())
            }
            Verdict::Violation => (FindingKind::Advisory, job.spec.violation_message()),
            Verdict::Uncertain => (FindingKind::Uncertain, job.spec.question()),
        };
        report.findings.push(Finding {
            requirement: id.clone(),
            kind,
            file: Some(unit.file.clone()),
            line: Some(unit.line),
            message: message.to_string(),
            fix_hint: job.spec.fix_hint.clone(),
            probability: Some(judged.probability),
            model: Some(judged.model),
        });
    }
    if let Some(first) = failures.first() {
        report.errors.push(Error {
            requirement: Some(id.clone()),
            message: format!(
                "{} of {} units could not be judged, first at {first}",
                failures.len(),
                job.units.len()
            ),
        });
    }
}
