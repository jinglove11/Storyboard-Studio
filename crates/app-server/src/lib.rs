//! Application Controller (plan §12.4, §17).
//!
//! The ONLY component allowed to write project files. Agents propose;
//! validators gate; approval authorizes; *then* `commit_patch` writes the
//! next version atomically (temp file → parse → schema check → rename).

use agent_protocol::{AppEvent, EventBus};
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
use std::collections::BTreeSet;
use std::path::PathBuf;

pub mod backend;

/// F07 persistence: every manifest lands in runs/<run_id>/manifest.json +
/// agent_runs at turn START (before any model call); every event streams into
/// agent_events with per-thread monotonic seq.
impl agent_runtime::RunObserver for AppServer {
    fn on_manifest(&self, manifest: &storyboard_domain::AgentRunManifest, thread_id: &str) {
        if let Ok(bytes) = serde_json::to_vec_pretty(manifest) {
            let _ = self.workspace.write_manifest(&manifest.run_id, &bytes);
        }
        let _ = self.db.insert_agent_thread(
            thread_id,
            manifest.base_project_version.as_deref().map(|_| thread_id),
            &manifest.provider_id,
            &manifest.model,
        );
        let _ = self.db.insert_agent_run(manifest, thread_id);
        let _ = self.db.append_audit(&AuditEvent::ManifestCreated { run_id: manifest.run_id.clone() });
    }

    fn on_event(&self, thread_id: &str, event: &agent_protocol::AppEvent) {
        if let Ok(payload) = serde_json::to_string(event) {
            let _ = self.db.insert_agent_event(thread_id, event.type_name(), &payload);
        }
    }

