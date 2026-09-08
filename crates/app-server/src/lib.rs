//! Application Controller (plan §12.4, §17).
//!
//! The ONLY component allowed to write project files. Agents propose;
//! validators gate; approval authorizes; *then* `commit_patch` writes the
//! next version atomically (temp file → parse → schema check → rename) and
//! finalizes the SQLite side in a single transaction. Startup reconciliation
//! adopts orphan snapshots left by a crash between the file write and the
//! DB finalize.

use agent_protocol::{AppEvent, EventBus};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use storyboard_clone::{CloneEngine, CloneOptions};
use storyboard_domain::{
    AuditEvent, OperationKind, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal,
    ProjectId, ProjectSnapshot, ProjectState, ProjectStatus, QueryIntent, RevisionId, TemplateId,
    TemplateMetadata, TemplateSnapshot,
};
use storyboard_importer::skill::{IndexEntry, SkillBundle};
use storyboard_importer::{build_metadata, scan_template};
use storyboard_matcher::{parse_intent, Matcher, MatcherConfig, Selection};
use storyboard_patch::{apply_proposal, diff_projects};
use storyboard_storage::{Db, Workspace};
use storyboard_validator::{validate, ValidationContext, ValidationReport, ValidatorConfig};

pub mod backend;

const KEYCHAIN_SERVICE: &str = "StoryboardStudio";

/// Non-secret provider configuration. The API key never appears here — it
/// lives in the OS keychain under `keychain_account` (= provider id).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub max_retries: Option<u32>,
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,
    #[serde(default)]
    pub keychain_account: String,
}

/// F07 persistence: every manifest lands in runs/<run_id>/manifest.json +
/// agent_runs at turn START (before any model call); every event streams into
/// agent_events with per-thread monotonic seq. The thread row is created
/// BEFORE the first event (agent_events has an FK to agent_threads).
impl agent_runtime::RunObserver for AppServer {
    fn on_manifest(
        &self,
        manifest: &storyboard_domain::AgentRunManifest,
        thread_id: &str,
        project_id: Option<&str>,
    ) {
        if let Ok(bytes) = serde_json::to_vec_pretty(manifest) {
            if let Err(e) = self.workspace.write_manifest(&manifest.run_id, &bytes) {
                self.warn_persistence("manifest file", &e.to_string());
            }
        }
        if let Err(e) = self.db.insert_agent_thread(
            thread_id,
            project_id,
            &manifest.provider_id,
            &manifest.model,
        ) {
            self.warn_persistence("agent thread row", &e.to_string());
        }
        if let Err(e) = self.db.update_agent_thread(
            thread_id,
            project_id,
            &manifest.provider_id,
            &manifest.model,
        ) {
            self.warn_persistence("agent thread row", &e.to_string());
        }
        if let Err(e) = self.db.insert_agent_run(manifest, thread_id) {
            self.warn_persistence("agent run row", &e.to_string());
        }
        if let Err(e) = self.db.append_audit(&AuditEvent::ManifestCreated {
            run_id: manifest.run_id.clone(),
        }) {
            self.warn_persistence("manifest audit", &e.to_string());
        }
    }

    fn on_event(&self, thread_id: &str, event: &agent_protocol::AppEvent) {
        // FK safety: an event may arrive before the manifest (unknown thread)
        let _ = self.db.ensure_agent_thread(thread_id);
        if let Ok(payload) = serde_json::to_string(event) {
            if let Err(e) = self
                .db
                .insert_agent_event(thread_id, event.type_name(), &payload)
            {
                self.warn_persistence("agent event", &e.to_string());
            }
        }
    }

    fn on_message(&self, thread_id: &str, message: &model_providers::ChatMessage) {
        if let Ok(json) = serde_json::to_string(message) {
            match self.db.insert_agent_message(thread_id, &json) {
                Ok(_seq) => {
                    if let Err(e) = self.workspace.append_rollout(thread_id, &json) {
                        self.warn_persistence("rollout append", &e.to_string());
                    }
                }
                Err(e) => self.warn_persistence("agent message", &e.to_string()),
            }
        }
    }

