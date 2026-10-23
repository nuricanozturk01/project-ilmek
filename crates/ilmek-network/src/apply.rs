//! Applies a [`Plan`] with a snapshot that can undo it. Every change is recorded in the
//! snapshot *before* it is made, so a crash half-way can still be rolled back.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::error::ApplyError;
use crate::plan::{FileWrite, Plan, Step};
use crate::render::PASSPHRASE;
use crate::system::{Probe, Runner};

/// The machine being changed. `root` is `/` in production and a scratch directory in tests.
#[derive(Clone, Copy)]
pub struct Environment<'a> {
    pub root: &'a Path,
    pub probe: &'a dyn Probe,
    pub runner: &'a dyn Runner,
}

impl std::fmt::Debug for Environment<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl Environment<'_> {
    fn real(&self, path: &Path) -> PathBuf {
        self.root.join(path.strip_prefix("/").unwrap_or(path))
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    /// Allow touching the interface that carries the default route or the current login.
    pub allow_management_interface: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub snapshot_id: String,
    pub snapshot_dir: PathBuf,
}

/// How far a snapshot got. `Applying` after a crash means the change may be partly in place and
/// should be rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Applying,
    Applied,
    RolledBack,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Applying => "applying (interrupted or in progress)",
            Self::Applied => "applied",
            Self::RolledBack => "rolled back",
        })
    }
}

/// Summary of one recorded snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: String,
    pub node: String,
    pub created_unix: u64,
    pub phase: Phase,
}

/// Lists snapshots, oldest first. Read-only; damaged or foreign entries are skipped.
#[must_use]
pub fn snapshots(snapshot_root: &Path, root: &Path) -> Vec<SnapshotInfo> {
    let dir = root.join(snapshot_root.strip_prefix("/").unwrap_or(snapshot_root));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<SnapshotInfo> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| load(&entry.path())?.ok())
        .map(|m| SnapshotInfo {
            id: m.id,
            node: m.node,
            created_unix: m.created_unix,
            phase: m.phase,
        })
        .collect();
    found.sort_by(|a, b| (a.created_unix, &a.id).cmp(&(b.created_unix, &b.id)));
    found
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Done {
    /// A file this run wrote; `backup` names the saved original if there was one.
    File {
        path: PathBuf,
        backup: Option<String>,
    },
    Directory(PathBuf),
    Undo(Vec<Command>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    id: String,
    node: String,
    created_unix: u64,
    phase: Phase,
    done: Vec<Done>,
    after_rollback: Vec<Command>,
}

pub fn apply(
    plan: &Plan,
    snapshot_root: &Path,
    env: Environment<'_>,
    options: &ApplyOptions,
) -> Result<Applied, ApplyError> {
    if !plan.missing.is_empty() {
        return Err(ApplyError::MissingPrerequisites(plan.missing.clone()));
    }
    if !env.probe.is_root() {
        return Err(ApplyError::NotRoot);
    }
    if !options.allow_management_interface {
        let management = env.probe.management_interfaces();
        if let Some(hit) = plan.interfaces.iter().find(|i| management.contains(i)) {
            return Err(ApplyError::ManagementInterface(hit.clone()));
        }
    }
    let secrets = read_secrets(plan, env)?;

    let (id, dir) = create_snapshot_dir(&env.real(snapshot_root), &plan.node)?;
    let mut manifest = Manifest {
        id: id.clone(),
        node: plan.node.clone(),
        created_unix: unix_now(),
        phase: Phase::Applying,
        done: Vec::new(),
        after_rollback: plan.after_rollback.clone(),
    };
    save(&dir, &manifest)?;
    capture_state(plan, env, &dir);

    for (index, step) in plan.steps.iter().enumerate() {
        if let Err(cause) = run_step(step, index, &secrets, env, &dir, &mut manifest) {
            let rollback_problems = rollback_manifest(&dir, &mut manifest, env);
            return Err(ApplyError::Failed {
                cause: Box::new(cause),
                snapshot: id,
                rollback_problems,
            });
        }
    }
    manifest.phase = Phase::Applied;
    save(&dir, &manifest)?;
    Ok(Applied {
        snapshot_id: id,
        snapshot_dir: dir,
    })
}

/// Undoes a snapshot. Keeps going after individual failures so as much as possible is restored.
pub fn rollback(
    snapshot_root: &Path,
    snapshot_id: &str,
    env: Environment<'_>,
) -> Result<(), ApplyError> {
    if !env.probe.is_root() {
        return Err(ApplyError::NotRoot);
    }
    if snapshot_id.is_empty()
        || !snapshot_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || snapshot_id.starts_with('.')
    {
        return Err(ApplyError::NoSnapshot(snapshot_id.to_owned()));
    }
    let dir = env.real(snapshot_root).join(snapshot_id);
    let mut manifest =
        load(&dir).ok_or_else(|| ApplyError::NoSnapshot(snapshot_id.to_owned()))??;
    if manifest.phase == Phase::RolledBack {
        return Err(ApplyError::AlreadyRolledBack(snapshot_id.to_owned()));
    }
    let problems = rollback_manifest(&dir, &mut manifest, env);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(ApplyError::RollbackIncomplete(problems))
    }
}

