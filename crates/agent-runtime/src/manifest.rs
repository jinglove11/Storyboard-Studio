use storyboard_domain::AgentRunManifest;

/// sha256 of the CORE_CONTRACT text — recorded in every run manifest so a
/// contract change is visible across runs (F07).
pub fn core_contract_hash(contract: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(contract.as_bytes());
    hex::encode(&h.finalize()[..16])
}

pub fn new_run_id() -> String {
    agent_protocol::new_id("run")
}

pub fn build_manifest(
    run_id: &str,
    provider_id: &str,
    model: &str,
    preset_version: &str,
    contract_hash: &str,
    registry_version: &str,
    template_revision: Option<&str>,
    base_project_version: Option<&str>,
    sampling: &serde_json::Value,
) -> AgentRunManifest {
    AgentRunManifest {
        run_id: run_id.into(),
        provider_id: provider_id.into(),
        model: model.into(),
        prompt_preset_version: preset_version.into(),
        core_contract_hash: contract_hash.into(),
        tool_registry_version: registry_version.into(),
        primary_template_revision: template_revision.map(String::from),
        base_project_version: base_project_version.map(String::from),
        sampling: sampling.clone(),
        created_at: agent_protocol::now_iso(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_hash_is_stable() {
        assert_eq!(core_contract_hash("abc"), core_contract_hash("abc"));
        assert_ne!(core_contract_hash("abc"), core_contract_hash("abd"));
    }
}
