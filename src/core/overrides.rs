//! `--command-override 'FROM=>TO'`: replaces a substring of command checks'
//! `run` for one run, never in any Reqfile, so a requirement package can be
//! measured with its working copy instead of its published runner.

use super::reqfile::Check;
use super::resolve::Effective;

/// One `--command-override`.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOverride {
    pub from: String,
    pub to: String,
}

/// Parses `FROM=>TO`, FROM not empty.
pub fn parse(arg: &str) -> Result<CommandOverride, String> {
    match arg.split_once("=>") {
        Some((from, to)) if !from.is_empty() => Ok(CommandOverride {
            from: from.to_string(),
            to: to.to_string(),
        }),
        _ => Err(format!(
            "`{arg}` is not FROM=>TO, the text of a command to replace and its replacement"
        )),
    }
}

/// Replaces each override's FROM by its TO in the `run` of every command
/// check of `blocks`, in order. An override whose FROM is in no command is
/// an error, so a typo never leaves the published command running silently.
pub fn apply(blocks: &mut [Effective], overrides: &[CommandOverride]) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for o in overrides {
        let mut matched = false;
        for block in blocks.iter_mut() {
            for check in &mut block.checks {
                if let Check::Command(command) = check
                    && command.run.contains(&o.from)
                {
                    command.run = command.run.replace(&o.from, &o.to);
                    matched = true;
                }
            }
        }
        if !matched {
            errors.push(format!(
                "--command-override: no command check runs `{}`",
                o.from
            ));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_splits_on_the_first_arrow_and_requires_a_from() {
        assert_eq!(
            parse("bunx pkg@1=>node a=>b.js"),
            Ok(CommandOverride {
                from: "bunx pkg@1".into(),
                to: "node a=>b.js".into()
            })
        );
        assert_eq!(parse("a=>").map(|o| o.to), Ok(String::new()));
        assert!(parse("=>b").is_err());
        assert!(parse("no arrow").is_err());
    }
}