fn run_step(
    step: &Step,
    index: usize,
    secrets: &[(PathBuf, String)],
    env: Environment<'_>,
    dir: &Path,
    manifest: &mut Manifest,
) -> Result<(), ApplyError> {
    match step {
        Step::Write(file) => write_step(file, index, secrets, env, dir, manifest),
        Step::Run { command, undo, .. } => {
            manifest.done.push(Done::Undo(undo.clone()));
            save(dir, manifest)?;
            env.runner
                .run(command)
                .map(drop)
                .map_err(|reason| ApplyError::Command {
                    command: command.to_string(),
                    reason,
                })
        }
    }
}

fn write_step(
    file: &FileWrite,
    index: usize,
    secrets: &[(PathBuf, String)],
    env: Environment<'_>,
    dir: &Path,
    manifest: &mut Manifest,
) -> Result<(), ApplyError> {
    let target = env.real(&file.path);
    let content = match &file.secret_file {
        Some(secret) => {
            let value = secrets
                .iter()
                .find(|(path, _)| path == secret)
                .map_or("", |(_, value)| value.as_str());
            file.content.replace(PASSPHRASE, value)
        }
        None => file.content.clone(),
    };

    for missing in missing_ancestors(&target) {
        manifest.done.push(Done::Directory(missing));
    }
    let backup = if target.exists() {
        let name = format!("{index}.orig");
        let backups = dir.join("files");
        std::fs::create_dir_all(&backups).map_err(|e| ApplyError::io("snapshot files", e))?;
        std::fs::copy(&target, backups.join(&name))
            .map_err(|e| ApplyError::io(format!("back up {}", target.display()), e))?;
        Some(name)
    } else {
        None
    };
    manifest.done.push(Done::File {
        path: file.path.clone(),
        backup,
    });
    save(dir, manifest)?;

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApplyError::io(format!("create {}", parent.display()), e))?;
    }
    write_file(&target, file.mode, content.as_bytes())
        .map_err(|e| ApplyError::io(format!("write {}", target.display()), e))
}

fn missing_ancestors(target: &Path) -> Vec<PathBuf> {
    let mut missing: Vec<PathBuf> = target
        .ancestors()
        .skip(1)
        .take_while(|p| !p.exists())
        .map(Path::to_owned)
        .collect();
    missing.reverse();
    missing
}

fn read_secrets(plan: &Plan, env: Environment<'_>) -> Result<Vec<(PathBuf, String)>, ApplyError> {
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    for step in &plan.steps {
        let Step::Write(FileWrite {
            secret_file: Some(path),
            ..
        }) = step
        else {
            continue;
        };
        if found.iter().any(|(p, _)| p == path) {
            continue;
        }
        let problem = |reason: &str| ApplyError::Passphrase {
            path: path.clone(),
            reason: reason.to_owned(),
        };
        let text = std::fs::read_to_string(env.real(path)).map_err(|e| problem(&e.to_string()))?;
        let passphrase = text
            .lines()
            .next()
            .unwrap_or_default()
            .trim_end_matches('\r');
        let printable = passphrase
            .bytes()
            .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\');
        if !(8..=63).contains(&passphrase.len()) || !printable {
            return Err(problem(
                "the first line must be 8–63 printable ASCII characters without quotes or backslashes",
            ));
        }
        found.push((path.clone(), passphrase.to_owned()));
    }
    Ok(found)
}

