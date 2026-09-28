//! The repository reqfile works on: its root, settings, target files,
//! Reqfiles and the effective requirement of every block, in the working
//! tree or at a commit.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::git;
use super::sources;
use crate::core::config::{self, Settings};
use crate::core::paths;
use crate::core::report::Source as SourceStatus;
use crate::core::reqfile::{self, Block, Check, GitRef, Kind, Location, Origin, Reqfile};
use crate::core::resolve::{self, Assets, Effective, Source};

pub struct Workspace {
    pub root: PathBuf,
    pub settings: Settings,
    /// Existing, non-excluded files, as repository paths.
    pub targets: BTreeSet<String>,
    /// Files in `.reqfile/` folders, as repository paths.
    pub asset_files: BTreeSet<String>,
    /// Sorted by path.
    pub reqfiles: Vec<Reqfile>,
    /// The effective requirement of every block, in Reqfile order.
    pub blocks: Vec<Effective>,
    /// The git sources of use blocks, with the commit each ref resolved to.
    pub sources: Vec<SourceStatus>,
    files: Files,
}

/// How file contents are read.
enum Files {
    /// From the working tree.
    Worktree(PathBuf),
    /// Read ahead from a commit: the Reqfiles, configs and requirement files.
    Commit(HashMap<String, Vec<u8>>),
}

impl Files {
    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        match self {
            Files::Worktree(root) => {
                fs::read(root.join(path)).map_err(|e| format!("{path}: cannot read: {e}"))
            }
            Files::Commit(contents) => contents
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: not read from the commit")),
        }
    }

    fn read_text(&self, path: &str) -> Result<String, String> {
        String::from_utf8(self.read(path)?).map_err(|_| format!("{path}: not valid UTF-8"))
    }
}

/// What discovery found in a tree of files.
struct Discovered {
    settings: Settings,
    targets: BTreeSet<String>,
    asset_files: BTreeSet<String>,
    reqfiles: Vec<Reqfile>,
}

impl Workspace {
    /// Loads the repository containing `cwd`, or every error found on the way.
    pub fn load(cwd: &Path) -> Result<Self, Vec<String>> {
        Self::load_trying(cwd, &[], None)
    }

    /// Loads the repository containing `cwd`, adding for this run a use
    /// block at its root for every code and process requirement defined at
    /// each location of `uses` (`--use`), restricted to `only` if given.
    pub fn load_trying(
        cwd: &Path,
        uses: &[String],
        only: Option<&[String]>,
    ) -> Result<Self, Vec<String>> {
        let root = git::root(cwd).map_err(|e| vec![e])?;
        // A file renamed but not yet staged is still listed under its old name.
        let listed: Vec<String> = git::files(&root)
            .map_err(|e| vec![e])?
            .into_iter()
            // Regular files only: a symlink could point outside the repository,
            // and its content would be handed to tools and sent to Jev.
            .filter(|p| fs::symlink_metadata(root.join(p)).is_ok_and(|m| m.is_file()))
            .collect();
        let trying = if uses.is_empty() {
            None
        } else {
            let here = cwd
                .canonicalize()
                .ok()
                .and_then(|c| {
                    c.strip_prefix(&root)
                        .ok()
                        .map(|p| p.to_string_lossy().into_owned())
                })
                .unwrap_or_default();
            Some(Trying { here, uses, only })
        };
        Self::from_files(root.clone(), listed, Files::Worktree(root), trying)
    }

    /// Loads the repository at `root` as it was at `commit`.
    pub fn at_commit(root: &Path, commit: &str) -> Result<Self, Vec<String>> {
        let listed = git::commit_files(root, commit).map_err(|e| vec![e])?;
        let needed: Vec<&str> = listed
            .iter()
            .map(String::as_str)
            .filter(|p| read_ahead(p))
            .collect();
        let contents = git::read_at(root, commit, &needed).map_err(|e| vec![e])?;
        Self::from_files(root.to_path_buf(), listed, Files::Commit(contents), None)
    }

