use crate::manifest::{build_manifest, core_contract_hash, new_run_id};
use crate::presets::PromptPresets;
use agent_protocol::{AppEvent, EventBus};
use model_providers::{ChatMessage, SamplingParams, StoryboardModelProvider, TurnRequest};
use storyboard_domain::AgentRunManifest;
use storyboard_tools::{AgentProfile, ToolRegistry, ToolBackend};
use std::sync::Arc;

/// Persistence hook (F07): the runtime stays storage-free, but every run
/// manifest and every emitted event is offered to an observer *as it
/// happens* — the app server records them into `runs/` + SQLite.
pub trait RunObserver: Sync {
    fn on_manifest(&self, _manifest: &AgentRunManifest, _thread_id: &str) {}
    fn on_event(&self, _thread_id: &str, _event: &AppEvent) {}
}

/// No-op observer for callers that don't persist.
pub struct NoopObserver;
impl RunObserver for NoopObserver {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalMode {
    /// Auto-approve low-risk patches (identity/scene), prompt otherwise.
    AutoLowRisk,
    /// Always ask the user.
    AlwaysPrompt,
}

#[derive(Debug, Clone)]
pub struct ApprovalPolicy {
    pub mode: ApprovalMode,
}

impl ApprovalPolicy {
    /// Decide for a proposal; returns (approved, risk).
    pub fn decide(&self, proposal: &serde_json::Value) -> (bool, &'static str) {
        let has_resize = proposal["operations"]
            .as_array()
            .map(|ops| ops.iter().any(|o| o["type"] == "resize_storyboard"))
            .unwrap_or(false);
        let has_delete = proposal["operations"]
            .as_array()
            .map(|ops| ops.iter().any(|o| o["type"] == "delete_conflicting_block"))
            .unwrap_or(false);
        let risk: &'static str = if has_resize { "high" } else if has_delete { "medium" } else { "low" };
        let approved = match (self.mode, risk) {
            (ApprovalMode::AutoLowRisk, "low") => true,
            _ => false,
        };
        (approved, risk)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub profile: AgentProfile,
    pub approval: ApprovalPolicy,
    pub max_tool_rounds: usize,
    pub max_validator_retries: usize,
    pub sampling: SamplingParams,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: AgentProfile::StoryboardProduction,
            approval: ApprovalPolicy { mode: ApprovalMode::AlwaysPrompt },
            max_tool_rounds: 8,
            max_validator_retries: 2,
            sampling: SamplingParams::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TurnStatus {
    /// Assistant finished with a plain answer (no patch).
    Completed { reply: String },
    /// Patch validated OK; waiting for the user / auto-approval.
    NeedsApproval { patch_id: i64, auto_approved: bool, risk: String },
    /// Validation kept failing after retries.
    ValidationExhausted { failures: Vec<String> },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct TurnOutput {
    pub thread_id: String,
    pub turn_id: String,
    pub run_id: String,
    pub manifest: AgentRunManifest,
    pub status: TurnStatus,
    pub transcript: Vec<ChatMessage>,
    pub last_validation: Option<serde_json::Value>,
    pub last_proposal: Option<serde_json::Value>,
}

pub struct AgentRuntime {
    pub config: RuntimeConfig,
    pub registry: ToolRegistry,
    pub presets: PromptPresets,
    pub provider: Arc<dyn StoryboardModelProvider>,
    pub bus: Arc<EventBus>,
}

impl AgentRuntime {
    pub fn new(config: RuntimeConfig, provider: Arc<dyn StoryboardModelProvider>, bus: Arc<EventBus>) -> Self {
        let presets = PromptPresets::v1();
        let registry = ToolRegistry::for_profile(config.profile);
        Self { config, registry, presets, provider, bus }
    }

    /// Run one agent turn against a project (plan §6.2 agent loop).
    pub fn run_turn(
        &self,
        thread_id: &str,
        project_id: Option<&str>,
        user_message: &str,
        backend: &dyn ToolBackend,
        observer: Option<&dyn RunObserver>,
    ) -> TurnOutput {
        let turn_id = agent_protocol::new_id("turn");
        let run_id = new_run_id();
        let emit = |e: AppEvent| {
            self.bus.emit(e.clone());
            if let Some(o) = observer {
                o.on_event(thread_id, &e);
            }
        };
        emit(AppEvent::ThreadStarted { thread_id: thread_id.into() });
        emit(AppEvent::TurnStarted {
            thread_id: thread_id.into(),
            turn_id: turn_id.clone(),
            run_id: run_id.clone(),
        });

        // --- run manifest (F07) ---
        let contract_hash = core_contract_hash(&self.presets.core_contract);
        let (template_rev, base_version) = match project_id.and_then(|pid| backend.read_project(pid).ok()) {
            Some(state) => (
                state.get("source_revision_id").and_then(|v| v.as_str()).map(String::from),
                state.get("current_version").map(|v| v.to_string()),
            ),
            None => (None, None),
        };
        let manifest = build_manifest(
            &run_id,
            self.provider.id(),
            self.provider.model(),
            &self.presets.version,
            &contract_hash,
            self.registry.version(),
            template_rev.as_deref(),
            base_version.as_deref(),
            &serde_json::to_value(&self.config.sampling).unwrap_or_default(),
        );
        if let Some(o) = observer {
            o.on_manifest(&manifest, thread_id);
        }
        emit(AppEvent::AgentRunManifestCreated { run_id: run_id.clone() });

        // --- conversation ---
        let task = match project_id {
            Some(pid) => format!(
                "Project {pid}. Use the tools to inspect, then propose a typed storyboard patch. {}",
                self.presets.character_replace
            ),
            None => "Find the best Primary Template with search_templates.".into(),
        };
        let mut messages = vec![ChatMessage::system(self.presets.system_prompt(&task))];
        messages.push(ChatMessage::user(user_message));

        let mut transcript = messages.clone();
        let mut retries = 0usize;
        let mut last_validation: Option<serde_json::Value> = None;
        let mut last_proposal: Option<serde_json::Value> = None;
        let mut status = TurnStatus::Failed { error: "no provider response".into() };
        let mut proposed_patch_id: i64 = -1;

        'outer: for _round in 0..self.config.max_tool_rounds {
            let req = TurnRequest {
                messages: messages.clone(),
                tools: self.registry.schemas_for_provider(),
                sampling: self.config.sampling.clone(),
                force_json: false,
            };
            let resp = match self.provider.start_turn(&req) {
                Ok(r) => r,
                Err(e) => {
                    status = TurnStatus::Failed { error: e.to_string() };
                    break;
                }
            };
            let assistant = resp.message.clone();
            transcript.push(assistant.clone());

            if assistant.tool_calls.is_empty() {
                emit(AppEvent::MessageDelta {
                    thread_id: thread_id.into(),
                    text: assistant.content.clone(),
                });
                status = TurnStatus::Completed { reply: assistant.content.clone() };
                break;
            }

            messages.push(assistant);
            let mut proposal_this_round: Option<serde_json::Value> = None;

            for call in &resp.message.tool_calls {
                emit(AppEvent::ToolStarted {
                    thread_id: thread_id.into(),
                    tool: call.name.clone(),
                });
                let args: serde_json::Value =
                    serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::Value::Null);
                let outcome = self.registry.dispatch(&call.name, &args, Some(&run_id), backend);
                let (ok, payload) = match outcome {
                    Ok(v) => (true, v),
                    Err(e) => (false, serde_json::json!({ "error": e.to_string() })),
                };
                if call.name == "propose_storyboard_patch" {
                    if let Some(p) = args.get("proposal").cloned() {
                        proposal_this_round = Some(p);
                    }
                    if let Some(pid) = payload.get("patch_id").and_then(|v| v.as_i64()) {
                        proposed_patch_id = pid;
                    }
                }
                if call.name == "validate_storyboard_patch" {
                    if let Some(report) = payload.get("report").cloned() {
                        last_validation = Some(report.clone());
                        let passed = report.get("passed").and_then(|v| v.as_bool()).unwrap_or(false);
                        emit(AppEvent::ValidatorCompleted {
                            thread_id: thread_id.into(),
                            passed,
                            report_json: report.clone(),
                        });
                        if passed {
                            if let Some(p) = proposal_this_round.take().or_else(|| last_proposal.clone()) {
                                last_proposal = Some(p);
                            }
                            // approval flow
                            let proposal = last_proposal.clone().unwrap_or_default();
                            let (auto, r) = self.config.approval.decide(&proposal);
                            emit(AppEvent::ApprovalRequested {
                                thread_id: thread_id.into(),
                                patch_id: proposed_patch_id,
                                risk: r.to_string(),
                            });
                            if auto {
                                emit(AppEvent::ApprovalResolved {
                                    thread_id: thread_id.into(),
                                    patch_id: proposed_patch_id,
                                    approved: true,
                                });
                            }
                            status = TurnStatus::NeedsApproval {
                                patch_id: proposed_patch_id,
                                auto_approved: auto,
                                risk: r.to_string(),
                            };
                            break 'outer;
                        }
                    }
                }
                emit(AppEvent::ToolCompleted {
                    thread_id: thread_id.into(),
                    tool: call.name.clone(),
                    ok,
                    summary: serde_json::to_string(&payload)
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect(),
                });
                messages.push(ChatMessage::tool_result(&call.id, payload.to_string()));
            }

            // validator retry loop (plan §6.2 step 7): feed structured errors back
            if let Some(report) = &last_validation {
                if report.get("passed").and_then(|v| v.as_bool()) != Some(true) {
                    if retries < self.config.max_validator_retries {
                        retries += 1;
                        let retry_msg = format!(
                            "{}\n\nValidator report (fix ONLY the failed gates):\n{}",
                            self.presets.failure_recovery,
                            serde_json::to_string_pretty(report).unwrap_or_default()
                        );
                        messages.push(ChatMessage::user(retry_msg));
                    } else {
                        status = TurnStatus::ValidationExhausted {
                            failures: vec![serde_json::to_string(report).unwrap_or_default()],
                        };
                        break;
                    }
                }
            }
            if let Some(p) = proposal_this_round {
                last_proposal = Some(p);
            }
        }

        match &status {
            TurnStatus::Failed { error } => {
                emit(AppEvent::TurnFailed {
                    thread_id: thread_id.into(),
                    turn_id: turn_id.clone(),
                    error: error.clone(),
                });
            }
            _ => emit(AppEvent::TurnCompleted {
                thread_id: thread_id.into(),
                turn_id: turn_id.clone(),
            }),
        }

        TurnOutput {
            thread_id: thread_id.into(),
            turn_id,
            run_id,
            manifest,
            status,
            transcript,
            last_validation,
            last_proposal,
        }
    }
}
