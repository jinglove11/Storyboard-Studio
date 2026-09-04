//! Codex-derived agent lifecycle 2.0 (plan §6).
//!
//! Threads are long-lived, Op-queue-driven, cancellable, steerable and
//! durable (rollout persistence via `RunObserver`). The runtime never
//! commits; the Application Controller commits after approval.

pub mod manifest;
pub mod presets;
pub mod thread;
pub mod turn;

pub use manifest::{core_contract_hash, new_run_id};
pub use presets::PromptPresets;
pub use thread::{NoopObserver, RunObserver, ThreadHandle, ThreadLifecycle, ThreadManager, ThreadOp};
pub use turn::{apply_context_budget, ApprovalMode, ApprovalPolicy, CompactionStats, ContextBudget,
    RuntimeConfig, TurnStatus};

pub const PROMPT_PRESET_VERSION: &str = "v1";