    /// A repository of labeled example files, checked with one requirement
    /// and the given settings: how `reqfile test` runs a case.
    pub fn for_example(
        root: PathBuf,
        settings: Settings,
        block: Effective,
    ) -> Result<Self, String> {
        let targets = git::files(&root)?.into_iter().collect();
        Ok(Self {
            files: Files::Worktree(root.clone()),
            root,
            settings,
            targets,
            asset_files: BTreeSet::new(),
            reqfiles: Vec::new(),
            blocks: vec![block],
            sources: Vec::new(),
        })
    }

    fn from_files(
        root: PathBuf,
        listed: Vec<String>,
        files: Files,
        trying: Option<Trying>,
    ) -> Result<Self, Vec<String>> {
        let mut discovered = discover(&listed, &files)?;
        let mut errors: Vec<String> = resolve::check_unique_definitions(&discovered.reqfiles)
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut sources: Vec<Source> = Vec::new();
        if let Some(trying) = trying {
            match tried(&trying, &discovered.reqfiles, &mut sources) {
                Ok(reqfile) => discovered.reqfiles.push(reqfile),
                Err(e) => errors.extend(e),
            }
        }
        for (repo, reference) in git_locations(&discovered.reqfiles) {
            if sources
                .iter()
                .any(|s| s.repo == repo && s.reference == reference)
            {
                continue;
            }
            match load_source(&repo, &reference) {
                Ok(source) => sources.push(source),
                Err(e) => errors.extend(e),
            }
        }
        // A commit pin says what it is; a tag reports the commit it resolved to.
        let statuses = sources
            .iter()
            .filter(|s| matches!(s.reference, GitRef::Tag(_)))
            .map(|s| SourceStatus {
                location: format!("{}@{}", s.repo, s.reference.as_str()),
                commit: s.commit.clone(),
                offline: s.offline,
            })
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let blocks = resolve::resolve(&discovered.reqfiles, &sources, &discovered.asset_files)
            .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>())?;
        Ok(Self {
            root,
            settings: discovered.settings,
            targets: discovered.targets,
            asset_files: discovered.asset_files,
            reqfiles: discovered.reqfiles,
            blocks,
            sources: statuses,
            files,
        })
    }

    /// `path` (relative to `cwd`) as a repository path.
    pub fn repository_path(&self, cwd: &Path, path: &Path) -> Result<String, String> {
        let cwd = cwd
            .canonicalize()
            .map_err(|e| format!("cannot resolve the current folder: {e}"))?;
        let absolute = cwd.join(path);
        let relative = absolute
            .strip_prefix(&self.root)
            .map_err(|_| format!("{} is outside the repository", path.display()))?;
        paths::normalize(&relative.to_string_lossy())
            .ok_or_else(|| format!("{} is outside the repository", path.display()))
    }

    /// `$REQFILE_ASSETS` of a block: the folder holding the files of its checks.
    pub fn assets_path(&self, block: &Effective) -> PathBuf {
        match &block.assets {
            Assets::Repo(dir) => self.root.join(dir),
            Assets::External(dir) => dir.clone(),
        }
    }

    /// Identifies block `index` with its settings and the content of its
    /// requirement files, examples left out since checks never read them.
    pub fn fingerprint(&self, index: usize) -> Result<String, String> {
        let block = &self.blocks[index];
        let mut hasher = Sha256::new();
        match &block.assets {
            Assets::Repo(dir) => {
                let prefix = format!("{dir}/");
                let examples = format!("{dir}/examples/");
                for path in self
                    .asset_files
                    .iter()
                    .filter(|p| p.starts_with(&prefix) && !p.starts_with(&examples))
                {
                    hasher.update(format!("{}\n", &path[prefix.len()..]));
                    hasher.update(Sha256::digest(self.files.read(path)?));
                }
            }
            Assets::External(dir) => {
                for (relative, path) in walk(dir)? {
                    if relative.starts_with("examples/") {
                        continue;
                    }
                    hasher.update(format!("{relative}\n"));
                    let content = fs::read(&path)
                        .map_err(|e| format!("{}: cannot read: {e}", path.display()))?;
                    hasher.update(Sha256::digest(content));
                }
            }
        }
        let assets: String = hasher.finalize()[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(resolve::fingerprint(
            block,
            &assets,
            &self.settings.fingerprint(
                &block.dir,
                block.checks.iter().any(|c| matches!(c, Check::Decision(_))),
            ),
        ))
    }
}

