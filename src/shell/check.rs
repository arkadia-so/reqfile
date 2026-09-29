//! `reqfile check`: plans every check, runs commands and Jev calls, and reports.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::ask;
use super::conflicts;
use super::jev_cache::Caches;
use super::pool;
use super::process;
use super::workspace::Workspace;
use crate::core::command::{self, CommandViolation};
use crate::core::config::{self, DecisionConfig};
use crate::core::decision::{self, DecisionSpec, Unit, Verdict};
use crate::core::paths;
use crate::core::plan::{self, Candidate, CommandPlan};
use crate::core::report::{CheckRun, Error, Finding, FindingKind, Report, RequirementRun, Setup};
use crate::core::reqfile::{self, Check, CommandCheck, OutputFormat};
use crate::core::resolve::{self, Assets, Effective, Scope};
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
    /// `--use LOCATION`, repeated: requirements to try for this run.
    pub uses: Vec<String>,
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
    block: &'w Effective,
    /// Its requirement's position in `Report::requirements`.
    requirement: usize,
    check: &'w CommandCheck,
    args: Vec<String>,
}

struct DecisionJob<'w> {
    block: &'w Effective,
    requirement: usize,
    /// How to call Jev, from the settings of that folder.
    jev: DecisionConfig,
    spec: DecisionSpec,
    blocking: bool,
    units: Vec<Unit>,
}

pub fn run(cwd: &Path, options: &Options) -> Report {
    match Workspace::load_trying(cwd, &options.uses, options.only.as_deref()) {
        Ok(workspace) => run_in(&workspace, options),
        Err(errors) => Report::from_errors(errors),
    }
}

/// Runs the checks of a loaded workspace.
pub fn run_in(workspace: &Workspace, options: &Options) -> Report {
    let selected = match select(&workspace.blocks, options.only.as_deref()) {
        Ok(selected) => selected,
        Err(errors) => return Report::from_errors(errors),
    };
    let specs = match load_specs(workspace, &selected) {
        Ok(specs) => specs,
        Err(errors) => return Report::from_errors(errors),
    };
    let changes = match changes(workspace, &selected, options) {
        Ok(changes) => changes,
        Err(error) => return Report::from_errors([error]),
    };
    let candidates = match selected
        .iter()
        .map(|&i| candidates(workspace, changes.as_ref(), i))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(candidates) => candidates,
        Err(error) => return Report::from_errors([error]),
    };
    let mut log = match open_log(workspace, options) {
        Ok(log) => log,
        Err(error) => return Report::from_errors([error]),
    };
    let started = std::time::Instant::now();
    let mut report = Report {
        sources: workspace.sources.clone(),
        requirements: selected
            .iter()
            .map(|&i| {
                let block = &workspace.blocks[i];
                RequirementRun {
                    id: block.id.clone(),
                    reqfile: block.reqfile.clone(),
                    kind: block.kind.as_str(),
                    must: block.must.clone(),
                    source: block
                        .imported
                        .as_ref()
                        .map(|imported| match &imported.location {
                            reqfile::Location::Git { repo, .. } => format!(
                                "{repo}@{}",
                                imported
                                    .commit
                                    .as_deref()
                                    .map_or("", |c| &c[..c.len().min(7)])
                            ),
                            reqfile::Location::Local(path) => path.clone(),
                        }),
                    ..RequirementRun::default()
                }
            })
            .collect(),
        ..Report::default()
    };
    let plan = plan(
        workspace,
        &selected,
        specs,
        &candidates,
        options.fast,
        &mut report,
    );
    report.checks_run += plan.commands.len() + plan.decisions.len();
    report.setup = missing_setup(&plan.decisions);
    run_commands(workspace, &plan.commands, &mut report, &mut log);
    // Decisions and conflicts share one Jev cache, saved once at the end, so
    // pruning keeps the answers of both.
    let mut caches = Caches::open(&workspace.root);
    let decided = selected.iter().any(|&i| {
        workspace.blocks[i]
            .checks
            .iter()
            .any(|c| matches!(c, Check::Decision(_)))
    });
    if decided {
        run_decisions(
            &plan.decisions,
            caches.as_mut().map_err(|e| &*e),
            &mut report,
            &mut log,
        );
    }
    let mut answered = Vec::new();
    conflicts::check(
        workspace,
        &mut report,
        caches.as_mut().map_err(|e| &*e),
        &mut answered,
    );
    for (finding, other, probability) in &answered {
        log_line(
            &mut log,
            "conflict",
            conflicts::log_fields(&report, *finding, other, *probability),
        );
    }
    if (decided || !answered.is_empty())
        && let Ok(caches) = &caches
    {
        // Only a run that saw every unit knows which cached answers are obsolete.
        let prune = decided && options.changed.is_none() && options.only.is_none();
        if let Some(message) = caches.save(prune) {
            // A cache that cannot be written blocks the run only if a blocking decision check ran.
            let blocking = plan.decisions.iter().any(|job| job.blocking);
            report.errors.push(Error {
                requirement: None,
                message,
                blocking,
            });
        }
    }
    if let (Some(path), Some(log)) = (&options.log, log)
        && let Err(message) = append_log(path, &log.lines)
    {
        report.errors.push(Error::blocking(None, message));
    }
    // Blocking violations first, then advisory ones, then uncertain ones, each in Reqfile order.
    report.findings.sort_by_key(|f| f.kind);
    report.millis = started.elapsed().as_millis() as u64;
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

