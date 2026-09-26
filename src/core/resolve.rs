//! Use blocks resolved to their definitions: the effective requirement of
//! every block, the files each one applies to, and what identifies it.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use super::error::ConfigError;
use super::paths;
use super::reqfile::{Block, Check, GitRef, Kind, Location, Origin, Reqfile};

/// The folder holding a requirement's own files, `.reqfile/<ID>/`, next to its Reqfile.
pub fn assets_dir(reqfile_dir: &str, id: &str) -> String {
    paths::join(reqfile_dir, &format!(".reqfile/{id}"))
}

/// A repository taken by reference, at the commit its ref resolved to.
pub struct Source {
    pub repo: String,
    pub reference: GitRef,
    pub commit: String,
    /// Whether the ref could not be resolved again and the last cached
    /// resolution was used.
    pub offline: bool,
    /// The files of the commit, on disk.
    pub root: PathBuf,
    /// Reqfiles discovered in the commit, following its own exclusions.
    pub reqfiles: Vec<Reqfile>,
}

/// Where the files of a requirement's checks are: `$REQFILE_ASSETS`.
#[derive(Debug, Clone, PartialEq)]
pub enum Assets {
    /// A `.reqfile/<ID>/` folder of the repository, as a repository path.
    Repo(String),
    /// A folder outside the repository, on disk: the `.reqfile/<ID>/` of a
    /// git source, or of the repository while `reqfile test` runs a case.
    External(PathBuf),
}

/// A block with its effective requirement: for a definition, itself; for a
/// use block, its definition's `must`, `ref` and type, with the `why`, `who`
/// and `checks` it sets itself.
#[derive(Debug, Clone)]
pub struct Effective {
    /// The Reqfile holding the block, whose folder its checks run in.
    pub reqfile: String,
    pub dir: String,
    pub line: usize,
    pub id: String,
    pub kind: Kind,
    pub must: String,
    pub why: String,
    pub who: Option<String>,
    pub reference: Option<String>,
    pub checks: Vec<Check>,
    /// The checks as written, which identify them.
    pub checks_source: String,
    pub imported: Option<Imported>,
    pub assets: Assets,
}

/// Where a use block's requirement comes from.
#[derive(Debug, Clone)]
pub struct Imported {
    pub location: Location,
    /// The definition, `path:line`, its path relative to the source's root
    /// for a git source and to the repository root for a folder.
    pub definition: String,
    /// The commit of a git source.
    pub commit: Option<String>,
    /// Whether the checks are the definition's, rather than set by the use block.
    pub checks_inherited: bool,
    /// The definition's `.reqfile/<ID>/`, which holds its labeled examples.
    pub definition_assets: Assets,
}

impl Imported {
    /// Where the requirement is defined, as shown to people.
    pub fn describe(&self) -> String {
        match &self.commit {
            Some(commit) => format!(
                "{} ({} at commit {commit})",
                self.location.describe(),
                self.definition
            ),
            None => format!("{} ({})", self.location.describe(), self.definition),
        }
    }
}

impl Effective {
    /// The block, `path:line`.
    pub fn block(&self) -> String {
        format!("{}:{}", self.reqfile, self.line)
    }
}

/// Every definition id appears once among the Reqfiles of a repository.
pub fn check_unique_definitions(reqfiles: &[Reqfile]) -> Vec<ConfigError> {
    let mut seen: HashMap<&str, &Reqfile> = HashMap::new();
    let mut errors = Vec::new();
    for reqfile in reqfiles {
        for block in reqfile
            .blocks
            .iter()
            .filter(|b| matches!(b.origin, Origin::Definition { .. }))
        {
            match seen.get(block.id.as_str()) {
                Some(first) => errors.push(ConfigError::at(
                    &reqfile.path,
                    block.line,
                    format!(
                        "requirement {} is also defined in {}; define it once and take it elsewhere with `use`",
                        block.id, first.path
                    ),
                )),
                None => {
                    seen.insert(&block.id, reqfile);
                }
            }
        }
    }
    errors
}

/// The effective requirement of every block, in Reqfile order. `sources`
/// holds every git source the use blocks name; `asset_files` every file in
/// a `.reqfile/` folder of the repository.
pub fn resolve(
    reqfiles: &[Reqfile],
    sources: &[Source],
    asset_files: &BTreeSet<String>,
) -> Result<Vec<Effective>, Vec<ConfigError>> {
    let mut effective = Vec::new();
    let mut errors = Vec::new();
    for reqfile in reqfiles {
        for block in &reqfile.blocks {
            match resolve_block(reqfiles, reqfile, block, sources, asset_files) {
                Ok(e) => effective.push(e),
                Err(e) => errors.push(e),
            }
        }
    }
    if errors.is_empty() {
        Ok(effective)
    } else {
        Err(errors)
    }
}