/// `--use` for one run: where it was given from, and what it asks for.
struct Trying<'a> {
    /// The current folder, as a repository path.
    here: String,
    uses: &'a [String],
    only: Option<&'a [String]>,
}

/// The Reqfile `--use` adds at the root for this run: a use block for every
/// code and process requirement defined at each location. Sources it
/// fetches go to `sources`.
fn tried(
    trying: &Trying,
    reqfiles: &[Reqfile],
    sources: &mut Vec<Source>,
) -> Result<Reqfile, Vec<String>> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut errors = Vec::new();
    for text in trying.uses {
        let location = reqfile::parse_location(text).map_err(|e| vec![format!("--use: {e}")])?;
        let (definitions, location, own_folder) = match location {
            Location::Local(path) => {
                let folder = paths::normalize(&paths::join(&trying.here, &path))
                    .ok_or_else(|| vec![format!("--use {text}: outside the repository")])?;
                let at_root = if folder.is_empty() {
                    ".".to_string()
                } else {
                    format!("./{folder}")
                };
                (
                    definitions_under(reqfiles, &folder),
                    Location::Local(at_root),
                    Some(folder),
                )
            }
            Location::Git { repo, reference } => {
                let source = load_source(&repo, &reference)?;
                let found = definitions_under(&source.reqfiles, "");
                sources.push(source);
                (found, Location::Git { repo, reference }, None)
            }
        };
        let chosen: Vec<(String, Kind)> = definitions
            .into_iter()
            .filter(|(id, _)| trying.only.is_none_or(|ids| ids.contains(id)))
            .collect();
        if chosen.is_empty() {
            errors.push(format!(
                "--use {text}: no code or process requirement to try there"
            ));
        }
        for (id, kind) in chosen {
            // A local folder's own definitions are what is tried, not a clash.
            let elsewhere = reqfiles
                .iter()
                .filter(|r| {
                    own_folder
                        .as_ref()
                        .is_none_or(|f| !paths::contains_dir(f, &r.dir))
                })
                .flat_map(|r| &r.blocks)
                .any(|b| b.id == id);
            if elsewhere || blocks.iter().any(|b| b.id == id) {
                errors.push(format!(
                    "--use {text}: {id} is already required here; try it with --only on other ids, or edit the Reqfile"
                ));
                continue;
            }
            blocks.push(Block {
                id,
                kind,
                line: 1,
                why: None,
                who: None,
                checks: None,
                origin: Origin::Use(location.clone()),
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Reqfile {
        path: "--use".to_string(),
        dir: String::new(),
        blocks,
    })
}

/// The code and process definitions of the Reqfiles at or under `folder`.
fn definitions_under(reqfiles: &[Reqfile], folder: &str) -> Vec<(String, Kind)> {
    reqfiles
        .iter()
        .filter(|r| paths::contains_dir(folder, &r.dir))
        .flat_map(|r| &r.blocks)
        .filter(|b| matches!(b.origin, Origin::Definition { .. }) && b.kind.importable())
        .map(|b| (b.id.clone(), b.kind))
        .collect()
}

/// Files read ahead from a commit: whatever discovery and fingerprints read.
fn read_ahead(path: &str) -> bool {
    let name = paths::file_name(path);
    let in_reqfile_folder = path.split('/').any(|part| part == ".reqfile");
    name == reqfile::FILE_NAME
        || reqfile::is_near_miss(path)
        || (in_reqfile_folder && !path.contains("/examples/"))
}

/// Every git location named by a use block, once.
fn git_locations(reqfiles: &[Reqfile]) -> BTreeSet<(String, GitRef)> {
    reqfiles
        .iter()
        .flat_map(|r| &r.blocks)
        .filter_map(|b| match &b.origin {
            Origin::Use(Location::Git { repo, reference }) => {
                Some((repo.clone(), reference.clone()))
            }
            _ => None,
        })
        .collect()
}

/// A git source: fetched, then discovered like a repository of its own,
/// following its own exclusions. Its settings serve discovery only.
pub fn load_source(repo: &str, reference: &GitRef) -> Result<Source, Vec<String>> {
    let fetched = sources::fetch(repo, reference).map_err(|e| vec![e])?;
    let listed: Vec<String> = walk(&fetched.dir)
        .map_err(|e| vec![e])?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let location = format!("{repo}@{}", reference.as_str());
    let discovered =
        discover(&listed, &Files::Worktree(fetched.dir.clone())).map_err(|errors| {
            errors
                .into_iter()
                .map(|e| format!("in {location}: {e}"))
                .collect::<Vec<_>>()
        })?;
    Ok(Source {
        repo: repo.to_string(),
        reference: reference.clone(),
        commit: fetched.commit,
        offline: fetched.offline,
        root: fetched.dir,
        reqfiles: discovered.reqfiles,
    })
}

/// The regular files under `dir`, relative to it, sorted.
fn walk(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let entries =
            fs::read_dir(&folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot read {}: {e}", folder.display()))?;
            let kind = entry
                .file_type()
                .map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(dir)
                    .expect("walked paths are under the folder")
                    .to_string_lossy()
                    .into_owned();
                found.insert(relative, path);
            }
        }
    }
    Ok(found.into_iter().collect())
}

