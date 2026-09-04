//! Codex-derived agent lifecycle (plan §6): thread → turn → tool loop →
//! PatchProposal → deterministic validation → approval request.
//!
//! The runtime never commits. `commit_storyboard_patch` is not a tool; the
//! Application Controller commits after the user (or auto-approval policy)
//! resolves the request.

pub mod manifest;
pub mod presets;
pub mod turn;

pub use manifest::{core_contract_hash, new_run_id};
pub use presets::PromptPresets;
pub use turn::{AgentRuntime, ApprovalMode, ApprovalPolicy, RuntimeConfig, TurnOutput, TurnStatus};

pub const PROMPT_PRESET_VERSION: &str = "v1";
