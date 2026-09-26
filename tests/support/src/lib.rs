//! Test support for reqfile: throwaway git repositories to run the real
//! binary in, and a local fake of Jev's System One endpoint.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};
use tempfile::TempDir;

/// A new repository running the `reqfile` binary of the calling test crate.
#[macro_export]
macro_rules! repo {
    () => {
        $crate::Repo::new(&$crate::binary!())
    };
}

/// The `reqfile` binary of the calling test crate, at a stable private path.
#[macro_export]
macro_rules! binary {
    () => {
        $crate::stable_binary(env!("CARGO_BIN_EXE_reqfile"), env!("CARGO_TARGET_TMPDIR"))
    };
}

/// A copy of the binary cargo built, at a path cargo never touches. Every
/// `cargo test` re-creates `target/debug/reqfile`, and on macOS a process
/// launched while that file is being replaced is killed (SIGKILL), which made
/// tests flaky when several `cargo test` ran at once. The copy is named by the
/// build's size and time, so all test processes of one build share it, and it
/// is written to a temporary name first so no process ever sees half of it.
pub fn stable_binary(built: &str, tmp: &str) -> String {
    // Once per test process: its tests run as threads sharing one process id.
    static COPY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    COPY.get_or_init(|| {
        let meta = fs::metadata(built).expect("cargo built reqfile");
        let built_at = meta
            .modified()
            .expect("a modification time")
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a time after 1970")
            .as_nanos();
        let copy = Path::new(tmp).join(format!("reqfile-{}-{built_at}", meta.len()));
        if !copy.exists() {
            let staging = copy.with_extension(format!("{}.tmp", std::process::id()));
            fs::copy(built, &staging).expect("copy reqfile");
            // Another test process may have put its identical copy in place
            // meanwhile; never replace a binary others may be launching.
            if fs::hard_link(&staging, &copy).is_err() && !copy.exists() {
                panic!("cannot place the reqfile copy at {}", copy.display());
            }
            fs::remove_file(&staging).expect("remove the staging copy");
        }
        copy.to_string_lossy().into_owned()
    })
    .clone()
}

/// The model reqfile asks when no config sets one.
pub const DEFAULT_MODEL: &str = "~typesafe/jev-latest";

/// A throwaway git repository.
pub struct Repo {
    dir: TempDir,
    binary: PathBuf,
    env: Mutex<Vec<(String, String)>>,
}

/// The result of running reqfile.
#[derive(Debug)]
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{}", self.stdout))
    }

    /// The combined output, for asserting on messages wherever they are printed.
    pub fn output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