    fn on_message(&self, thread_id: &str, seq: usize, message: &model_providers::ChatMessage) {
        if let Ok(json) = serde_json::to_string(message) {
            let _ = self.db.insert_agent_message(thread_id, seq as u64, &json);
            let _ = self.workspace.append_rollout(thread_id, &json);
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
    agent_manager: std::sync::Mutex<Option<std::sync::Arc<agent_runtime::ThreadManager>>>,
}

impl AppServer {
    /// Create a fresh workspace and import the frozen skill bundle.
    pub fn init(root: impl AsRef<std::path::Path>, skill: &SkillBundle) -> Result<Self, AppError> {
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
        };
        server.db.append_audit(&AuditEvent::WorkspaceInitialized {
            workspace_root: server.workspace.root.display().to_string(),
        })?;
        server.import_skill(skill)?;
        Ok(server)
    }

    /// Open an initialized workspace.
    pub fn open(root: impl AsRef<std::path::Path>) -> Result<Self, AppError> {
        let root: PathBuf = root.as_ref().to_path_buf();
        let workspace = Workspace::open(root)?;
        let db = Db::open(workspace.db_path())?;
        Ok(Self {
            workspace,
            db,
            matcher_config: MatcherConfig::default(),
            validator_config: ValidatorConfig::default(),
            bus: std::sync::Arc::new(EventBus::new()),
            agent_manager: None.into(),
        })
    }

    /// The long-lived agent thread manager (lifecycle 2.0: Op queue, steer,
    /// cancel, durable rollout). Provider defaults to the scripted mock
    /// until Settings wires a real one; events flow on the shared bus.
    pub fn agent_manager(self: &std::sync::Arc<Self>) -> std::sync::Arc<agent_runtime::ThreadManager> {
        let mut guard = self.agent_manager.lock().unwrap();
        if let Some(m) = guard.as_ref() {
            return m.clone();
        }
        let manager = std::sync::Arc::new(agent_runtime::ThreadManager::new(
            agent_runtime::RuntimeConfig::default(),
            std::sync::Arc::new(model_providers::MockProvider::simple_text(
                "acknowledged (mock provider active — configure a real provider in Settings)",
            )),
            self.bus.clone(),
            self.clone() as std::sync::Arc<dyn agent_runtime::RunObserver>,
            Some(self.clone() as std::sync::Arc<dyn storyboard_tools::ToolBackend>),
        ));
        *guard = Some(manager.clone());
        manager
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
                self.workspace.store_original(&scanned.snapshot.sha256, &bytes)?;
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

    pub fn match_templates(&self, intent: &QueryIntent, seed: Option<u64>) -> Result<Option<Selection>, AppError> {
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
            &CloneOptions { title, rng_seed: seed, ..Default::default() },
        )?;
        let bytes = serde_json::to_vec_pretty(&cloned.raw)?;
        self.workspace.write_project_version(&cloned.project_id, 1, &bytes)?;
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
        self.db.insert_version(&storyboard_storage::ProjectVersionRow {
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
        let bytes = self.workspace.read_project_version(pid, row.current_version)?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
        let sha256 = self.db.current_revision_sha(&row.source_template_id)?;
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
        let metadata = self.db.get_template_metadata(proposal.primary_template_id.as_str())?;
        let ctx = ValidationContext {
            template: &template,
            template_metadata: &metadata,
            base,
            proposal,
            draft,
            applied_touched_panels: touched.clone(),
            current_template_sha: &metadata.sha256,
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
        let patch_id = self.db.insert_patch(pid, proposal.base_project_version, &json, run_id)?;
        let status = if report.passed { "validated" } else { "validation_failed" };
        self.db.update_patch(patch_id, status, Some(&serde_json::to_string(&report)?))?;
        self.db.update_project_status(
            pid,
            if report.passed { ProjectStatus::AwaitingApproval } else { ProjectStatus::PatchRejected },
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
            gate_results: report.gates().iter().map(|g| format!("{}={}", g.gate, g.passed)).collect(),
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

    /// Approve or reject a stored patch (user action / auto-approval policy).
    pub fn resolve_approval(&self, pid: &ProjectId, patch_id: i64, approved: bool) -> Result<(), AppError> {
        self.db.update_patch(patch_id, if approved { "approved" } else { "rejected" }, None)?;
        self.db.append_audit(&AuditEvent::ApprovalResolved {
            project_id: pid.to_string(),
            approved,
            policy: "user".into(),
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
            if text.contains(&a.to_lowercase())
                && !replacements.iter().any(|r| r.old_token == *a)
            {
                replacements.push(storyboard_domain::TokenReplacement {
                    old_token: a.clone(),
                    new_token: new_anchor.to_string(),
                });
            }
        }
        if replacements.is_empty() {
            return Err(AppError::NotFound("no verified identity anchors in project".into()));
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
                kind: OperationKind::ReplaceCharacterIdentity { replacements, slots: None },
            }],
        };
        self.propose_patch(pid, &proposal, None)
    }

    /// THE commit (plan §12.4). Loads the stored proposal, re-applies,
    /// re-validates, writes atomically, creates the version + diff.
    pub fn commit_patch(&self, pid: &ProjectId, patch_id: i64) -> Result<CommitOutcome, AppError> {
        let patch = self.db.latest_patch(pid)?;
        if patch.id != patch_id {
            return Err(AppError::NotFound(format!("patch {patch_id} is not the latest for {pid}")));
        }
        if patch.status != "approved" {
            return Err(AppError::InvalidState(format!(
                "patch {} status is `{}`, only `approved` patches can commit",
                patch.id, patch.status
            )));
        }
        // §21 chain: approval -> CommitRequested -> Committed -> Versioned
        let base_pre = self.load_project_snapshot(pid)?;
        self.db.update_project_status(pid, ProjectStatus::CommitRequested, base_pre.version)?;
        let proposal: PatchProposal = serde_json::from_str(&patch.proposal_json)?;
        let (report, app) = self.validate_patch(pid, &proposal)?;
        if !report.passed {
            self.db.update_patch(patch_id, "validation_failed", Some(&serde_json::to_string(&report)?))?;
            return Err(AppError::ValidationFailed(
                report.gates().iter().filter(|g| !g.passed).map(|g| format!("{}: {:?}", g.gate, g.failures)).collect::<Vec<_>>().join("; "),
            ));
        }

        // atomic write: temp → parse → schema → rename (§22)
        let base = self.load_project_snapshot(pid)?;
        let new_version = base.version + 1;
        let bytes = serde_json::to_vec_pretty(&app.draft)?;
        let reparsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !storyboard_domain::schema::validate_storyboard_json(&reparsed).is_empty() {
            return Err(AppError::InvalidState("draft fails schema validation at commit time".into()));
        }
        self.workspace.write_project_version(pid, new_version, &bytes)?;

        // diff file
        let diff = diff_projects(base.version, new_version, &base.raw, &app.draft);
        let diff_bytes = serde_json::to_vec_pretty(&diff)?;
        let diff_path = self.workspace.write_diff(pid, base.version, new_version, &diff_bytes)?;

        self.db.update_project_status(pid, ProjectStatus::Committed, new_version)?;
        self.db.insert_version(&storyboard_storage::ProjectVersionRow {
            project_id: pid.to_string(),
            version_number: new_version,
            parent_version: Some(base.version),
            snapshot_path: self.workspace.version_path(pid, new_version).display().to_string(),
            diff_path: Some(diff_path.display().to_string()),
            created_at: agent_protocol::now_iso(),
        })?;
        self.db.update_patch(patch_id, "committed", None)?;
        self.db.update_project_status(pid, ProjectStatus::Versioned, new_version)?;
        self.db.append_audit(&AuditEvent::PatchCommitted {
            project_id: pid.to_string(),
            new_version,
            parent_version: base.version,
            run_id: patch.run_id.clone(),
        })?;
        self.bus.emit(AppEvent::PatchCommitRequested { thread_id: String::new(), patch_id });
        self.bus.emit(AppEvent::PatchCommitCompleted { thread_id: String::new(), new_version });
        self.bus.emit(AppEvent::ProjectVersionCreated { project_id: pid.to_string(), version: new_version });
        Ok(CommitOutcome {
            project_id: pid.to_string(),
            new_version,
            parent_version: base.version,
            diff_path: diff_path.display().to_string(),
            preservation_ratio: report.preservation_ratio,
        })
    }

    /// MVP rollback (F04): copy an older snapshot forward as a new version —
    /// versions stay immutable, history stays complete.
    pub fn rollback(&self, pid: &ProjectId, to_version: u64) -> Result<u64, AppError> {
        let current = self.db.get_project(pid)?;
        if to_version == current.current_version {
            return Err(AppError::InvalidState("already at that version".into()));
        }
        if to_version > current.current_version {
            return Err(AppError::InvalidState("cannot roll forward".into()));
        }
        let bytes = self.workspace.read_project_version(pid, to_version)?;
        let new_version = current.current_version + 1;
        self.workspace.write_project_version(pid, new_version, &bytes)?;
        self.db.insert_version(&storyboard_storage::ProjectVersionRow {
            project_id: pid.to_string(),
            version_number: new_version,
            parent_version: Some(current.current_version),
            snapshot_path: self.workspace.version_path(pid, new_version).display().to_string(),
            diff_path: None,
            created_at: agent_protocol::now_iso(),
        })?;
        self.db.update_project_status(pid, ProjectStatus::Versioned, new_version)?;
        self.db.append_audit(&AuditEvent::VersionRolledBack {
            project_id: pid.to_string(),
            from_version: current.current_version,
            to_version: new_version,
        })?;
        Ok(new_version)
    }

    /// Export the current version's JSON to a user-chosen path (plan §27).
    pub fn export_json(&self, pid: &ProjectId, out_path: &std::path::Path) -> Result<PathBuf, AppError> {
        let row = self.db.get_project(pid)?;
        let bytes = self.workspace.read_project_version(pid, row.current_version)?;
        // exported JSON must parse and stay schema-compatible
        let v: serde_json::Value = serde_json::from_slice(&bytes)?;
        if !storyboard_domain::schema::validate_storyboard_json(&v).is_empty() {
            return Err(AppError::InvalidState("refusing to export schema-invalid JSON".into()));
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
