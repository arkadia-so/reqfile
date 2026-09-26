//! Running a check's shell command with a timeout.

use std::io::Read;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::command::Outcome;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Runs `script` with `sh` in `cwd`, `args` appended as arguments. With
/// `merge_stderr`, stderr is interleaved into stdout.
pub fn run(
    cwd: &Path,
    script: &str,
    args: &[String],
    merge_stderr: bool,
    timeout: Duration,
) -> Outcome {
    let mut script = if args.is_empty() {
        script.to_string()
    } else {
        format!("{script} \"$@\"")
    };
    if merge_stderr {
        script = format!("exec 2>&1\n{script}");
    }
    let spawned = Command::new("sh")
        .arg("-c")
        .arg(&script)
        .arg("sh")
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => return Outcome::LaunchFailed(e.to_string()),
    };
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline => break Err(kill_group(&mut child)),
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => break Err(Outcome::LaunchFailed(e.to_string())),
        }
    };
    let status = match status {
        Ok(status) => status,
        Err(outcome) => return outcome,
    };
    // Something the command started in the background can keep its output
    // open after it exits; the deadline still applies until the output ends.
    let finished = |drain: &Option<Drain>| drain.as_ref().is_none_or(|d| d.is_finished());
    while !(finished(&stdout) && finished(&stderr)) {
        if Instant::now() >= deadline {
            signal_group(child.id());
            return Outcome::TimedOut;
        }
        thread::sleep(POLL_INTERVAL);
    }
    let (stdout, stderr) = match (collect(stdout), collect(stderr)) {
        (Ok(stdout), Ok(stderr)) => (stdout, stderr),
        (Err(e), _) | (_, Err(e)) => return Outcome::LaunchFailed(e),
    };
    match (status.code(), status.signal()) {
        (Some(code), _) => Outcome::Exited {
            code,
            stdout,
            stderr,
        },
        (None, Some(signal)) => Outcome::Signaled(signal),
        (None, None) => Outcome::LaunchFailed("the command ended without an exit code".into()),
    }
}

/// Kills the command and everything it started.
fn kill_group(child: &mut Child) -> Outcome {
    signal_group(child.id());
    match child.wait() {
        Ok(_) => Outcome::TimedOut,
        Err(e) => Outcome::LaunchFailed(format!("timed out and could not be reaped: {e}")),
    }
}

/// Kills every process of the group created for the command.
fn signal_group(pid: u32) {
    // SAFETY: kill has no memory-safety preconditions; the negative pid
    // targets the process group created for this child.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

type Drain = thread::JoinHandle<std::io::Result<Vec<u8>>>;

fn drain(pipe: Option<impl Read + Send + 'static>) -> Option<Drain> {
    pipe.map(|mut pipe| {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            pipe.read_to_end(&mut buffer).map(|_| buffer)
        })
    })
}

fn collect(drain: Option<Drain>) -> Result<String, String> {
    match drain.map(|d| d.join()) {
        Some(Ok(Ok(bytes))) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Some(Ok(Err(e))) => Err(format!("the command's output could not be read: {e}")),
        Some(Err(_)) => Err("the command's output reader panicked".to_string()),
        None => Ok(String::new()),
    }
}
