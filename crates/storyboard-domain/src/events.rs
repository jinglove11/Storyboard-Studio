use serde::{Deserialize, Serialize};

/// Append-only audit events (plan §1 可审计). Every tool call, patch, gate,
/// approval and commit lands here as well as in the agent event stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditEvent {
    WorkspaceInitialized { workspace_root: String },
    TemplateImported {
        template_id: String,
        revision_id: String,
        sha256: String,
        source_name: String,
        warnings: Vec<String>,
        metadata_confidence: f32,
    },
    ProjectCloned {
        project_id: String,
        template_id: String,
        revision_id: String,
        version: u64,
    },
    PatchProposed {
        project_id: String,
        base_version: u64,
        operation_count: usize,
        run_id: Option<String>,
    },
    PatchValidated {
        project_id: String,
        base_version: u64,
        passed: bool,
        gate_results: Vec<String>,
    },
    ApprovalResolved {
        project_id: String,
        approved: bool,
        policy: String,
    },
    PatchCommitted {
        project_id: String,
        new_version: u64,
        parent_version: u64,
        run_id: Option<String>,
    },
    VersionRolledBack {
        project_id: String,
        from_version: u64,
        to_version: u64,
    },
    ProjectExported {
        project_id: String,
        version: u64,
        path: String,
    },
    ManifestCreated { run_id: String },
}
