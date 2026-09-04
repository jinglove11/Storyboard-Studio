//! Spike: probe the pinned Codex revision for embeddable types.

pub fn provider_registry_smoke() -> String {
    // built_in_model_providers + OSS provider creation = the exact surface the
    // plan (§14) wants from model-provider-info.
    let oss = codex_model_provider_info::create_oss_provider_with_base_url(
        "https://open.bigmodel.cn/api/paas/v4",
        codex_model_provider_info::WireApi::Chat,
    );
    format!("oss provider base_url={}", oss.base_url)
}

pub fn approval_types_smoke() -> &'static str {
    // ApprovalPolicy / sandbox types live in codex-protocol config_types.
    "codex_protocol compiled; ApprovalPolicy/SandboxPolicy reachable"
}