fn rollback_manifest(dir: &Path, manifest: &mut Manifest, env: Environment<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    for done in manifest.done.iter().rev() {
        match done {
            Done::Undo(commands) => {
                for command in commands {
                    if let Err(reason) = env.runner.run(command) {
                        problems.push(format!("`{command}`: {reason}"));
                    }
                }
            }
            Done::File { path, backup } => {
                if let Err(problem) = restore_file(env, dir, path, backup.as_deref()) {
                    problems.push(problem);
                }
            }
            Done::Directory(path) => match std::fs::remove_dir(env.real(path)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => problems.push(format!("remove directory {}: {e}", path.display())),
            },
        }
    }
    for command in &manifest.after_rollback {
        if let Err(reason) = env.runner.run(command) {
            problems.push(format!("`{command}`: {reason}"));
        }
    }
    if problems.is_empty() {
        manifest.phase = Phase::RolledBack;
        if let Err(e) = save(dir, manifest) {
            problems.push(e.to_string());
        }
    }
    problems
}

fn restore_file(
    env: Environment<'_>,
    dir: &Path,
    path: &Path,
    backup: Option<&str>,
) -> Result<(), String> {
    let target = env.real(path);
    let result = match backup {
        Some(name) => std::fs::copy(dir.join("files").join(name), &target).map(drop),
        None => std::fs::remove_file(&target),
    };
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound || backup.is_some() => {
            Err(format!("restore {}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

fn capture_state(plan: &Plan, env: Environment<'_>, dir: &Path) {
    let state = dir.join("state");
    if std::fs::create_dir_all(&state).is_err() {
        return;
    }
    for (name, command) in &plan.capture {
        let text = env
            .runner
            .run(command)
            .unwrap_or_else(|reason| format!("(could not capture: {reason})"));
        let _ = std::fs::write(state.join(format!("{name}.txt")), text);
    }
    if let Some(value) = env.probe.ip_forward() {
        let _ = std::fs::write(state.join("ip_forward.txt"), value);
    }
}

fn create_snapshot_dir(root: &Path, node: &str) -> Result<(String, PathBuf), ApplyError> {
    std::fs::create_dir_all(root).map_err(|e| ApplyError::io("snapshot directory", e))?;
    let stamp = unix_now();
    for attempt in 0..100 {
        let id = if attempt == 0 {
            format!("{node}-{stamp}")
        } else {
            format!("{node}-{stamp}-{attempt}")
        };
        let dir = root.join(&id);
        match std::fs::create_dir(&dir) {
            Ok(()) => {
                restrict(&dir).map_err(|e| ApplyError::io("snapshot directory", e))?;
                return Ok((id, dir));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(ApplyError::io("snapshot directory", e)),
        }
    }
    Err(ApplyError::Manifest(
        "could not allocate a snapshot id".into(),
    ))
}

fn restrict(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let _ = dir;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn save(dir: &Path, manifest: &Manifest) -> Result<(), ApplyError> {
    let bytes =
        serde_json::to_vec_pretty(manifest).map_err(|e| ApplyError::Manifest(e.to_string()))?;
    let staging = dir.join("manifest.json.new");
    write_file(&staging, 0o600, &bytes).map_err(|e| ApplyError::io("snapshot manifest", e))?;
    std::fs::rename(&staging, dir.join("manifest.json"))
        .map_err(|e| ApplyError::io("snapshot manifest", e))
}

/// `None` when the snapshot directory does not exist.
fn load(dir: &Path) -> Option<Result<Manifest, ApplyError>> {
    let bytes = std::fs::read(dir.join("manifest.json")).ok()?;
    Some(serde_json::from_slice(&bytes).map_err(|e| ApplyError::Manifest(e.to_string())))
}

fn write_file(path: &Path, mode: u32, bytes: &[u8]) -> std::io::Result<()> {
    let staging = path.with_extension("ilmek-new");
    let _ = std::fs::remove_file(&staging);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = options.open(&staging)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&staging, path)
}
