use serde::{Deserialize, Serialize};

/// Fixed execution environment for every agent run (plan §6.3, F07).
/// Secrets never enter the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunManifest {
    pub run_id: String,
    pub provider_id: String,
    pub model: String,
    pub prompt_preset_version: String,
    pub core_contract_hash: String,
    pub tool_registry_version: String,
    pub primary_template_revision: Option<String>,
    pub base_project_version: Option<String>,
    pub sampling: serde_json::Value,
    pub created_at: String,
}
