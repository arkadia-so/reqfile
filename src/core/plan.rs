//! Which files each check runs on.

use super::decision::DecisionSpec;
use super::reqfile::CommandCheck;

/// A file a block's checks may look at.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'s> {
    /// The repository path.
    pub path: &'s str,
    /// The path relative to the folder of the block's Reqfile.
    pub relative: &'s str,
    /// Whether the file exists: with `--changed`, deleted files are candidates too.
    pub exists: bool,
}

#[derive(Debug, PartialEq)]
pub enum CommandPlan {
    /// Run with these arguments, relative to the folder of the Reqfile.
    Run(Vec<String>),
    NoMatchingFiles,
}

pub fn plan_command(check: &CommandCheck, candidates: &[Candidate]) -> CommandPlan {
    let Some(glob) = &check.files else {
        return CommandPlan::Run(Vec::new());
    };
    let matching: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| glob.is_match(c.relative))
        .collect();
    if check.pass_files {
        // A deleted file cannot be handed to a tool; it only triggers commands that do not take files.
        let args: Vec<String> = matching
            .iter()
            .filter(|c| c.exists)
            .map(|c| c.relative.to_string())
            .collect();
        if args.is_empty() {
            CommandPlan::NoMatchingFiles
        } else {
            CommandPlan::Run(args)
        }
    } else if matching.is_empty() {
        CommandPlan::NoMatchingFiles
    } else {
        CommandPlan::Run(Vec::new())
    }
}

/// The existing files a decision check extracts units from.
pub fn decision_files<'s>(spec: &DecisionSpec, candidates: &[Candidate<'s>]) -> Vec<Candidate<'s>> {
    candidates
        .iter()
        .filter(|c| c.exists && spec.applies_to(c.relative))
        .copied()
        .collect()
}