/// Plans every check of the selected blocks against their candidate files;
/// checks with nothing to check and units that cannot be read are recorded
/// in `report`.
fn plan<'w>(
    workspace: &'w Workspace,
    selected: &[usize],
    specs: Vec<Option<DecisionSpec>>,
    candidates: &[Vec<Candidate>],
    fast: bool,
    report: &mut Report,
) -> Plan<'w> {
    let mut plan = Plan {
        commands: Vec::new(),
        decisions: Vec::new(),
    };
    for (requirement, ((&index, mut spec), candidates)) in
        selected.iter().zip(specs).zip(candidates).enumerate()
    {
        let block = &workspace.blocks[index];
        for check in &block.checks {
            match check {
                Check::Command(check) if fast && !check.fast => {
                    report.checks_left_for_full_run += 1;
                    report.requirements[requirement].deferred += 1;
                }
                Check::Command(check) => match plan::plan_command(check, candidates) {
                    CommandPlan::Run(args) => plan.commands.push(CommandJob {
                        block,
                        requirement,
                        check,
                        args,
                    }),
                    CommandPlan::NoMatchingFiles => {
                        report.checks_with_nothing_to_check += 1;
                        report.requirements[requirement].nothing_to_check += 1;
                    }
                },
                Check::Decision(check) => {
                    let spec = spec
                        .take()
                        .expect("a spec was loaded for the decision check");
                    let files = plan::decision_files(&spec, candidates);
                    match extract_units(&workspace.root, &spec, &files) {
                        // No file, or no code in them is a unit: nothing would be judged.
                        Ok(units) if units.is_empty() => {
                            report.checks_with_nothing_to_check += 1;
                            report.requirements[requirement].nothing_to_check += 1;
                        }
                        Ok(units) => plan.decisions.push(DecisionJob {
                            block,
                            requirement,
                            jev: workspace.settings.decision(&block.dir),
                            spec,
                            blocking: check.blocking,
                            units,
                        }),
                        Err(message) => {
                            report.checks_run += 1;
                            report.errors.push(Error {
                                requirement: Some(block.id.clone()),
                                message,
                                blocking: check.blocking,
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
    // Commands can run reqfile itself, such as `$REQFILE list --format json`,
    // with the same version as the run.
    let executable = std::env::current_exe().ok();
    let outcomes = pool::map(commands, parallelism, |job| {
        let started = std::time::Instant::now();
        let assets = workspace.assets_path(job.block);
        let mut env: Vec<(&str, &Path)> = vec![("REQFILE_ASSETS", &assets)];
        if let Some(executable) = &executable {
            env.push(("REQFILE", executable));
        }
        let outcome = process::run(
            &workspace.root.join(&job.block.dir),
            &job.check.run,
            &job.args,
            &env,
            job.check.format == OutputFormat::Exit,
            Duration::from_secs(job.check.timeout_secs),
        );
        (
            command::judge(job.check, outcome, &job.block.dir, &root),
            started.elapsed().as_millis() as u64,
        )
    });
    for (job, (outcome, millis)) in commands.iter().zip(outcomes) {
        report.requirements[job.requirement]
            .checks
            .push(CheckRun::Command {
                run: job.check.run.clone(),
                files: job.args.len(),
                millis,
            });
        let id = &job.block.id;
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
                        json!({ "requirement": id, "file": v.file, "line": v.line, "message": v.message, "probability": v.probability, "uncertain": v.uncertain }),
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
                // An advisory command that cannot run could never have blocked.
                report.errors.push(Error {
                    requirement: Some(id.clone()),
                    message,
                    blocking: job.check.blocking,
                });
            }
        }
    }
}

/// The Jev keys the planned decision checks need and the environment lacks.
fn missing_setup(decisions: &[DecisionJob]) -> Vec<Setup> {
    let mut setup: Vec<Setup> = Vec::new();
    for job in decisions {
        let variable = &job.jev.api_key_env;
        if std::env::var(variable).is_ok_and(|v| !v.trim().is_empty()) {
            continue;
        }
        let entry = match setup.iter().position(|s| &s.variable == variable) {
            Some(i) => &mut setup[i],
            None => {
                setup.push(Setup {
                    variable: variable.clone(),
                    requirements: Vec::new(),
                    blocking: false,
                });
                setup.last_mut().expect("just pushed")
            }
        };
        if !entry.requirements.contains(&job.block.id) {
            entry.requirements.push(job.block.id.clone());
        }
        entry.blocking |= job.blocking;
    }
    setup
}

/// Asks Jev about the units of the decision checks and records its verdicts.
fn run_decisions(
    decisions: &[DecisionJob],
    caches: Result<&mut Caches, &String>,
    report: &mut Report,
    log: &mut Option<Log>,
) {
    let checks: Vec<ask::Check> = decisions
        .iter()
        .map(|job| ask::Check {
            id: &job.block.id,
            spec: &job.spec,
            jev: &job.jev,
            units: &job.units,
        })
        .collect();
    let answers = ask::answers(caches, &checks);
    report.units_judged = Some(answers.iter().flatten().filter(|a| a.is_ok()).count());
    for (job, answers) in decisions.iter().zip(answers) {
        report.requirements[job.requirement]
            .checks
            .push(CheckRun::Decision {
                units: answers.iter().filter(|a| a.is_ok()).count(),
                cached: answers
                    .iter()
                    .filter(|a| a.as_ref().is_ok_and(|j| j.cached))
                    .count(),
                model: answers
                    .iter()
                    .find_map(|a| a.as_ref().ok().map(|j| j.model.clone())),
            });
        judge_decisions(report, job, answers, log);
    }
}

/// The blocks to check, in Reqfile order, restricted by `--only`.
fn select(blocks: &[Effective], only: Option<&[String]>) -> Result<Vec<usize>, Vec<String>> {
    let Some(only) = only else {
        return Ok((0..blocks.len()).collect());
    };
    let unknown: Vec<String> = only
        .iter()
        .filter(|id| !blocks.iter().any(|b| &b.id == *id))
        .map(|id| format!("--only: no requirement has the id {id}"))
        .collect();
    if unknown.is_empty() {
        Ok((0..blocks.len())
            .filter(|&i| only.contains(&blocks[i].id))
            .collect())
    } else {
        Err(unknown)
    }
}

/// The decision.yaml of every selected block with a decision check, read
/// from its `$REQFILE_ASSETS`, in order.
fn load_specs(
    workspace: &Workspace,
    selected: &[usize],
) -> Result<Vec<Option<DecisionSpec>>, Vec<String>> {
    let mut specs = Vec::new();
    let mut errors = Vec::new();
    for &index in selected {
        let block = &workspace.blocks[index];
        let Some(check) = block.checks.iter().find_map(|c| match c {
            Check::Decision(d) => Some(d),
            Check::Command(_) => None,
        }) else {
            specs.push(None);
            continue;
        };
        // Inherited checks are declared in the definition; the block is what applies here.
        let at = match &block.imported {
            None => format!("{}:{}", block.reqfile, check.line),
            Some(_) => block.block(),
        };
        let jev = workspace.settings.decision(&block.dir);
        if check.blocking && !jev.model_is_pinned() {
            errors.push(format!(
                "{at}: the decision check of {} is blocking, so its model must be pinned, but {} follows `{}`; set `decision.model` to an exact version, such as typesafe/jev-1.13-20260917, in {}",
                block.id,
                paths::display_dir(&block.dir),
                jev.model,
                config::path(&block.dir)
            ));
            specs.push(None);
            continue;
        }
        let path = workspace.assets_path(block).join("decision.yaml");
        let shown = match &block.assets {
            Assets::Repo(dir) => paths::join(dir, "decision.yaml"),
            Assets::External(_) => path.to_string_lossy().into_owned(),
        };
        match fs::read_to_string(&path) {
            Ok(text) => match decision::parse_spec(&shown, &text) {
                Ok(spec) => specs.push(Some(spec)),
                Err(e) => errors.push(e.to_string()),
            },
            Err(e) => errors.push(format!(
                "{at}: the decision check of {} needs {shown}: {e}",
                block.id
            )),
        }
    }
    if errors.is_empty() {
        Ok(specs)
    } else {
        Err(errors)
    }
}

/// What changed since the base of `--changed`, for each base in use.
struct Changes {
    by_base: BTreeMap<String, BaseChanges>,
    /// The base of each selected block with a check that will run.
    base_of: HashMap<usize, String>,
}

struct BaseChanges {
    /// Every changed path, including excluded ones such as `.reqfile/` files.
    all: BTreeSet<String>,
    /// Changed paths that checks may target, deleted ones included.
    targets: BTreeSet<String>,
    requirements: Requirements,
}

/// The requirements at the merge-base, to compare each block with.
enum Requirements {
    /// No Reqfile, config or requirement file changed, and no git source
    /// could have moved: every block is as it was.
    Unchanged,
    At(Box<Workspace>),
    /// The merge-base's requirements cannot be read, such as an invalid
    /// Reqfile there: every block counts as changed.
    Unreadable,
}

/// With `--changed`, the changes since the base of each folder; a base is
/// resolved only for the selected blocks with a check that will run.
fn changes(
    workspace: &Workspace,
    selected: &[usize],
    options: &Options,
) -> Result<Option<Changes>, String> {
    let Some(cli_base) = &options.changed else {
        return Ok(None);
    };
    let default_base = super::git::default_base(&workspace.root)?;
    let runs = |check: &Check| match check {
        Check::Command(command) => !options.fast || command.fast,
        Check::Decision(_) => true,
    };
    let mut changes = Changes {
        by_base: BTreeMap::new(),
        base_of: HashMap::new(),
    };
    for &index in selected {
        let block = &workspace.blocks[index];
        if !block.checks.iter().any(runs) {
            continue;
        }
        let base = cli_base
            .clone()
            .or_else(|| workspace.settings.base(&block.dir).map(String::from))
            .or_else(|| default_base.clone())
            .ok_or_else(|| {
                format!(
                    "--changed needs a base for {}: pass one (reqfile check --changed origin/main), set `base` in {}, or set origin/HEAD (git remote set-head origin --auto)",
                    paths::display_dir(&block.dir),
                    config::path(&block.dir)
                )
            })?;
        if !changes.by_base.contains_key(&base) {
            let merge_base = super::git::merge_base(&workspace.root, &base)?;
            let all: BTreeSet<String> = super::git::changed(&workspace.root, &merge_base)?
                .into_iter()
                .collect();
            let targets = all
                .iter()
                .filter(|p| !workspace.settings.is_excluded(p))
                .cloned()
                .collect();
            let requirements_changed = all.iter().any(|p| {
                paths::file_name(p) == crate::core::reqfile::FILE_NAME
                    || p.split('/').any(|part| part == ".reqfile")
            });
            let requirements = if !requirements_changed && workspace.sources.is_empty() {
                Requirements::Unchanged
            } else {
                match Workspace::at_commit(&workspace.root, &merge_base) {
                    Ok(at) => Requirements::At(Box::new(at)),
                    Err(_) => Requirements::Unreadable,
                }
            };
            changes.by_base.insert(
                base.clone(),
                BaseChanges {
                    all,
                    targets,
                    requirements,
                },
            );
        }
        changes.base_of.insert(index, base);
    }
    Ok(Some(changes))
}

/// The files block `index` may look at: its whole scope, or with
/// `--changed`, the files that changed and those whose block is not the
/// same as at the base (another block, or one whose requirement, files,
/// settings or source changed), since unchanged files may now fail it.
fn candidates<'w>(
    workspace: &'w Workspace,
    changes: Option<&'w Changes>,
    index: usize,
) -> Result<Vec<Candidate<'w>>, String> {
    let scope = Scope::of(&workspace.blocks, index);
    let in_scope = |path: &'w String| {
        scope.relative(path).map(|relative| Candidate {
            path,
            relative,
            exists: workspace.targets.contains(path),
        })
    };
    let Some(changes) = changes else {
        return Ok(workspace.targets.iter().filter_map(in_scope).collect());
    };
    let Some(base) = changes.base_of.get(&index) else {
        // No check of this block runs.
        return Ok(Vec::new());
    };
    let changed = &changes.by_base[base];
    let block = &workspace.blocks[index];
    let now = match &changed.requirements {
        Requirements::At(_) => Some(workspace.fingerprint(index)?),
        Requirements::Unchanged | Requirements::Unreadable => None,
    };
    let mut same_block: HashMap<&str, bool> = HashMap::new();
    let mut is_same = |dir: &'w str| -> bool {
        let Requirements::At(at) = &changed.requirements else {
            return matches!(changed.requirements, Requirements::Unchanged);
        };
        *same_block.entry(dir).or_insert_with(|| {
            resolve::nearest(&at.blocks, dir, &block.id)
                .is_some_and(|i| at.fingerprint(i).ok() == now)
        })
    };
    let deleted = changed
        .targets
        .iter()
        .filter(|p| !workspace.targets.contains(*p));
    Ok(workspace
        .targets
        .iter()
        .chain(deleted)
        .filter_map(in_scope)
        .filter(|c| {
            let newly_targeted = match &changed.requirements {
                Requirements::At(at) => c.exists && !at.targets.contains(c.path),
                Requirements::Unchanged | Requirements::Unreadable => false,
            };
            changed.all.contains(c.path) || newly_targeted || !is_same(paths::parent(c.path))
        })
        .collect())
}

fn extract_units(
    root: &Path,
    spec: &DecisionSpec,
    files: &[Candidate],
) -> Result<Vec<Unit>, String> {
    let mut units = Vec::new();
    for file in files {
        let source = fs::read_to_string(root.join(file.path))
            .map_err(|e| format!("cannot read {}: {e}", file.path))?;
        units.extend(spec.units(file.path, file.relative, &source));
    }
    Ok(units)
}

/// For a requirement taken with `use`, its block and where it was resolved.
fn provenance(block: &Effective) -> (Option<String>, Option<String>) {
    match &block.imported {
        Some(imported) => (Some(block.block()), Some(imported.describe())),
        None => (None, None),
    }
}

fn command_finding(job: &CommandJob, violation: CommandViolation) -> Finding {
    let (block, source) = provenance(job.block);
    Finding {
        requirement: job.block.id.clone(),
        kind: if violation.uncertain {
            FindingKind::Uncertain
        } else if job.check.blocking {
            FindingKind::Violation
        } else {
            FindingKind::Advisory
        },
        file: violation.file,
        line: violation.line,
        message: violation.message,
        fix_hint: job.check.fix_hint.clone(),
        probability: violation.probability,
        model: None,
        block,
        source,
        conflicts: Vec::new(),
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
            "requirement": job.block.id,
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
    let id = &job.block.id;
    let (block, source) = provenance(job.block);
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
            block: block.clone(),
            source: source.clone(),
            conflicts: Vec::new(),
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
            blocking: job.blocking,
        });
    }
}
