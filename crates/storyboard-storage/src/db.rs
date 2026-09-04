use rusqlite::Connection;
use storyboard_domain::{
    AuditEvent, ProjectId, ProjectState, ProjectStatus, SceneAliasTable, TemplateMetadata,
    VersionNumber,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

const MIGRATIONS: &[(&str, &str)] = &[("0001_init", include_str!("../../../migrations/0001_init.sql"))];

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("workspace: {0}")]
    Workspace(#[from] crate::WorkspaceError),
    #[error("{0}")]
    Locked(String),
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRevisionRow {
    pub template_id: String,
    pub revision_id: String,
    pub sha256: String,
    pub file_path: String,
    pub schema_fingerprint: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub title: String,
    pub source_template_id: String,
    pub source_template_revision_id: String,
    pub current_version: VersionNumber,
    pub status: ProjectStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProjectRow> for ProjectState {
    fn from(r: ProjectRow) -> Self {
        ProjectState {
            project_id: ProjectId(r.id.parse().unwrap_or_default()),
            title: r.title,
            status: r.status,
            current_version: r.current_version,
            source_template_id: storyboard_domain::TemplateId::new(r.source_template_id),
            source_revision_id: r.source_template_revision_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectVersionRow {
    pub project_id: String,
    pub version_number: VersionNumber,
    pub parent_version: Option<VersionNumber>,
    pub snapshot_path: String,
    pub diff_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRow {
    pub id: i64,
    pub project_id: String,
    pub base_version: VersionNumber,
    pub proposal_json: String,
    pub validation_json: Option<String>,
    pub status: String,
    pub run_id: Option<String>,
    pub created_at: String,
}

/// Thread-safe SQLite handle. Interior mutability via Mutex; the desktop app
/// is single-process and low write concurrency.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::Locked("db mutex poisoned".into()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at TEXT NOT NULL);",
        )?;
        for (name, sql) in MIGRATIONS {
            let done: bool = conn
                .query_row("SELECT 1 FROM schema_migrations WHERE name = ?1", [name], |_| Ok(true))
                .unwrap_or(false);
            if !done {
                conn.execute_batch(sql)?;
                conn.execute(
                    "INSERT INTO schema_migrations (name, applied_at) VALUES (?1, ?2)",
                    rusqlite::params![name, now_iso()],
                )?;
            }
        }
        Ok(())
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, DbError>) -> Result<T, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::Locked("db mutex poisoned".into()))?;
        f(&conn)
    }

    // ---- templates ---------------------------------------------------------

    /// Insert or update a template + revision + rebuilt metadata + tags.
    /// Re-importing identical content is a no-op returning the same revision.
    pub fn upsert_template(&self, meta: &TemplateMetadata) -> Result<TemplateRevisionRow, DbError> {
        self.with_conn(|conn| {
            let now = now_iso();
            let existing_rev: Option<String> = conn
                .query_row(
                    "SELECT id FROM template_revisions WHERE sha256 = ?1",
                    [&meta.sha256],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            if let Some(rev) = existing_rev {
                return Ok(TemplateRevisionRow {
                    template_id: meta.template_id.clone(),
                    revision_id: rev,
                    sha256: meta.sha256.clone(),
                    file_path: format!("templates/originals/{}.json", meta.sha256),
                    schema_fingerprint: meta.schema_fingerprint.clone(),
                    imported_at: now,
                });
            }
            conn.execute(
                "INSERT INTO templates (id, title, current_revision_id, created_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET current_revision_id = excluded.current_revision_id",
                rusqlite::params![meta.template_id, meta.title, meta.revision_id, now],
            )?;
            let file_path = format!("templates/originals/{}.json", meta.sha256);
            conn.execute(
                "INSERT INTO template_revisions (id, template_id, file_path, sha256, schema_fingerprint, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    meta.revision_id,
                    meta.template_id,
                    file_path,
                    meta.sha256,
                    meta.schema_fingerprint,
                    now
                ],
            )?;
            conn.execute(
                "INSERT INTO template_metadata
                   (revision_id, scene_family, exact_scene, time_tags_json, panel_count,
                    total_role_count, female_lead_count, male_lead_count, pace, narrative_type, metadata_json)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                rusqlite::params![
                    meta.revision_id,
                    meta.scene_family,
                    meta.exact_scene,
                    serde_json::to_string(&meta.time_tags)?,
                    meta.panel_count as i64,
                    meta.total_role_count as i64,
                    meta.female_lead_count.map(|v| v as i64),
                    meta.male_lead_count.map(|v| v as i64),
                    meta.pace,
                    meta.narrative_type,
                    serde_json::to_string(meta)?
                ],
            )?;
            let mut tag_stmt = conn.prepare(
                "INSERT OR REPLACE INTO template_tags (revision_id, kind, value, weight) VALUES (?1,?2,?3,?4)",
            )?;
            let mut put = |kind: &str, values: &[String]| -> Result<(), DbError> {
                for v in values {
                    tag_stmt.execute(rusqlite::params![meta.revision_id, kind, v, 1.0])?;
                }
                Ok(())
            };
            put("scene", &meta.scene_tags)?;
            put("location", &meta.location_tags)?;
            put("time", &meta.time_tags)?;
            put("environment", &meta.environment_tags)?;
            put("camera", &meta.camera_profile)?;
            put("composition", &meta.composition_profile)?;
            put("interaction", &meta.interaction_profile)?;
            put("prop", &meta.important_props)?;
            put("keyword", &meta.keywords)?;
            put("anchor", &meta.character_anchor_variants)?;
            Ok(TemplateRevisionRow {
                template_id: meta.template_id.clone(),
                revision_id: meta.revision_id.clone(),
                sha256: meta.sha256.clone(),
                file_path,
                schema_fingerprint: meta.schema_fingerprint.clone(),
                imported_at: now,
            })
        })
    }

    pub fn list_template_metadata(&self) -> Result<Vec<TemplateMetadata>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT metadata_json FROM template_metadata")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok(out)
        })
    }

    pub fn get_template_metadata(&self, template_id: &str) -> Result<TemplateMetadata, DbError> {
        self.with_conn(|conn| {
            let json: String = conn.query_row(
                "SELECT m.metadata_json FROM template_metadata m
                 JOIN template_revisions r ON r.id = m.revision_id
                 JOIN templates t ON t.current_revision_id = r.id
                 WHERE t.id = ?1",
                [template_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(format!("template {template_id}")),
                other => other.into(),
            })?;
            Ok(serde_json::from_str(&json)?)
        })
    }

    pub fn current_revision_sha(&self, template_id: &str) -> Result<String, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT r.sha256 FROM template_revisions r
                 JOIN templates t ON t.current_revision_id = r.id WHERE t.id = ?1",
                [template_id],
                |r| r.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(format!("template {template_id}")),
                other => other.into(),
            })
        })
    }

    // ---- projects ----------------------------------------------------------

    pub fn create_project(&self, state: &ProjectState) -> Result<(), DbError> {
        self.with_conn(|conn| {
            let n = conn.execute(
                "INSERT INTO projects (id, title, source_template_id, source_template_revision_id,
                                       current_version, status, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
                rusqlite::params![
                    state.project_id.to_string(),
                    state.title,
                    state.source_template_id.as_str(),
                    state.source_revision_id,
                    state.current_version as i64,
                    serde_json::to_string(&state.status)?.trim_matches('"'),
                    now_iso()
                ],
            )?;
            if n == 0 {
                return Err(DbError::Conflict(format!("project {} exists", state.project_id)));
            }
            Ok(())
        })
    }

    pub fn get_project(&self, pid: &ProjectId) -> Result<ProjectRow, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, title, source_template_id, source_template_revision_id,
                        current_version, status, created_at, updated_at
                 FROM projects WHERE id = ?1",
                [pid.to_string()],
                |r| {
                    Ok(ProjectRow {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        source_template_id: r.get(2)?,
                        source_template_revision_id: r.get(3)?,
                        current_version: r.get::<_, i64>(4)? as u64,
                        status: serde_json::from_str(&format!("\"{}\"", r.get::<_, String>(5)?))
                            .unwrap_or(ProjectStatus::Draft),
                        created_at: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(format!("project {pid}")),
                other => other.into(),
            })
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRow>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, source_template_id, source_template_revision_id,
                        current_version, status, created_at, updated_at
                 FROM projects ORDER BY updated_at DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(ProjectRow {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    source_template_id: r.get(2)?,
                    source_template_revision_id: r.get(3)?,
                    current_version: r.get::<_, i64>(4)? as u64,
                    status: serde_json::from_str(&format!("\"{}\"", r.get::<_, String>(5)?))
                        .unwrap_or(ProjectStatus::Draft),
                    created_at: r.get(6)?,
                    updated_at: r.get(7)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn update_project_status(&self, pid: &ProjectId, status: ProjectStatus, current_version: VersionNumber) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE projects SET status = ?1, current_version = ?2, updated_at = ?3 WHERE id = ?4",
                rusqlite::params![
                    serde_json::to_string(&status)?.trim_matches('"'),
                    current_version as i64,
                    now_iso(),
                    pid.to_string()
                ],
            )?;
            Ok(())
        })
    }

    pub fn insert_version(&self, row: &ProjectVersionRow) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO project_versions (project_id, version_number, parent_version, snapshot_path, diff_path, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                rusqlite::params![
                    row.project_id,
                    row.version_number as i64,
                    row.parent_version.map(|v| v as i64),
                    row.snapshot_path,
                    row.diff_path,
                    row.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn list_versions(&self, pid: &ProjectId) -> Result<Vec<ProjectVersionRow>, DbError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT project_id, version_number, parent_version, snapshot_path, diff_path, created_at
                 FROM project_versions WHERE project_id = ?1 ORDER BY version_number",
            )?;
            let rows = stmt.query_map([pid.to_string()], |r| {
                Ok(ProjectVersionRow {
                    project_id: r.get(0)?,
                    version_number: r.get::<_, i64>(1)? as u64,
                    parent_version: r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                    snapshot_path: r.get(3)?,
                    diff_path: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    // ---- patches -----------------------------------------------------------

    pub fn insert_patch(
        &self,
        pid: &ProjectId,
        base_version: VersionNumber,
        proposal_json: &str,
        run_id: Option<&str>,
    ) -> Result<i64, DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO patches (project_id, base_version, proposal_json, status, run_id, created_at, updated_at)
                 VALUES (?1,?2,?3,'proposed',?4,?5,?5)",
                rusqlite::params![pid.to_string(), base_version as i64, proposal_json, run_id, now_iso()],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn update_patch(&self, patch_id: i64, status: &str, validation_json: Option<&str>) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE patches SET status = ?1, validation_json = COALESCE(?2, validation_json), updated_at = ?3 WHERE id = ?4",
                rusqlite::params![status, validation_json, now_iso(), patch_id],
            )?;
            Ok(())
        })
    }

    pub fn latest_patch(&self, pid: &ProjectId) -> Result<PatchRow, DbError> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, project_id, base_version, proposal_json, validation_json, status, run_id, created_at
                 FROM patches WHERE project_id = ?1 ORDER BY id DESC LIMIT 1",
                [pid.to_string()],
                |r| {
                    Ok(PatchRow {
                        id: r.get(0)?,
                        project_id: r.get(1)?,
                        base_version: r.get::<_, i64>(2)? as u64,
                        proposal_json: r.get(3)?,
                        validation_json: r.get(4)?,
                        status: r.get(5)?,
                        run_id: r.get(6)?,
                        created_at: r.get(7)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(format!("no patch for {pid}")),
                other => other.into(),
            })
        })
    }

    // ---- agent persistence (F07) -------------------------------------------

    pub fn insert_agent_thread(
        &self,
        id: &str,
        project_id: Option<&str>,
        provider_id: &str,
        model: &str,
    ) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO agent_threads (id, project_id, provider_id, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, project_id, provider_id, model, now_iso()],
            )?;
            Ok(())
        })
    }

    pub fn insert_agent_run(&self, m: &storyboard_domain::AgentRunManifest, thread_id: &str) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO agent_runs
                   (id, thread_id, provider_id, model, prompt_preset_version, core_contract_hash,
                    tool_registry_version, template_revision_id, base_project_version_id,
                    sampling_json, manifest_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                rusqlite::params![
                    m.run_id,
                    thread_id,
                    m.provider_id,
                    m.model,
                    m.prompt_preset_version,
                    m.core_contract_hash,
                    m.tool_registry_version,
                    m.primary_template_revision,
                    m.base_project_version,
                    serde_json::to_string(&m.sampling)?,
                    serde_json::to_string(m)?,
                    m.created_at
                ],
            )?;
            Ok(())
        })
    }

    /// Append one agent event; seq is per-thread monotonic. Returns the seq.
    pub fn insert_agent_event(&self, thread_id: &str, type_name: &str, payload_json: &str) -> Result<u64, DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO agent_events (thread_id, seq, type, payload_json, created_at)
                 VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM agent_events WHERE thread_id = ?1), ?2, ?3, ?4)",
                rusqlite::params![thread_id, type_name, payload_json, now_iso()],
            )?;
            Ok(conn.last_insert_rowid() as u64)
        })
    }

    pub fn list_agent_events(&self, thread_id: &str) -> Result<Vec<(u64, String)>, DbError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT seq, type FROM agent_events WHERE thread_id = ?1 ORDER BY seq")?;
            let rows = stmt.query_map([thread_id], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn has_agent_run(&self, run_id: &str) -> Result<bool, DbError> {
        self.with_conn(|conn| {
            Ok(conn
                .query_row("SELECT 1 FROM agent_runs WHERE id = ?1", [run_id], |_| Ok(true))
                .unwrap_or(false))
        })
    }

    // ---- audit -------------------------------------------------------------

    pub fn append_audit(&self, event: &AuditEvent) -> Result<(), DbError> {
        self.with_conn(|conn| {
            let kind = serde_json::to_value(event)?
                .get("kind")
                .and_then(|k| k.as_str())
                .unwrap_or("unknown")
                .to_string();
            conn.execute(
                "INSERT INTO audit_events (kind, payload_json, created_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![kind, serde_json::to_string(event)?, now_iso()],
            )?;
            Ok(())
        })
    }

    pub fn list_audit(&self, limit: u32) -> Result<Vec<AuditEvent>, DbError> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT payload_json FROM audit_events ORDER BY seq DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |r| r.get::<_, String>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(serde_json::from_str(&row?)?);
            }
            Ok(out)
        })
    }

    // ---- settings ----------------------------------------------------------

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), DbError> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
                rusqlite::params![key, serde_json::to_string(value)?],
            )?;
            Ok(())
        })
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>, DbError> {
        self.with_conn(|conn| {
            let res: Option<String> = conn
                .query_row("SELECT value_json FROM settings WHERE key = ?1", [key], |r| r.get(0))
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            match res {
                Some(s) => Ok(Some(serde_json::from_str(&s)?)),
                None => Ok(None),
            }
        })
    }

    /// Persist the alias table so the matcher never hard-codes synonyms.
    pub fn save_alias_table(&self, table: &SceneAliasTable) -> Result<(), DbError> {
        self.set_setting("scene_aliases", &serde_json::to_value(table)?)
    }

    pub fn load_alias_table(&self) -> Result<SceneAliasTable, DbError> {
        Ok(self
            .get_setting("scene_aliases")?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }
}