impl Repo {
    pub fn new(binary: &str) -> Self {
        let repo = Self {
            dir: tempfile::tempdir().expect("create a temporary folder"),
            binary: PathBuf::from(binary),
            env: Mutex::new(Vec::new()),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["commit", "-q", "--allow-empty", "-m", "start"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Writes a file, creating its folders.
    pub fn write(&self, path: &str, content: &str) -> &Self {
        let path = self.path().join(path);
        fs::create_dir_all(path.parent().expect("a file has a parent")).expect("create folders");
        fs::write(path, content).expect("write file");
        self
    }

    pub fn remove(&self, path: &str) -> &Self {
        fs::remove_file(self.path().join(path)).expect("remove file");
        self
    }

    /// Commits every change.
    pub fn commit(&self, message: &str) -> &Self {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
        self
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.path())
            .envs(git_env())
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Sets an environment variable for the next runs of reqfile.
    pub fn env(&self, key: &str, value: &str) -> &Self {
        self.env
            .lock()
            .expect("env lock")
            .push((key.to_string(), value.to_string()));
        self
    }

    /// Runs reqfile from the repository root.
    pub fn run(&self, args: &[&str]) -> Run {
        self.run_in("", args)
    }

    /// Runs reqfile from a folder of the repository.
    pub fn run_in(&self, dir: &str, args: &[&str]) -> Run {
        let output = Command::new(&self.binary)
            .args(args)
            .current_dir(self.path().join(dir))
            .env_remove("OPENROUTER_API_KEY")
            .envs(git_env())
            .envs(self.env.lock().expect("env lock").iter().cloned())
            .output()
            .expect("run reqfile");
        Run {
            code: output.status.code().unwrap_or_else(|| {
                panic!(
                    "reqfile was killed by signal {:?}\nstdout: {}\nstderr: {}",
                    std::os::unix::process::ExitStatusExt::signal(&output.status),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                )
            }),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

/// Keeps the user's git configuration out of the tests.
fn git_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_AUTHOR_NAME", "Test"),
        ("GIT_AUTHOR_EMAIL", "test@example.com"),
        ("GIT_COMMITTER_NAME", "Test"),
        ("GIT_COMMITTER_EMAIL", "test@example.com"),
    ]
}

/// What the fake Jev answers to a request.
pub enum Reply {
    /// The same probability of a violation for every question of the request.
    Probability(f64),
    /// A probability per question id; questions not listed get no answer.
    ByQuestion(Vec<(&'static str, f64)>),
    /// An HTTP status with a body.
    Status(u16, String),
}

/// A request the fake Jev received.
#[derive(Debug, Clone)]
pub struct Received {
    pub path: String,
    pub authorization: Option<String>,
    pub body: Value,
}

/// A local stand-in for Jev's System One endpoint.
pub struct FakeJev {
    endpoint: String,
    received: Arc<Mutex<Vec<Received>>>,
    model: Arc<Mutex<Option<String>>>,
}

impl FakeJev {
    pub fn start(reply: impl Fn(&Value) -> Reply + Send + 'static) -> Self {
        let server = tiny_http::Server::http("127.0.0.1:0").expect("start the fake Jev");
        let endpoint = format!(
            "http://{}/api/v1",
            server.server_addr().to_ip().expect("an IP address")
        );
        let received = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&received);
        let model: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let answering = Arc::clone(&model);
        thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let mut body = String::new();
                request
                    .as_reader()
                    .read_to_string(&mut body)
                    .expect("read the request");
                let body: Value = serde_json::from_str(&body).expect("requests are JSON");
                let authorization = request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("Authorization"))
                    .map(|h| h.value.to_string());
                let ids: Vec<String> = body["questions"]
                    .as_object()
                    .map(|q| q.keys().cloned().collect())
                    .unwrap_or_default();
                let answers = |probabilities: Vec<(String, f64)>| {
                    let answers: serde_json::Map<String, Value> = probabilities
                        .into_iter()
                        .map(|(id, p)| (id, json!({ "type": "noul", "noul": p })))
                        .collect();
                    json!({
                        "model": answering.lock().expect("model lock").clone().unwrap_or_else(|| resolve(&body)),
                        "answers": answers,
                        "usage": { "input_tokens": 100, "output_tokens": 1 },
                    })
                    .to_string()
                };
                let (status, answer) = match reply(&body) {
                    Reply::Probability(p) => {
                        (200, answers(ids.into_iter().map(|id| (id, p)).collect()))
                    }
                    Reply::ByQuestion(list) => (
                        200,
                        answers(
                            list.into_iter()
                                .map(|(id, p)| (id.to_string(), p))
                                .collect(),
                        ),
                    ),
                    Reply::Status(status, body) => (status, body),
                };
                log.lock().expect("log lock").push(Received {
                    path: request.url().to_string(),
                    authorization,
                    body,
                });
                let response = tiny_http::Response::from_string(answer).with_status_code(status);
                request.respond(response).expect("respond");
            }
        });
        Self {
            endpoint,
            received,
            model,
        }
    }

    /// Answers 0.95 for units containing `VIOLATION`, 0.5 for `UNSURE`, 0.05 otherwise.
    pub fn by_marker() -> Self {
        Self::start(|body| {
            let unit = body["state"]["unit"].as_str().unwrap_or_default();
            Reply::Probability(if unit.contains("VIOLATION") {
                0.95
            } else if unit.contains("UNSURE") {
                0.5
            } else {
                0.05
            })
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// A `.reqfile/config.yaml` pointing decision checks at this fake; more
    /// `decision` keys can be appended as lines indented by two spaces.
    pub fn config(&self) -> String {
        format!("decision:\n  endpoint: {}\n", self.endpoint)
    }

    /// The exact model the fake answers as from now on, whatever is
    /// requested, like a new Jev release behind `latest`.
    pub fn answer_as(&self, model: &str) {
        *self.model.lock().expect("model lock") = Some(model.to_string());
    }

    /// Requests other than the probes reqfile sends to learn the current model.
    pub fn questions_received(&self) -> Vec<Received> {
        self.received()
            .into_iter()
            .filter(|r| r.body["questions"].get("probe").is_none())
            .collect()
    }

    pub fn received(&self) -> Vec<Received> {
        self.received.lock().expect("log lock").clone()
    }
}

/// A Reqfile with one requirement per `(id, checks)` pair, checks written in YAML.
pub fn reqfile(requirements: &[(&str, &str)]) -> String {
    let mut out = String::from("reqfile: 1\ncode:\n");
    for (id, checks) in requirements {
        out += &format!(
            "  - id: {id}\n    must: Something.\n    why: A reason.\n    who: Someone.\n    checks:\n"
        );
        for line in checks.trim_end().lines() {
            out += &format!("      {line}\n");
        }
    }
    out
}

/// A decision.yaml asking about Python functions, with the default test thresholds.
pub const PYTHON_FUNCTIONS: &str = r#"
units:
  - { language: python, rule: { kind: function_definition } }
question: Does this function do more than one job?
violation_when: It mixes several jobs.
ok_when: It does one job.
fix_hint: Split the function.
thresholds: { violation_above: 0.8, pass_below: 0.2 }
"#;

/// The exact model a requested one resolves to: the latest alias to Jev 1.13,
/// anything else to itself.
fn resolve(body: &Value) -> String {
    match body["model"].as_str() {
        Some(DEFAULT_MODEL) | None => "typesafe/jev-1.13-20260917".to_string(),
        Some(model) => model.to_string(),
    }
}