fn resolve_block(
    reqfiles: &[Reqfile],
    reqfile: &Reqfile,
    block: &Block,
    sources: &[Source],
    asset_files: &BTreeSet<String>,
) -> Result<Effective, ConfigError> {
    let error = |message: String| ConfigError::at(&reqfile.path, block.line, message);
    let own_assets = assets_dir(&reqfile.dir, &block.id);
    let location = match &block.origin {
        Origin::Definition { must, reference } => {
            let checks = block.checks.as_ref().expect("a definition has checks");
            return Ok(Effective {
                reqfile: reqfile.path.clone(),
                dir: reqfile.dir.clone(),
                line: block.line,
                id: block.id.clone(),
                kind: block.kind,
                must: must.clone(),
                why: block.why.clone().expect("a definition has a why"),
                who: block.who.clone(),
                reference: reference.clone(),
                checks: checks.list.clone(),
                checks_source: checks.source.clone(),
                imported: None,
                assets: Assets::Repo(own_assets),
            });
        }
        Origin::Use(location) => location,
    };
    let (found, commit, definition_assets) = match location {
        Location::Local(path) => {
            let folder = paths::normalize(&paths::join(&reqfile.dir, path)).ok_or_else(|| {
                error(format!(
                    "`use: {path}` points above the repository root; a folder source must be inside the repository"
                ))
            })?;
            let shown = paths::display_dir(&folder).to_string();
            let (source_reqfile, definition) =
                find_definition(reqfiles, &folder, &shown, &block.id)
                    .map_err(|e| error(format!("`use: {path}`: {e}")))?;
            let assets = Assets::Repo(assets_dir(&source_reqfile.dir, &block.id));
            ((source_reqfile, definition), None, assets)
        }
        Location::Git { repo, reference } => {
            let source = sources
                .iter()
                .find(|s| &s.repo == repo && &s.reference == reference)
                .expect("every git source was loaded");
            let describe = location.describe();
            let shown = format!("the root of {repo} at commit {}", source.commit);
            let (source_reqfile, definition) =
                find_definition(&source.reqfiles, "", &shown, &block.id)
                    .map_err(|e| error(format!("`use: {describe}`: {e}")))?;
            let assets =
                Assets::External(source.root.join(assets_dir(&source_reqfile.dir, &block.id)));
            (
                (source_reqfile, definition),
                Some(source.commit.clone()),
                assets,
            )
        }
    };
    let (source_reqfile, definition) = found;
    let Origin::Definition { must, reference } = &definition.origin else {
        unreachable!("only definitions are found");
    };
    if definition.kind != block.kind {
        return Err(error(format!(
            "requirement {} is listed under `{}`, but its definition in {}:{} is a {} requirement; list it under `{}`",
            block.id,
            block.kind.as_str(),
            source_reqfile.path,
            definition.line,
            definition.kind.as_str(),
            definition.kind.as_str()
        )));
    }
    let definition_checks = definition.checks.as_ref().expect("a definition has checks");
    let (checks, assets) = match &block.checks {
        Some(local) => (local, Assets::Repo(own_assets)),
        None => {
            let prefix = format!("{own_assets}/");
            let examples = format!("{own_assets}/examples/");
            if let Some(file) = asset_files
                .iter()
                .find(|f| f.starts_with(&prefix) && !f.starts_with(&examples))
            {
                return Err(error(format!(
                    "requirement {} takes its checks from its definition, whose files are in its own .reqfile/{}/, so {own_assets}/ may only hold examples/; found {file}. Set `checks` here to use local files, or move the file",
                    block.id, block.id
                )));
            }
            (definition_checks, definition_assets.clone())
        }
    };
    Ok(Effective {
        reqfile: reqfile.path.clone(),
        dir: reqfile.dir.clone(),
        line: block.line,
        id: block.id.clone(),
        kind: block.kind,
        must: must.clone(),
        why: block
            .why
            .clone()
            .or_else(|| definition.why.clone())
            .expect("a definition has a why"),
        who: block.who.clone().or_else(|| definition.who.clone()),
        reference: reference.clone(),
        checks: checks.list.clone(),
        checks_source: checks.source.clone(),
        imported: Some(Imported {
            location: location.clone(),
            definition: format!("{}:{}", source_reqfile.path, definition.line),
            commit,
            checks_inherited: block.checks.is_none(),
            definition_assets,
        }),
        assets,
    })
}

