//! The Jev cache: a local, disposable file in the git folder, so checking
//! never modifies tracked files. CI can keep it between runs.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::git;
use crate::core::cache::{self, Cache};

/// The cache of a repository, open for one run.
pub struct Caches {
    path: PathBuf,
    cache: Cache,
    /// The text as read, to write the file back only when it changes.
    text: String,
}

impl Caches {
    pub fn open(root: &Path) -> Result<Self, String> {
        let path = git::common_dir(root)?
            .join("reqfile")
            .join(cache::FILE_NAME);
        let (cache, text) = match fs::read_to_string(&path) {
            Ok(text) => (
                Cache::parse(&text).map_err(|e| {
                    format!("{}: {e}; delete the file to rebuild it", path.display())
                })?,
                text,
            ),
            Err(e) if e.kind() == ErrorKind::NotFound => (Cache::default(), String::new()),
            Err(e) => return Err(format!("cannot read the Jev cache {}: {e}", path.display())),
        };
        Ok(Self { path, cache, text })
    }

    pub fn get(&mut self, key: &str) -> Option<f64> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, key: String, probability: f64) {
        self.cache.insert(key, probability);
    }

    /// Writes the cache atomically when it changed; with `prune`, only
    /// answers used in this run are kept.
    pub fn save(&self, prune: bool) -> Option<String> {
        let content = self.cache.render(prune);
        if content == self.text {
            return None;
        }
        let fail =
            |e: std::io::Error| format!("cannot write the Jev cache {}: {e}", self.path.display());
        let write = || -> Result<(), String> {
            fs::create_dir_all(self.path.parent().expect("the cache path has a parent"))
                .map_err(fail)?;
            let temporary = self
                .path
                .with_extension(format!("{}.tmp", std::process::id()));
            fs::write(&temporary, content).map_err(fail)?;
            fs::rename(&temporary, &self.path).map_err(fail)
        };
        write().err()
    }
}