    fn resolve_auto_approval(&self, _thread_id: &str, project_id: &str, patch_id: i64) -> bool {
        let Ok(pid) = project_id.parse::<ProjectId>() else {
            return false;
        };
        match self.resolve_approval_internal(&pid, patch_id, true, "auto:low-risk") {
            Ok(()) => true,
            Err(e) => {
                self.warn_persistence("auto approval", &e.to_string());
                false
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("storage: {0}")]
    Storage(#[from] storyboard_storage::DbError),
    #[error("workspace: {0}")]
    Workspace(#[from] storyboard_storage::WorkspaceError),
    #[error("patch: {0}")]
    Patch(#[from] storyboard_domain::PatchError),
    #[error("parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("import: {0}")]
    Import(#[from] storyboard_importer::ScanError),
    #[error("skill bundle: {0}")]
    Skill(#[from] storyboard_importer::SkillBundleError),
    #[error("clone: {0}")]
    Clone(#[from] storyboard_clone::CloneError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportSummary {
    pub templates_imported: usize,
    pub duplicates: usize,
    pub total_warnings: usize,
    pub confidence_min: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitOutcome {
    pub project_id: String,
    pub new_version: u64,
    pub parent_version: u64,
    pub diff_path: String,
    pub preservation_ratio: f32,
}

pub struct AppServer {
    pub workspace: Workspace,
    pub db: Db,
    pub matcher_config: MatcherConfig,
    pub validator_config: ValidatorConfig,
    pub bus: std::sync::Arc<EventBus>,
    agent_manager: Mutex<Option<std::sync::Arc<agent_runtime::ThreadManager>>>,
    /// Per-project commit mutex: commit and rollback for the same project
    /// serialize; different projects proceed concurrently.
    commit_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Persistence failures that must not stay silent (disk full, db lock…).
    persistence_warnings: Mutex<Vec<String>>,
}

impl AppServer {
    /// Create a fresh workspace and import the frozen skill bundle.
    pub fn init(root: impl AsRef<std::path::Path>, skill: &SkillBundle) -> Result<Self, AppError> {
        let server = Self::init_empty(root)?;
        server.import_skill(skill)?;
        Ok(server)
    }

    /// Create a fresh empty workspace (no templates imported yet — the
    /// packaged app starts here and imports via the UI when the user wants).
    pub fn init_empty(root: impl AsRef<std::path::Path>) -> Result<Self, AppError> {
        let root: PathBuf = root.as_ref().to_path_buf();
        let workspace = Workspace::init(root)?;
        let db = Db::open(workspace.db_path())?;
        let bus = std::sync::Arc::new(EventBus::new());
        let server = Self {
            workspace,
            db,
            matcher_config: MatcherConfig::default(),
            validator_config: ValidatorConfig::default(),
            bus,
            agent_manager: None.into(),
            commit_locks: Mutex::new(HashMap::new()),
            persistence_warnings: Mutex::new(Vec::new()),
        };
        server.db.append_audit(&AuditEvent::WorkspaceInitialized {
            workspace_root: server.workspace.root.display().to_string(),
        })?;
        Ok(server)
    }

    /// Open an initialized workspace, then reconcile crash leftovers: orphan
    /// version snapshots on disk beyond the DB's latest version are adopted
    /// (roll-forward), and a lagging `projects.current_version` catches up.
    pub fn open(root: impl AsRef<std::path::Path>) -> Result<Self, AppError> {
        let root: PathBuf = root.as_ref().to_path_buf();
        let workspace = Workspace::open(root)?;
        let db = Db::open(workspace.db_path())?;
        let server = Self {
            workspace,
            db,
            matcher_config: MatcherConfig::default(),
            validator_config: ValidatorConfig::default(),
            bus: std::sync::Arc::new(EventBus::new()),
            agent_manager: None.into(),
            commit_locks: Mutex::new(HashMap::new()),
            persistence_warnings: Mutex::new(Vec::new()),
        };
        let adopted = server.reconcile_orphan_versions();
        if adopted > 0 {
            server.warn_persistence(
                "startup reconciliation",
                &format!("adopted {adopted} orphan version snapshot(s) after an unclean shutdown"),
            );
        }
        Ok(server)
    }

    fn warn_persistence(&self, what: &str, detail: &str) {
        let msg = format!("{what}: {detail}");
        self.persistence_warnings.lock().unwrap().push(msg.clone());
        // visible on the bus too — silent audit loss is unacceptable
        self.bus.emit(AppEvent::PatchCommitFailed {
            thread_id: String::new(),
            reason: format!("persistence degraded — {msg}"),
        });
    }

    pub fn take_persistence_warnings(&self) -> Vec<String> {
        std::mem::take(&mut *self.persistence_warnings.lock().unwrap())
    }

    /// Roll-forward recovery: adopt on-disk snapshots the DB does not know.
    fn reconcile_orphan_versions(&self) -> usize {
        let mut adopted = 0;
        let Ok(projects) = self.db.list_projects() else {
            return 0;
        };
        for row in projects {
            let Ok(pid) = row.id.parse::<ProjectId>() else {
                continue;
            };
            let db_max = self.db.max_recorded_version(&row.id).unwrap_or(0);
            for v in self.workspace.list_disk_versions(&pid) {
                if v > db_max {
                    let snapshot = self.workspace.version_path(&pid, v).display().to_string();
                    let diff_path = self.workspace.diff_path(&pid, db_max, v);
                    let diff = if diff_path.exists() {
                        Some(diff_path.display().to_string())
                    } else {
                        None
                    };
                    let audit = serde_json::json!({
                        "project_id": row.id, "recovered_version": v, "after_db_version": db_max,
                        "reason": "snapshot written but DB finalize never ran (crash recovery)"
                    });
                    if self
                        .db
                        .adopt_orphan_version(
                            &row.id,
                            v,
                            db_max,
                            &snapshot,
                            diff.as_deref(),
                            &audit.to_string(),
                        )
                        .is_ok()
                    {
                        adopted += 1;
                    }
                }
            }
            // lagging current_version (crash between finalize steps)
            let db_max_now = self.db.max_recorded_version(&row.id).unwrap_or(0);
            if db_max_now > row.current_version {
                let _ = self
                    .db
                    .update_project_status(&pid, ProjectStatus::Versioned, db_max_now);
            }
        }
        adopted
    }

    // ---- agent manager / provider factory -----------------------------------

    /// The long-lived agent thread manager (lifecycle 2.0: Op queue, steer,
    /// cancel, durable rollout). The provider comes from the persisted
    /// configuration: a real OpenAI-compatible provider via Settings, or the
    /// explicit `"mock"` demo mode. No silent fallback — an unconfigured
    /// workspace returns an error the UI must surface.
    pub fn agent_manager(
        self: &std::sync::Arc<Self>,
    ) -> Result<std::sync::Arc<agent_runtime::ThreadManager>, AppError> {
        let mut guard = self.agent_manager.lock().unwrap();
        if let Some(m) = guard.as_ref() {
            return Ok(m.clone());
        }
        let provider = self.build_active_provider()?;
        let manager = std::sync::Arc::new(agent_runtime::ThreadManager::new(
            agent_runtime::RuntimeConfig::default(),
            provider,
            self.bus.clone(),
            self.clone() as std::sync::Arc<dyn agent_runtime::RunObserver>,
            Some(self.clone() as std::sync::Arc<dyn storyboard_tools::ToolBackend>),
        ));
        *guard = Some(manager.clone());
        Ok(manager)
    }

    /// Drop the cached manager (provider/config changes take effect on the
    /// next `agent_manager()` call). Running threads finish their current
    /// turn only if a handle still holds the manager alive; new turns run on
    /// the rebuilt manager.
    pub fn reset_agent_manager(&self) {
        let _ = self.agent_manager.lock().unwrap().take();
    }

    fn build_active_provider(
        &self,
    ) -> Result<std::sync::Arc<dyn model_providers::StoryboardModelProvider>, AppError> {
        let active = self
            .db
            .get_setting("agent.active_provider")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(String::from));
        match active.as_deref() {
            Some("mock") => Ok(std::sync::Arc::new(model_providers::MockProvider::simple_text(
                "mock provider active (demo mode — configure a real provider in Settings)",
            ))),
            Some(id) => {
                let row = self.db.get_provider(id)?;
                let cfg: ProviderConfig = serde_json::from_str(&row.config_json)?;
                if cfg.base_url.is_empty() || cfg.model.is_empty() {
                    return Err(AppError::InvalidState(format!(
                        "provider `{id}` is incomplete (base_url/model missing)"
                    )));
                }
                let key = self.read_provider_api_key(id)?;
                Ok(std::sync::Arc::new(
                    model_providers::OpenAiCompatibleProvider::with_options(
                        id,
                        &cfg.base_url,
                        &key,
                        &cfg.model,
                        cfg.max_retries.unwrap_or(2),
                        cfg.idle_timeout_secs.unwrap_or(120),
                    ),
                ))
            }
            None => Err(AppError::InvalidState(
                "no provider configured — add one in Settings (or activate the explicit `mock` demo mode)".into(),
            )),
        }
    }

    // ---- provider configuration + keychain -----------------------------------

    fn keyring_entry(&self, account: &str) -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|e| AppError::InvalidState(format!("keychain unavailable: {e}")))
    }

    /// Store the API key in the OS keychain (Credential Manager on Windows,
    /// Keychain on macOS, Secret Service on Linux). SQLite only ever sees the
    /// opaque account id (= provider id).
    pub fn set_provider_api_key(&self, provider_id: &str, key: &str) -> Result<(), AppError> {
        if key.trim().is_empty() {
            return Err(AppError::InvalidState("empty api key".into()));
        }
        self.keyring_entry(provider_id)?
            .set_password(key)
            .map_err(|e| AppError::InvalidState(format!("keychain write failed: {e}")))?;
        self.db.append_audit(&AuditEvent::Custom {
            event_kind: "provider.key.set".into(),
            detail: format!("api key stored in OS keychain for provider {provider_id}"),
        })?;
        Ok(())
    }

    pub fn read_provider_api_key(&self, provider_id: &str) -> Result<String, AppError> {
        self.keyring_entry(provider_id)?
            .get_password()
            .map_err(|e| {
                AppError::InvalidState(format!(
                    "no api key in keychain for provider `{provider_id}` ({e})"
                ))
            })
    }

    pub fn provider_has_api_key(&self, provider_id: &str) -> bool {
        self.keyring_entry(provider_id)
            .ok()
            .and_then(|e| e.get_password().ok())
            .is_some()
    }

    pub fn delete_provider_api_key(&self, provider_id: &str) -> Result<(), AppError> {
        match self.keyring_entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::InvalidState(format!(
                "keychain delete failed: {e}"
            ))),
        }
    }

    /// Save the non-secret provider config (never the key). The active
    /// provider selection is a separate settings key; both reset the cached
    /// agent manager.
    pub fn save_provider(
        &self,
        id: &str,
        name: &str,
        base_url: &str,
        model: &str,
    ) -> Result<(), AppError> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AppError::InvalidState(
                "provider id must be alphanumeric/-/_".into(),
            ));
        }
        let cfg = ProviderConfig {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            max_retries: None,
            idle_timeout_secs: None,
            keychain_account: id.to_string(),
        };
        self.db
            .upsert_provider(id, "openai-compatible", name, &serde_json::to_string(&cfg)?)?;
        self.reset_agent_manager();
        Ok(())
    }

    pub fn delete_provider(&self, id: &str) -> Result<(), AppError> {
        self.db.delete_provider(id)?;
        let _ = self.delete_provider_api_key(id);
        if let Ok(Some(v)) = self.db.get_setting("agent.active_provider") {
            if v.as_str() == Some(id) {
                let _ = self
                    .db
                    .set_setting("agent.active_provider", &serde_json::json!(null));
            }
        }
        self.reset_agent_manager();
        Ok(())
    }

    /// Provider list for Settings — config only, plus whether a key exists.
    pub fn provider_summaries(&self) -> Result<serde_json::Value, AppError> {
        let active = self
            .db
            .get_setting("agent.active_provider")
            .ok()
            .flatten()
            .and_then(|v| v.as_str().map(String::from));
        let rows = self.db.list_providers()?;
        let list: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                let cfg: ProviderConfig =
                    serde_json::from_str(&r.config_json).unwrap_or(ProviderConfig {
                        base_url: String::new(),
                        model: String::new(),
                        max_retries: None,
                        idle_timeout_secs: None,
                        keychain_account: String::new(),
                    });
                serde_json::json!({
                    "id": r.id,
                    "name": r.name,
                    "type": r.ptype,
                    "base_url": cfg.base_url,
                    "model": cfg.model,
                    "has_api_key": self.provider_has_api_key(&r.id),
                    "active": active.as_deref() == Some(r.id.as_str()),
                })
            })
            .collect();
        Ok(serde_json::json!({ "providers": list, "active": active }))
    }

    pub fn set_active_provider(&self, id: &str) -> Result<(), AppError> {
        // validate before flipping: the manager must be constructible
        if id != "mock" {
            let row = self.db.get_provider(id)?;
            let cfg: ProviderConfig = serde_json::from_str(&row.config_json)?;
            if cfg.base_url.is_empty() || cfg.model.is_empty() {
                return Err(AppError::InvalidState("provider incomplete".into()));
            }
            if !self.provider_has_api_key(id) {
                return Err(AppError::InvalidState(
                    "store an API key for this provider before activating it".into(),
                ));
            }
        }
        self.db
            .set_setting("agent.active_provider", &serde_json::json!(id))?;
        self.reset_agent_manager();
        Ok(())
    }

    /// Connectivity probe: one tiny non-streaming turn against the provider.
    pub fn test_provider(&self, id: &str) -> Result<serde_json::Value, AppError> {
        let row = self.db.get_provider(id)?;
        let cfg: ProviderConfig = serde_json::from_str(&row.config_json)?;
        let key = self.read_provider_api_key(id)?;
        let provider = model_providers::OpenAiCompatibleProvider::with_options(
            id,
            &cfg.base_url,
            &key,
            &cfg.model,
            0,
            30,
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AppError::InvalidState(format!("test runtime: {e}")))?;
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let req = model_providers::TurnRequest {
            messages: vec![model_providers::ChatMessage::user(
                "ping — reply with the single word: pong",
            )],
            tools: vec![],
            sampling: model_providers::SamplingParams {
                temperature: 0.0,
                top_p: 1.0,
                max_tokens: 16,
            },
            force_json: false,
            stream: false,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let fut = <model_providers::OpenAiCompatibleProvider as model_providers::StoryboardModelProvider>::run_turn(
            &provider, req, cancel, tx,
        );
        let outcome = rt.block_on(async {
            match tokio::time::timeout(std::time::Duration::from_secs(30), fut).await {
                Ok(Ok(resp)) => Ok(serde_json::json!({
                    "ok": true,
                    "model": cfg.model,
                    "reply": resp.message.content.chars().take(120).collect::<String>(),
                })),
                Ok(Err(e)) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
                Err(_) => Ok(serde_json::json!({ "ok": false, "error": "timeout after 30s" })),
            }
        });
        while rx.try_recv().is_ok() {}
        outcome
    }

    /// Durable-session support: reload a thread's rollout as chat history.
    pub fn agent_thread_history(&self, thread_id: &str) -> Vec<model_providers::ChatMessage> {
        self.db.list_agent_messages(thread_id).unwrap_or_default()
    }

    // ---- Phase 0: import ----------------------------------------------------

    pub fn import_skill(&self, skill: &SkillBundle) -> Result<ImportSummary, AppError> {
        let entries: Vec<IndexEntry> = skill.legacy_index()?;
        let mut summary = ImportSummary {
            templates_imported: 0,
            duplicates: 0,
            total_warnings: 0,
            confidence_min: 1.0,
        };
        for entry in &entries {
            let bytes = skill.read_template(&entry.template_id)?;
            let scanned = scan_template(&entry.template_id, &entry.source_file, &bytes, true)?;
            let metadata = build_metadata(&scanned, Some(entry));
            if self.workspace.has_original(&scanned.snapshot.sha256) {
                summary.duplicates += 1;
            } else {
                self.workspace
                    .store_original(&scanned.snapshot.sha256, &bytes)?;
            }
            self.db.upsert_template(&metadata)?;
            summary.templates_imported += 1;
            summary.total_warnings += metadata.warnings.len();
            summary.confidence_min = summary.confidence_min.min(metadata.metadata_confidence);
            self.db.append_audit(&AuditEvent::TemplateImported {
                template_id: metadata.template_id.clone(),
                revision_id: metadata.revision_id.clone(),
                sha256: metadata.sha256.clone(),
                source_name: metadata.source_name.clone(),
                warnings: metadata.warnings.clone(),
                metadata_confidence: metadata.metadata_confidence,
            })?;
        }
        let aliases = skill.alias_table()?;
        self.db.save_alias_table(&aliases)?;
        Ok(summary)
    }

    // ---- templates ------------------------------------------------------------

    pub fn template_metadata(&self) -> Result<Vec<TemplateMetadata>, AppError> {
        Ok(self.db.list_template_metadata()?)
    }

    pub fn load_template_snapshot(&self, template_id: &str) -> Result<TemplateSnapshot, AppError> {
        let meta = self.db.get_template_metadata(template_id)?;
        let bytes = self.workspace.read_original(&meta.sha256)?;
        let scanned = scan_template(template_id, &meta.source_name, &bytes, true)?;
        Ok(scanned.snapshot)
    }

    // ---- matching (plan §8) -------------------------------------------------

    pub fn parse_intent(&self, input: &str) -> QueryIntent {
        parse_intent(input, &self.db.load_alias_table().unwrap_or_default())
    }

    pub fn match_templates(
        &self,
        intent: &QueryIntent,
        seed: Option<u64>,
    ) -> Result<Option<Selection>, AppError> {
        let templates = self.db.list_template_metadata()?;
        let aliases = self.db.load_alias_table()?;
        let matcher = Matcher::new(self.matcher_config.clone(), aliases, templates);
        Ok(matcher.select(intent, seed))
    }

    // ---- clone (plan §11) ----------------------------------------------------

    pub fn clone_project(
        &self,
        template_id: &str,
        title: Option<String>,
        seed: u64,
    ) -> Result<ProjectState, AppError> {
        let snapshot = self.load_template_snapshot(template_id)?;
        let cloned = CloneEngine::clone_template(
            &snapshot,
            &CloneOptions {
                title,
                rng_seed: seed,
                ..Default::default()
            },
        )?;
        let bytes = serde_json::to_vec_pretty(&cloned.raw)?;
        self.workspace
            .write_project_version(&cloned.project_id, 1, &bytes)?;
        let now = agent_protocol::now_iso();
        let state = ProjectState {
            project_id: cloned.project_id,
            title: cloned.summary.title.clone(),
            status: ProjectStatus::Cloned,
            current_version: 1,
            source_template_id: snapshot.id.clone(),
            source_revision_id: snapshot.revision_id.as_str().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        self.db.create_project(&state)?;
        self.db
            .insert_version(&storyboard_storage::ProjectVersionRow {
                project_id: state.project_id.to_string(),
                version_number: 1,
                parent_version: None,
                snapshot_path: self
                    .workspace
                    .version_path(&state.project_id, 1)
                    .display()
                    .to_string(),
                diff_path: None,
                created_at: agent_protocol::now_iso(),
            })?;
        self.db.append_audit(&AuditEvent::ProjectCloned {
            project_id: state.project_id.to_string(),
            template_id: snapshot.id.as_str().to_string(),
            revision_id: snapshot.revision_id.as_str().to_string(),
            version: 1,
        })?;
        self.bus.emit(AppEvent::ProjectVersionCreated {
            project_id: state.project_id.to_string(),
            version: 1,
        });
        Ok(state)
    }

    // ---- project snapshots ----------------------------------------------------

    pub fn load_project_snapshot(&self, pid: &ProjectId) -> Result<ProjectSnapshot, AppError> {
        let row = self.db.get_project(pid)?;
        let bytes = self
            .workspace
            .read_project_version(pid, row.current_version)?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
        // Resolve the PERSISTED revision — never `templates.current_revision_id`:
        // deriving from the current pointer silently rebases old projects
        // onto re-imported template revisions and defeats Reference Integrity.
        let sha256 = self.db.revision_sha(&row.source_template_revision_id)?;
        Ok(ProjectSnapshot {
            project_id: *pid,
            version: row.current_version,
            title: row.title.clone(),
            source: storyboard_domain::SourceTemplateRef {
                template_id: TemplateId::new(row.source_template_id.clone()),
                revision_id: RevisionId::new(row.source_template_revision_id.clone()),
                sha256,
            },
            raw,
        })
    }

    // ---- patch pipeline ---------------------------------------------------

    fn run_validation(
        &self,
        base: &ProjectSnapshot,
        proposal: &PatchProposal,
        draft: &serde_json::Value,
        touched: &BTreeSet<u32>,
    ) -> Result<ValidationReport, AppError> {
        let template = self.load_template_snapshot(proposal.primary_template_id.as_str())?;
        let metadata = self
            .db
            .get_template_metadata(proposal.primary_template_id.as_str())?;
        // Reference Integrity compares against the revision the project
        // actually baselines on (persisted), not the template's current one.
        let baseline_sha = self.db.revision_sha(base.source.revision_id.as_str())?;
        let ctx = ValidationContext {
            template: &template,
            template_metadata: &metadata,
            base,
            proposal,
            draft,
            applied_touched_panels: touched.clone(),
            current_template_sha: &baseline_sha,
            config: self.validator_config.clone(),
        };
        Ok(validate(&ctx))
    }

    /// Apply in memory + run every gate. Nothing is written.
    pub fn validate_patch(
        &self,
        pid: &ProjectId,
        proposal: &PatchProposal,
    ) -> Result<(ValidationReport, storyboard_patch::PatchApplication), AppError> {
        let base = self.load_project_snapshot(pid)?;
        let app = apply_proposal(&base, proposal)?;
        let report = self.run_validation(&base, proposal, &app.draft, &app.touched_panels)?;
        Ok((report, app))
    }

    /// Store the proposal + validation; the agent's only write-path entry.
    pub fn propose_patch(
        &self,
        pid: &ProjectId,
        proposal: &PatchProposal,
        run_id: Option<&str>,
    ) -> Result<(i64, ValidationReport), AppError> {
        let (report, _) = self.validate_patch(pid, proposal)?;
        let json = serde_json::to_string(proposal)?;
        let patch_id = self
            .db
            .insert_patch(pid, proposal.base_project_version, &json, run_id)?;
        let status = if report.passed {
            "validated"
        } else {
            "validation_failed"
        };
        self.db.transition_patch(
            patch_id,
            &pid.to_string(),
            &["proposed"],
            status,
            Some(&serde_json::to_string(&report)?),
        )?;
        self.db.update_project_status(
            pid,
            if report.passed {
                ProjectStatus::AwaitingApproval
            } else {
                ProjectStatus::PatchRejected
            },
            proposal.base_project_version,
        )?;
        self.db.append_audit(&AuditEvent::PatchProposed {
            project_id: pid.to_string(),
            base_version: proposal.base_project_version,
            operation_count: proposal.operations.len(),
            run_id: run_id.map(String::from),
        })?;
        self.db.append_audit(&AuditEvent::PatchValidated {
            project_id: pid.to_string(),
            base_version: proposal.base_project_version,
            passed: report.passed,
            gate_results: report
                .gates()
                .iter()
                .map(|g| format!("{}={}", g.gate, g.passed))
                .collect(),
        })?;
        self.bus.emit(AppEvent::PatchProposed {
            thread_id: String::new(),
            project_id: pid.to_string(),
            operation_count: proposal.operations.len(),
        });
        self.bus.emit(AppEvent::ValidatorCompleted {
            thread_id: String::new(),
            passed: report.passed,
            report_json: serde_json::to_value(&report)?,
        });
        Ok((patch_id, report))
    }

    /// Authoritative re-validation of a STORED patch row (by id). The
    /// approval/commit chain always refers to this row — a re-submitted
    /// proposal from the model can never shadow it.
    pub fn validate_patch_by_id(
        &self,
        pid: &ProjectId,
        patch_id: i64,
    ) -> Result<ValidationReport, AppError> {
        let patch = self.db.get_patch(patch_id)?;
        if patch.project_id != pid.to_string() {
            return Err(AppError::NotFound(format!(
                "patch {patch_id} belongs to project {}, not {pid}",
                patch.project_id
            )));
        }
        let proposal: PatchProposal = serde_json::from_str(&patch.proposal_json)?;
        let (report, _) = self.validate_patch(pid, &proposal)?;
        let status = if report.passed {
            "validated"
        } else {
            "validation_failed"
        };
        self.db
            .update_patch(patch_id, status, Some(&serde_json::to_string(&report)?))?;
        self.bus.emit(AppEvent::ValidatorCompleted {
            thread_id: String::new(),
            passed: report.passed,
            report_json: serde_json::to_value(&report)?,
        });
        Ok(report)
    }

    /// Everything the approval UI needs about one stored patch: proposal,
    /// stored validation, and a fresh in-memory preview diff of THIS patch.
    pub fn patch_detail(
        &self,
        pid: &ProjectId,
        patch_id: i64,
    ) -> Result<serde_json::Value, AppError> {
        let patch = self.db.get_patch(patch_id)?;
        if patch.project_id != pid.to_string() {
            return Err(AppError::NotFound(format!(
                "patch {patch_id} belongs to project {}, not {pid}",
                patch.project_id
            )));
        }
        let proposal: PatchProposal = serde_json::from_str(&patch.proposal_json)?;
        let (_, app) = self.validate_patch(pid, &proposal)?;
        let validation: serde_json::Value = match &patch.validation_json {
            Some(s) => serde_json::from_str(s)?,
            None => serde_json::Value::Null,
        };
        Ok(serde_json::json!({
            "patch_id": patch.id,
            "project_id": patch.project_id,
            "base_version": patch.base_version,
            "status": patch.status,
            "run_id": patch.run_id,
            "created_at": patch.created_at,
            "proposal": serde_json::to_value(&proposal)?,
            "validation": validation,
            "preview": {
                "applied": app.applied,
                "touched_panels": app.touched_panels.iter().collect::<Vec<_>>(),
                "diff": serde_json::to_value(&app.diff)?,
            },
        }))
    }

    /// Approve or reject a stored patch (user action / auto-approval policy).
    /// Ownership + state are enforced by the conditional transition: only a
    /// `validated` patch of THIS project can flip to approved; rejection
    /// accepts validated/approved.
    pub fn resolve_approval(
        &self,
        pid: &ProjectId,
        patch_id: i64,
        approved: bool,
    ) -> Result<(), AppError> {
        self.resolve_approval_internal(pid, patch_id, approved, "user")
    }

    fn resolve_approval_internal(
        &self,
        pid: &ProjectId,
        patch_id: i64,
        approved: bool,
        policy: &str,
    ) -> Result<(), AppError> {
        if approved {
            self.db.transition_patch(
                patch_id,
                &pid.to_string(),
                &["validated"],
                "approved",
                None,
            )?;
        } else {
            self.db.transition_patch(
                patch_id,
                &pid.to_string(),
                &["validated", "approved"],
                "rejected",
                None,
            )?;
        }
        self.db.append_audit(&AuditEvent::ApprovalResolved {
            project_id: pid.to_string(),
            approved,
            policy: policy.into(),
        })?;
        self.bus.emit(AppEvent::ApprovalResolved {
            thread_id: String::new(),
            patch_id,
            approved,
        });
        Ok(())
    }

    /// Deterministic identity-swap patch built from the template's
    /// text-verified anchors — exactly what a well-behaved agent emits for
    /// "把角色换成 X". Proposes + validates + stores the patch row.
    /// (A standalone quick action; the agent turn's result is its OWN patch.)
    pub fn validate_identity_swap(
        &self,
        pid: &ProjectId,
        new_anchor: &str,
    ) -> Result<(i64, ValidationReport), AppError> {
        let row = self.db.get_project(pid)?;
        let meta = self.db.get_template_metadata(&row.source_template_id)?;
        let snap = self.load_project_snapshot(pid)?;
        let text = serde_json::to_string(&snap.raw)?.to_lowercase();

        let mut replacements: Vec<storyboard_domain::TokenReplacement> = Vec::new();
        for v in &meta.character_anchor_variants {
            if text.contains(&v.to_lowercase()) {
                replacements.push(storyboard_domain::TokenReplacement {
                    old_token: v.clone(),
                    new_token: new_anchor.to_string(),
                });
            }
        }
        for a in &meta.character_anchors {
            if text.contains(&a.to_lowercase()) && !replacements.iter().any(|r| r.old_token == *a) {
                replacements.push(storyboard_domain::TokenReplacement {
                    old_token: a.clone(),
                    new_token: new_anchor.to_string(),
                });
            }
        }
        if replacements.is_empty() {
            return Err(AppError::NotFound(
                "no verified identity anchors in project".into(),
            ));
        }

        let proposal = PatchProposal {
            base_project_version: snap.version,
            primary_template_id: TemplateId::new(&row.source_template_id),
            intent_hash: "identity-swap".into(),
            intent: PatchIntent::CharacterReplace,
            touched_panels: vec![],
            expected_preservation_ratio: 0.90,
            rationale: vec![format!("identity swap -> {new_anchor}")],
            user_requested_resize: false,
            operations: vec![PatchOperation {
                common: PatchOperationCommon {
                    operation_id: "op-identity-swap".into(),
                    panel_index: None,
                    panel_id: None,
                    anchor: None,
                    expected_old: None,
                    expected_old_hash: None,
                    expected_project_version: snap.version,
                },
                kind: OperationKind::ReplaceCharacterIdentity {
                    replacements,
                    slots: None,
                    appearance_replacements: Vec::new(),
                },
            }],
        };
        self.propose_patch(pid, &proposal, None)
    }

    fn lock_project(&self, pid: &ProjectId) -> Arc<Mutex<()>> {
        let mut locks = self.commit_locks.lock().unwrap();
        locks
            .entry(pid.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// THE commit (plan §12.4). Loads the stored proposal, re-applies,
    /// re-validates, writes atomically, then finalizes the SQLite side in a
    /// single transaction. A crash after the file write is healed at startup
    /// (orphan adoption) or on retry (same-bytes detection).
    pub fn commit_patch(&self, pid: &ProjectId, patch_id: i64) -> Result<CommitOutcome, AppError> {
        let lock = self.lock_project(pid);
        let _guard = lock.lock().unwrap();

        let patch = self.db.get_patch(patch_id)?;
        if patch.project_id != pid.to_string() {
            return Err(AppError::NotFound(format!(
                "patch {patch_id} belongs to project {}, not {pid}",
                patch.project_id
            )));
        }
        if patch.status != "approved" {
            return Err(AppError::InvalidState(format!(
                "patch {} status is `{}`, only `approved` patches can commit",
                patch.id, patch.status
            )));
        }
        // timeline truth: the request is announced BEFORE the writes
        self.bus.emit(AppEvent::PatchCommitRequested {
            thread_id: String::new(),
            patch_id,
        });

        let proposal: PatchProposal = serde_json::from_str(&patch.proposal_json)?;
        let (report, app) = self.validate_patch(pid, &proposal)?;
        if !report.passed {
            self.db.update_patch(
                patch_id,
                "validation_failed",
                Some(&serde_json::to_string(&report)?),
            )?;
            return Err(AppError::ValidationFailed(
                report
                    .gates()
                    .iter()
                    .filter(|g| !g.passed)
                    .map(|g| format!("{}: {:?}", g.gate, g.failures))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }

        let base = self.load_project_snapshot(pid)?;
        let new_version = base.version + 1;
        // atomic write: temp → parse → schema → rename (§22)
        let bytes = serde_json::to_vec_pretty(&app.draft)?;
        let reparsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !storyboard_domain::schema::validate_storyboard_json(&reparsed).is_empty() {
            return Err(AppError::InvalidState(
                "draft fails schema validation at commit time".into(),
            ));
        }
        match self
            .workspace
            .write_project_version(pid, new_version, &bytes)
        {
            Ok(_) => {}
            Err(storyboard_storage::WorkspaceError::VersionConflict { .. }) => {
                // a previous crash wrote this file but never finalized the DB;
                // adopting is only safe when the bytes are identical
                let existing = self.workspace.read_project_version(pid, new_version)?;
                if existing != bytes {
                    return Err(AppError::InvalidState(format!(
                        "version v{new_version} already exists with DIFFERENT content — re-clone required"
                    )));
                }
            }
            Err(e) => return Err(e.into()),
        }

        // diff file
        let diff = diff_projects(base.version, new_version, &base.raw, &app.draft);
        let diff_bytes = serde_json::to_vec_pretty(&diff)?;
        let diff_path = self
            .workspace
            .write_diff(pid, base.version, new_version, &diff_bytes)?;

        // single SQLite transaction: version row + project status/version +
        // patch committed + audit — no half-applied metadata on crash
        let new_title = app.draft.get("title").and_then(|t| t.as_str());
        let audit = serde_json::to_string(&AuditEvent::PatchCommitted {
            project_id: pid.to_string(),
            new_version,
            parent_version: base.version,
            run_id: patch.run_id.clone(),
        })?;
        self.db.finalize_commit(
            &pid.to_string(),
            patch_id,
            new_version,
            base.version,
            &self
                .workspace
                .version_path(pid, new_version)
                .display()
                .to_string(),
            Some(&diff_path.display().to_string()),
            new_title,
            &audit,
            "patch.committed",
        )?;

        self.bus.emit(AppEvent::PatchCommitCompleted {
            thread_id: String::new(),
            new_version,
        });
        self.bus.emit(AppEvent::ProjectVersionCreated {
            project_id: pid.to_string(),
            version: new_version,
        });
        Ok(CommitOutcome {
            project_id: pid.to_string(),
            new_version,
            parent_version: base.version,
            diff_path: diff_path.display().to_string(),
            preservation_ratio: report.preservation_ratio,
        })
    }

    /// Rollback (F04): copy an older snapshot forward as a new version —
    /// versions stay immutable, history stays complete, and a diff of
    /// (current → restored) records what the rollback changed.
    pub fn rollback(&self, pid: &ProjectId, to_version: u64) -> Result<u64, AppError> {
        let lock = self.lock_project(pid);
        let _guard = lock.lock().unwrap();

        let current = self.db.get_project(pid)?;
        if to_version == current.current_version {
            return Err(AppError::InvalidState("already at that version".into()));
        }
        if to_version > current.current_version {
            return Err(AppError::InvalidState("cannot roll forward".into()));
        }
        let bytes = self.workspace.read_project_version(pid, to_version)?;
        let restored: serde_json::Value = serde_json::from_slice(&bytes)?;
        let new_version = current.current_version + 1;
        let cur_bytes = self
            .workspace
            .read_project_version(pid, current.current_version)?;
        let cur_json: serde_json::Value = serde_json::from_slice(&cur_bytes)?;

        match self
            .workspace
            .write_project_version(pid, new_version, &bytes)
        {
            Ok(_) => {}
            Err(storyboard_storage::WorkspaceError::VersionConflict { .. }) => {
                let existing = self.workspace.read_project_version(pid, new_version)?;
                if existing != bytes {
                    return Err(AppError::InvalidState(format!(
                        "version v{new_version} already exists with DIFFERENT content"
                    )));
                }
            }
            Err(e) => return Err(e.into()),
        }
        let diff = diff_projects(current.current_version, new_version, &cur_json, &restored);
        let diff_bytes = serde_json::to_vec_pretty(&diff)?;
        let diff_path =
            self.workspace
                .write_diff(pid, current.current_version, new_version, &diff_bytes)?;

        let new_title = restored.get("title").and_then(|t| t.as_str());
        let audit = serde_json::to_string(&AuditEvent::VersionRolledBack {
            project_id: pid.to_string(),
            from_version: current.current_version,
            to_version: new_version,
        })?;
        self.db.finalize_commit(
            &pid.to_string(),
            -1, // no patch row for rollbacks
            new_version,
            current.current_version,
            &self
                .workspace
                .version_path(pid, new_version)
                .display()
                .to_string(),
            Some(&diff_path.display().to_string()),
            new_title,
            &audit,
            "version.rolled_back",
        )?;
        self.bus.emit(AppEvent::ProjectVersionCreated {
            project_id: pid.to_string(),
            version: new_version,
        });
        Ok(new_version)
    }

    /// Export the current version's JSON to a user-chosen path (plan §27).
    pub fn export_json(
        &self,
        pid: &ProjectId,
        out_path: &std::path::Path,
    ) -> Result<PathBuf, AppError> {
        let row = self.db.get_project(pid)?;
        let bytes = self
            .workspace
            .read_project_version(pid, row.current_version)?;
        // exported JSON must parse and stay schema-compatible
        let v: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !storyboard_domain::schema::validate_storyboard_json(&v).is_empty() {
            return Err(AppError::InvalidState(
                "refusing to export schema-invalid JSON".into(),
            ));
        }
        if let Some(dir) = out_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(out_path, &bytes)?;
        self.db.append_audit(&AuditEvent::ProjectExported {
            project_id: pid.to_string(),
            version: row.current_version,
            path: out_path.display().to_string(),
        })?;
        self.bus.emit(AppEvent::ExportCompleted {
            project_id: pid.to_string(),
            path: out_path.display().to_string(),
        });
        Ok(out_path.to_path_buf())
    }
}