/// The unique definition of `id` among the Reqfiles at or under `folder`,
/// shown to people as `shown`. Use blocks are never resolution targets, so
/// chains cannot form.
fn find_definition<'r>(
    reqfiles: &'r [Reqfile],
    folder: &str,
    shown: &str,
    id: &str,
) -> Result<(&'r Reqfile, &'r Block), String> {
    let found: Vec<(&Reqfile, &Block)> = reqfiles
        .iter()
        .filter(|r| paths::contains_dir(folder, &r.dir))
        .flat_map(|r| r.blocks.iter().map(move |b| (r, b)))
        .filter(|(_, b)| b.id == id)
        .collect();
    let definitions: Vec<&(&Reqfile, &Block)> = found
        .iter()
        .filter(|(_, b)| matches!(b.origin, Origin::Definition { .. }))
        .collect();
    match definitions.as_slice() {
        [one] => Ok(**one),
        [] => match found.first() {
            Some((reqfile, block)) => Err(format!(
                "{id} in {}:{} is a use block, not a definition; point `use` to the folder that defines {id}",
                reqfile.path, block.line
            )),
            None => Err(format!("no Reqfile at or under {shown} defines {id}")),
        },
        [first, second, ..] => Err(format!(
            "{id} is defined twice, in {} and {}",
            first.0.path, second.0.path
        )),
    }
}

/// The blocks that apply to files of folder `dir`: for each id, the block
/// in the nearest Reqfile at or above it. Shallowest Reqfile first, then in
/// Reqfile order.
pub fn applicable<'e>(blocks: &'e [Effective], dir: &str) -> Vec<&'e Effective> {
    let mut nearest: Vec<&Effective> = Vec::new();
    for block in blocks.iter().filter(|b| paths::contains_dir(&b.dir, dir)) {
        match nearest.iter_mut().find(|b| b.id == block.id) {
            Some(existing) if depth(&block.dir) > depth(&existing.dir) => *existing = block,
            Some(_) => {}
            None => nearest.push(block),
        }
    }
    nearest.sort_by_key(|b| depth(&b.dir));
    nearest
}

fn depth(dir: &str) -> usize {
    super::config::depth(dir)
}

/// The files a block applies to: those under its folder whose nearest
/// block with its id is this one.
pub struct Scope {
    dir: String,
    /// Folders under `dir` holding a nearer block with the same id.
    shadowed: Vec<String>,
}

impl Scope {
    pub fn of(blocks: &[Effective], index: usize) -> Self {
        let block = &blocks[index];
        let shadowed = blocks
            .iter()
            .filter(|b| b.id == block.id && b.dir != block.dir)
            .filter(|b| paths::contains_dir(&block.dir, &b.dir))
            .map(|b| b.dir.clone())
            .collect();
        Self {
            dir: block.dir.clone(),
            shadowed,
        }
    }

    /// `path` relative to the block's folder, if the block applies to it.
    pub fn relative<'p>(&self, path: &'p str) -> Option<&'p str> {
        let relative = paths::relative_to(&self.dir, path)?;
        let dir = paths::parent(path);
        (!self
            .shadowed
            .iter()
            .any(|shadow| paths::contains_dir(shadow, dir)))
        .then_some(relative)
    }
}

/// The index of the nearest block with `id` at or above folder `dir`.
pub fn nearest(blocks: &[Effective], dir: &str, id: &str) -> Option<usize> {
    blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.id == id && paths::contains_dir(&b.dir, dir))
        .max_by_key(|(_, b)| depth(&b.dir))
        .map(|(i, _)| i)
}

/// Identifies what a block checks and how: its place, its effective
/// requirement, the files of its checks (`assets`, a fingerprint of their
/// content), its settings and the commit of a git source. A file whose
/// block's fingerprint differs from the base is checked again.
pub fn fingerprint(block: &Effective, assets: &str, settings: &str) -> String {
    let imported = block.imported.as_ref().map(|i| {
        format!(
            "{}\n{}\n{:?}\n{}",
            i.location.describe(),
            i.definition,
            i.commit,
            i.checks_inherited
        )
    });
    super::runlog::fingerprint(&format!(
        "{}\n{}\n{}\n{}\n{}\n{:?}\n{:?}\n{}\n{:?}\n{assets}\n{settings}",
        block.reqfile,
        block.id,
        block.kind.as_str(),
        block.must,
        block.why,
        block.who,
        block.reference,
        block.checks_source,
        imported,
    ))
}
