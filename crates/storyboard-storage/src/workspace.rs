use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use storyboard_domain::{ProjectId, VersionNumber};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("original {0} already exists with different content — originals are immutable")]
    OriginalConflict(String),
    #[error("version v{version} of project {project} already exists — versions are immutable")]
    VersionConflict { project: String, version: VersionNumber },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("corrupt original {0}: sha256 mismatch")]
    CorruptOriginal(String),
}

/// The on-disk workspace (plan §19). All writes are atomic: temp file in the
/// target directory, then rename. Originals and version snapshots are
/// immutable after creation.
///
/// ```text
/// workspace/
/// ├─ templates/originals/<sha256>.json
/// ├─ projects/<project_id>/versions/v0001/project.json
/// │                        └─ diffs/v0001-v0002.json
/// │                        └─ exports/
/// ├─ prompts/<preset_version>/...
/// ├─ runs/<run_id>/manifest.json
/// ├─ database/storyboard.db
/// └─ logs/
/// ```
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

fn now_compact() -> String {
    chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string()
}

impl Workspace {
    /// Create the directory tree if needed and return a handle.
    pub fn init(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = root.as_ref().to_path_buf();
        for d in [
            "templates/originals",
            "projects",
            "prompts",
            "runs",
            "database",
            "logs",
        ] {
            fs::create_dir_all(root.join(d))?;
        }
        Ok(Self { root })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = root.as_ref().to_path_buf();
        if !root.join("database").is_dir() {
            return Err(WorkspaceError::NotFound(format!(
                "workspace at {} is not initialized",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("database/storyboard.db")
    }

    // ---- immutable template originals ------------------------------------

    pub fn original_path(&self, sha256: &str) -> PathBuf {
        self.root.join(format!("templates/originals/{sha256}.json"))
    }

    /// Store an original once. Re-storing identical bytes is a no-op;
    /// different bytes under the same hash is refused.
    pub fn store_original(&self, sha256: &str, bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
        let path = self.original_path(sha256);
        if path.exists() {
            let existing = fs::read(&path)?;
            if existing == bytes {
                return Ok(path);
            }
            return Err(WorkspaceError::OriginalConflict(sha256.into()));
        }
        atomic_write(&path, bytes)
    }

    pub fn read_original(&self, sha256: &str) -> Result<Vec<u8>, WorkspaceError> {
        let path = self.original_path(sha256);
        if !path.exists() {
            return Err(WorkspaceError::NotFound(format!("original {sha256}")));
        }
        let bytes = fs::read(&path)?;
        if storyboard_domain::content_hash(&bytes) != sha256 {
            return Err(WorkspaceError::CorruptOriginal(sha256.into()));
        }
        Ok(bytes)
    }

    pub fn has_original(&self, sha256: &str) -> bool {
        self.original_path(sha256).exists()
    }

    // ---- project versions -------------------------------------------------

    pub fn project_dir(&self, pid: &ProjectId) -> PathBuf {
        self.root.join("projects").join(pid.0.simple().to_string())
    }

    pub fn version_path(&self, pid: &ProjectId, version: VersionNumber) -> PathBuf {
        self.project_dir(pid).join(format!("versions/v{version:04}")).join("project.json")
    }

    pub fn diff_path(&self, pid: &ProjectId, from: VersionNumber, to: VersionNumber) -> PathBuf {
        self.project_dir(pid).join("diffs").join(format!("v{from:04}-v{to:04}.json"))
    }

    pub fn exports_dir(&self, pid: &ProjectId) -> PathBuf {
        self.project_dir(pid).join("exports")
    }

    /// Persist a project version snapshot immutably.
    pub fn write_project_version(
        &self,
        pid: &ProjectId,
        version: VersionNumber,
        bytes: &[u8],
    ) -> Result<PathBuf, WorkspaceError> {
        let path = self.version_path(pid, version);
        if path.exists() {
            return Err(WorkspaceError::VersionConflict {
                project: pid.to_string(),
                version,
            });
        }
        atomic_write(&path, bytes)
    }

    pub fn read_project_version(
        &self,
        pid: &ProjectId,
        version: VersionNumber,
    ) -> Result<Vec<u8>, WorkspaceError> {
        let path = self.version_path(pid, version);
        if !path.exists() {
            return Err(WorkspaceError::NotFound(format!(
                "project {pid} v{version}"
            )));
        }
        Ok(fs::read(&path)?)
    }

    pub fn write_diff(
        &self,
        pid: &ProjectId,
        from: VersionNumber,
        to: VersionNumber,
        bytes: &[u8],
    ) -> Result<PathBuf, WorkspaceError> {
        atomic_write(&self.diff_path(pid, from, to), bytes)
    }

    pub fn write_export(
        &self,
        pid: &ProjectId,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, WorkspaceError> {
        let dir = self.exports_dir(pid);
        fs::create_dir_all(&dir)?;
        atomic_write(&dir.join(file_name), bytes)
    }

    pub fn write_manifest(&self, run_id: &str, bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
        let dir = self.root.join("runs").join(run_id);
        fs::create_dir_all(&dir)?;
        atomic_write(&dir.join("manifest.json"), bytes)
    }
}

/// Atomic write: temp file in the same directory, flush, rename (plan §22:
/// 所有 Commit 原子化).
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<PathBuf, WorkspaceError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", now_compact()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(path.to_path_buf())
}