/// Settings, targets and Reqfiles of a tree of files, or every error found.
fn discover(listed: &[String], files: &Files) -> Result<Discovered, Vec<String>> {
    let mut errors = Vec::new();
    let settings = load_settings(listed, files, &mut errors);
    let targets: BTreeSet<String> = listed
        .iter()
        .filter(|p| !settings.is_excluded(p))
        .cloned()
        .collect();
    let asset_files = listed
        .iter()
        .filter(|p| p.split('/').any(|part| part == ".reqfile"))
        .filter(|p| settings.excluded_by(p).is_none())
        .cloned()
        .collect();
    let reqfiles = load_reqfiles(&targets, files, &mut errors);
    if errors.is_empty() {
        Ok(Discovered {
            settings,
            targets,
            asset_files,
            reqfiles,
        })
    } else {
        Err(errors)
    }
}

/// Every `.reqfile/config.yaml` among `listed`, read shallowest first so a
/// config in a folder a parent excludes is never read.
fn load_settings(listed: &[String], files: &Files, errors: &mut Vec<String>) -> Settings {
    let mut configs: Vec<(&str, &String)> = listed
        .iter()
        .filter_map(|p| config::config_dir(p).map(|dir| (dir, p)))
        .collect();
    configs.sort_by_key(|(dir, _)| config::depth(dir));
    let mut settings = Settings::default();
    for (_, path) in configs {
        if settings.excluded_by(path).is_some() {
            continue;
        }
        match files
            .read_text(path)
            .and_then(|text| config::parse(path, &text).map_err(|e| e.to_string()))
        {
            Ok(config) => settings.add(config),
            Err(e) => errors.push(e),
        }
    }
    for path in listed
        .iter()
        .filter(|p| config::is_near_miss(p) && settings.excluded_by(p).is_none())
    {
        errors.push(format!(
            "{path}:1: a config file must be named exactly .reqfile/config.yaml"
        ));
    }
    settings
}

/// Every Reqfile among the targets, sorted by path.
fn load_reqfiles(
    targets: &BTreeSet<String>,
    files: &Files,
    errors: &mut Vec<String>,
) -> Vec<Reqfile> {
    for path in targets.iter().filter(|p| reqfile::is_near_miss(p)) {
        match files.read_text(path) {
            Ok(text) if !reqfile::looks_like_reqfile(&text) => {}
            Ok(_) => errors.push(format!(
                "{path}:1: a requirements file must be named exactly {}",
                reqfile::FILE_NAME
            )),
            Err(e) => errors.push(e),
        }
    }
    let mut reqfiles = Vec::new();
    for path in targets
        .iter()
        .filter(|p| paths::file_name(p) == reqfile::FILE_NAME)
    {
        match files
            .read_text(path)
            .and_then(|text| reqfile::parse(path, &text).map_err(|e| e.to_string()))
        {
            Ok(reqfile) => reqfiles.push(reqfile),
            Err(e) => errors.push(e),
        }
    }
    reqfiles
}
