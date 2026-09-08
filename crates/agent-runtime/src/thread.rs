//! Codex-shaped thread lifecycle: every thread owns a task running an event
//! loop over an Op queue (`UserTurn` / `Steer` / `Cancel` / `Shutdown`).
//!
//! - **Cancellable** — `Cancel` aborts the in-flight turn at the next await
//!   point (model stream, backoff sleep).
//! - **Steerable** — `Steer` mid-turn aborts the current model call, appends
//!   the clarification, and re-submits with all tool results so far.
//! - **Queued** — a `UserTurn` arriving mid-turn is queued for after the
//!   current turn; only `Steer`/`Cancel`/`Shutdown` interrupt.
//! - **Durable** — every message is offered to the observer (rollout);
//!   `spawn_thread_with_history` rehydrates a conversation after restart.
//! - **Budgeted** — the conversation is compacted before each model call
//!   (message-count AND char ceilings, tool pairs kept intact).
//! - **Turn-scoped patch state** — `patch_id` survives across model rounds:
//!   round N proposes, round N+1 validates *that stored patch by id*, and the
//!   approval refers to the very same patch row.

use crate::manifest::{build_manifest, core_contract_hash, new_run_id};
use crate::presets::PromptPresets;
use crate::turn::{apply_context_budget, RuntimeConfig, TurnStatus};
use agent_protocol::{AppEvent, EventBus};
use model_providers::{ChatMessage, Role, TurnRequest, TurnStreamEvent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use storyboard_domain::AgentRunManifest;
use storyboard_tools::{ToolBackend, ToolRegistry};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Operations a thread accepts (Codex `Op` equivalent).
#[derive(Debug, Clone)]
pub enum ThreadOp {
    UserTurn {
        text: String,
        project_id: Option<String>,
    },
    Steer {
        text: String,
    },
    Cancel,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadLifecycle {
    Idle,
    Running { turn_id: String, run_id: String },
    Stopped,
}

/// Persistence hook (F07 + rollout). Callbacks are best-effort: persistence
/// failures never kill a turn. Tool execution goes through the separately
/// attached `ToolBackend` (see `ThreadManager::new`).
pub trait RunObserver: Send + Sync {
    /// Called at turn start, BEFORE any event is emitted — the observer must
    /// persist the thread row here so events satisfy their foreign key.
    fn on_manifest(
        &self,
        _manifest: &AgentRunManifest,
        _thread_id: &str,
        _project_id: Option<&str>,
    ) {
    }
    fn on_event(&self, _thread_id: &str, _event: &AppEvent) {}
    /// Rollout append: every conversation message in order. The observer
    /// assigns the durable monotonic seq itself (call order == rollout order).
    fn on_message(&self, _thread_id: &str, _message: &ChatMessage) {}
    /// Auto-approval policy hit: persist the approval decision (conditional
    /// update `validated -> approved`) and report whether it landed. Only the
    /// Application Controller implements this — the runtime itself never
    /// writes patch state, and the model cannot call it.
    fn resolve_auto_approval(&self, _thread_id: &str, _project_id: &str, _patch_id: i64) -> bool {
        false
    }
}

/// No-op observer (tests that don't persist).
pub struct NoopObserver;
impl RunObserver for NoopObserver {}

#[derive(Clone)]
pub struct ThreadHandle {
    pub id: String,
    tx: mpsc::Sender<ThreadOp>,
    status: watch::Receiver<ThreadLifecycle>,
    result: Arc<Mutex<Option<TurnStatus>>>,
}

impl ThreadHandle {
    pub fn submit(&self, op: ThreadOp) {
        let _ = self.tx.blocking_send(op);
    }
    pub fn try_submit(&self, op: ThreadOp) -> Result<(), String> {
        self.tx.try_send(op).map_err(|e| e.to_string())
    }
    pub fn lifecycle(&self) -> ThreadLifecycle {
        self.status.borrow().clone()
    }
    pub fn last_result(&self) -> Option<TurnStatus> {
        self.result.lock().unwrap().clone()
    }
    pub fn clear_result(&self) {
        *self.result.lock().unwrap() = None;
    }
    /// Poll until the thread leaves the Running state (sync-context helper).
    pub fn wait_idle(&self) {
        loop {
            if !matches!(&*self.status.borrow(), ThreadLifecycle::Running { .. }) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub struct ThreadManager {
    pub rt: Arc<tokio::runtime::Runtime>,
    pub config: RuntimeConfig,
    pub registry: Arc<ToolRegistry>,
    pub presets: PromptPresets,
    pub provider: Arc<dyn model_providers::StoryboardModelProvider>,
    pub bus: Arc<EventBus>,
    pub observer: Arc<dyn RunObserver>,
    pub backend: Option<Arc<dyn ToolBackend>>,
    threads: Mutex<HashMap<String, ThreadHandle>>,
}

impl ThreadManager {
    pub fn new(
        config: RuntimeConfig,
        provider: Arc<dyn model_providers::StoryboardModelProvider>,
        bus: Arc<EventBus>,
        observer: Arc<dyn RunObserver>,
        backend: Option<Arc<dyn ToolBackend>>,
    ) -> Self {
        let registry = Arc::new(ToolRegistry::for_profile(config.profile));
        let presets = PromptPresets::v1();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");
        Self {
            rt: Arc::new(rt),
            config,
            registry,
            presets,
            provider,
            bus,
            observer,
            backend,
            threads: Mutex::new(HashMap::new()),
        }
    }

    /// Create (or return the existing) thread.
    pub fn spawn_thread(&self, id: &str) -> ThreadHandle {
        self.spawn_thread_with_history(id, Vec::new())
    }

    /// Create a thread rehydrated from a persisted rollout (durable sessions).
    /// The map insert happens under a single lock so two concurrent spawns of
    /// the same id cannot create two owning tasks.
    pub fn spawn_thread_with_history(&self, id: &str, history: Vec<ChatMessage>) -> ThreadHandle {
        let mut threads = self.threads.lock().unwrap();
        if let Some(h) = threads.get(id) {
            return h.clone();
        }
        let (tx, rx) = mpsc::channel::<ThreadOp>(64);
        let (status_tx, status_rx) = watch::channel(ThreadLifecycle::Idle);
        let result: Arc<Mutex<Option<TurnStatus>>> = Arc::new(Mutex::new(None));

        let ctx = Arc::new(ThreadCtx {
            config: self.config.clone(),
            registry: self.registry.clone(),
            presets: self.presets.clone(),
            provider: self.provider.clone(),
            bus: self.bus.clone(),
            observer: self.observer.clone(),
            backend: self.backend.clone(),
            rt: self.rt.clone(),
        });
        let handle = ThreadHandle {
            id: id.to_string(),
            tx: tx.clone(),
            status: status_rx,
            result: result.clone(),
        };
        let thread_id = id.to_string();
        self.rt.spawn(async move {
            thread_loop(ctx, thread_id, history, rx, status_tx, result).await;
        });
        threads.insert(id.to_string(), handle.clone());
        handle
    }

    pub fn get(&self, id: &str) -> Option<ThreadHandle> {
        self.threads.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<String> {
        self.threads.lock().unwrap().keys().cloned().collect()
    }

    /// Convenience for sync callers (tests/CLI): run one user turn to completion.
    pub fn run_turn_blocking(
        &self,
        thread_id: &str,
        project_id: Option<&str>,
        text: &str,
    ) -> TurnStatus {
        let handle = self.spawn_thread(thread_id);
        handle.clear_result();
        handle.submit(ThreadOp::UserTurn {
            text: text.to_string(),
            project_id: project_id.map(String::from),
        });
        // wait until a terminal result lands (submit is async-delivered, so
        // waiting on lifecycle alone would race before the turn starts)
        loop {
            if handle.last_result().is_some() {
                break;
            }
            if matches!(handle.lifecycle(), ThreadLifecycle::Stopped) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        handle.last_result().unwrap_or(TurnStatus::Failed {
            error: "thread stopped".into(),
        })
    }
}

/// Everything a thread task needs (cloned into each task).
struct ThreadCtx {
    config: RuntimeConfig,
    registry: Arc<ToolRegistry>,
    presets: PromptPresets,
    provider: Arc<dyn model_providers::StoryboardModelProvider>,
    bus: Arc<EventBus>,
    observer: Arc<dyn RunObserver>,
    backend: Option<Arc<dyn ToolBackend>>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl ThreadCtx {
    fn emit(&self, thread_id: &str, e: AppEvent) {
        self.bus.emit(e.clone());
        self.observer.on_event(thread_id, &e);
    }
}

/// The per-thread event loop.
async fn thread_loop(
    ctx: Arc<ThreadCtx>,
    thread_id: String,
    mut history: Vec<ChatMessage>,
    mut rx: mpsc::Receiver<ThreadOp>,
    status_tx: watch::Sender<ThreadLifecycle>,
    result: Arc<Mutex<Option<TurnStatus>>>,
) {
    let mut pending: Vec<ThreadOp> = Vec::new();

    #[allow(unused_labels)]
    'outer: loop {
        let op = if !pending.is_empty() {
            pending.remove(0)
        } else {
            match rx.recv().await {
                Some(op) => op,
                None => break,
            }
        };
        match op {
            ThreadOp::Shutdown => break,
            // outside a running turn there is nothing to cancel / steer;
            // a steer while idle is treated as a fresh user turn
            ThreadOp::Cancel => {}
            ThreadOp::Steer { text } => {
                pending.push(ThreadOp::UserTurn {
                    text,
                    project_id: None,
                });
            }
            ThreadOp::UserTurn { text, project_id } => {
                let outcome = run_user_turn(
                    &ctx,
                    &thread_id,
                    &mut history,
                    &mut rx,
                    &mut pending,
                    &project_id,
                    text,
                    &status_tx,
                )
                .await;
                *result.lock().unwrap() = Some(outcome.clone());
                status_tx.send_replace(ThreadLifecycle::Idle);
                ctx.emit(
                    &thread_id,
                    AppEvent::ThreadIdle {
                        thread_id: thread_id.clone(),
                    },
                );
                if matches!(outcome, TurnStatus::Cancelled)
                    && pending.iter().any(|p| matches!(p, ThreadOp::Shutdown))
                {
                    pending.retain(|p| !matches!(p, ThreadOp::Shutdown));
                    break 'outer;
                }
            }
        }
    }
    status_tx.send_replace(ThreadLifecycle::Stopped);
}

/// How the model call ended (or was interrupted).
enum ModelOutcome {
    Resp(Result<model_providers::TurnResponse, model_providers::ProviderError>),
    Steered,
    Cancelled,
}

/// Execute one user turn: model loop with tool dispatch, validator retry,
/// approval, steering and cancellation.
#[allow(clippy::too_many_arguments)]
async fn run_user_turn(
    ctx: &ThreadCtx,
    thread_id: &str,
    history: &mut Vec<ChatMessage>,
    rx: &mut mpsc::Receiver<ThreadOp>,
    pending: &mut Vec<ThreadOp>,
    project_id: &Option<String>,
    text: String,
    status_tx: &watch::Sender<ThreadLifecycle>,
) -> TurnStatus {
    let turn_id = agent_protocol::new_id("turn");
    let run_id = new_run_id();
    status_tx.send_replace(ThreadLifecycle::Running {
        turn_id: turn_id.clone(),
        run_id: run_id.clone(),
    });

    // manifest at turn start (F07) — BEFORE any model call and BEFORE the
    // first event, so the observer can create the agent_threads row first
    // (agent_events carries a FK to it).
    let backend = ctx.backend.clone();
    let project_ctx: Option<(String, String)> = match (project_id, &backend) {
        (Some(pid), Some(b)) => b.read_project(pid).ok().and_then(|state| {
            let rev = state.get("source_revision_id")?.as_str()?.to_string();
            let ver = state.get("current_version")?.to_string();
            Some((rev, ver))
        }),
        _ => None,
    };
    let (t_rev, base_v) = match project_ctx {
        Some((r, v)) => (Some(r), Some(v)),
        None => (None, None),
    };
    let manifest = build_manifest(
        &run_id,
        ctx.provider.id(),
        ctx.provider.model(),
        &ctx.presets.version,
        &core_contract_hash(&ctx.presets.core_contract),
        ctx.registry.version(),
        t_rev.as_deref(),
        base_v.as_deref(),
        &serde_json::to_value(&ctx.config.sampling).unwrap_or_default(),
    );
    ctx.observer
        .on_manifest(&manifest, thread_id, project_id.as_deref());
    ctx.emit(
        thread_id,
        AppEvent::TurnStarted {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.clone(),
            run_id: run_id.clone(),
        },
    );
    ctx.emit(
        thread_id,
        AppEvent::AgentRunManifestCreated {
            run_id: run_id.clone(),
        },
    );

    // system prompt once per conversation
    if !matches!(history.first().map(|m| &m.role), Some(Role::System)) {
        let task = match project_id {
            Some(pid) => format!(
                "Project {pid}. Use the tools to inspect, then propose a typed storyboard patch. {}",
                ctx.presets.character_replace
            ),
            None => "Find the best Primary Template with search_templates.".into(),
        };
        history.insert(0, ChatMessage::system(ctx.presets.system_prompt(&task)));
        ctx.observer.on_message(thread_id, &history[0]);
    }
    history.push(ChatMessage::user(text));
    ctx.observer.on_message(thread_id, history.last().unwrap());

    let mut outcome = TurnStatus::Failed {
        error: "no provider response".into(),
    };
    let mut validator_retries = 0usize;

    // ---- turn-scoped patch state: survives across model rounds ----
    // Round N may propose (storing patch_id); round N+1 validates THAT patch
    // by id. Resetting these per round orphaned the approval from the stored
    // patch row — the exact bug where patch_id became -1 after validation.
    let mut turn_patch_id: i64 = -1;
    let mut turn_proposal: Option<serde_json::Value> = None;
    let mut turn_report: Option<serde_json::Value> = None;

    'turn: for _round in 0..ctx.config.max_tool_rounds {
        // fresh cancellation scope per round (steer cancels the old one)
        let round_cancel = CancellationToken::new();

        // context budget before every (re)submission
        if let Some(stats) = apply_context_budget(history, &ctx.config.budget) {
            ctx.emit(
                thread_id,
                AppEvent::MessageDelta {
                    thread_id: thread_id.to_string(),
                    text: format!("[context compacted: -{} messages]", stats.removed_messages),
                },
            );
        }

        let (delta_tx, mut delta_rx) = mpsc::channel::<TurnStreamEvent>(256);
        let req = TurnRequest {
            messages: history.clone(),
            tools: ctx.registry.schemas_for_provider(),
            sampling: ctx.config.sampling.clone(),
            force_json: false,
            stream: true,
        };

        // pump model deltas to the event stream while the model works
        let pump = {
            let thread_id = thread_id.to_string();
            let bus = ctx.bus.clone();
            let obs = ctx.observer.clone();
            ctx.rt.spawn(async move {
                while let Some(ev) = delta_rx.recv().await {
                    if let TurnStreamEvent::Delta { text } = ev {
                        let e = AppEvent::MessageDelta {
                            thread_id: thread_id.clone(),
                            text,
                        };
                        bus.emit(e.clone());
                        obs.on_event(&thread_id, &e);
                    }
                }
            })
        };

        // The model call runs as its OWN task: a queued UserTurn can arrive
        // without dropping the in-flight request (select! drops the losing
        // futures — owning the future inline forced the old code to cancel).
        let model_cancel = round_cancel.child_token();
        let provider = ctx.provider.clone();
        let mut model_task = ctx
            .rt
            .spawn(async move { provider.run_turn(req, model_cancel, delta_tx).await });

        let model_outcome = loop {
            tokio::select! {
                op = rx.recv() => {
                    match op {
                        Some(ThreadOp::Steer { text }) => {
                            round_cancel.cancel(); // abort in-flight model call
                            pump.abort();
                            let _ = (&mut model_task).await;
                            history.push(ChatMessage::user(format!("[steer] {text}")));
                            ctx.observer.on_message(thread_id, history.last().unwrap());
                            break ModelOutcome::Steered; // re-submit with the clarification appended
                        }
                        Some(ThreadOp::Cancel) => {
                            round_cancel.cancel();
                            pump.abort();
                            let _ = (&mut model_task).await;
                            break ModelOutcome::Cancelled;
                        }
                        Some(ThreadOp::Shutdown) => {
                            round_cancel.cancel();
                            pump.abort();
                            let _ = (&mut model_task).await;
                            pending.clear();
                            pending.push(ThreadOp::Shutdown);
                            break ModelOutcome::Cancelled;
                        }
                        Some(ut @ ThreadOp::UserTurn { .. }) => {
                            pending.push(ut); // FIFO queue; the current turn keeps running
                        }
                        None => {
                            round_cancel.cancel();
                            pump.abort();
                            let _ = (&mut model_task).await;
                            break ModelOutcome::Cancelled;
                        }
                    }
                }
                r = &mut model_task => {
                    let _ = pump.await; // drain remaining deltas
                    break ModelOutcome::Resp(r.map_err(|e| model_providers::ProviderError::Http(e.to_string())).and_then(|r| r));
                }
            }
        };

        let resp = match model_outcome {
            ModelOutcome::Steered => continue 'turn,
            ModelOutcome::Cancelled => {
                outcome = TurnStatus::Cancelled;
                break 'turn;
            }
            ModelOutcome::Resp(Ok(resp)) => resp,
            ModelOutcome::Resp(Err(model_providers::ProviderError::Cancelled)) => {
                outcome = TurnStatus::Cancelled;
                break 'turn;
            }
            ModelOutcome::Resp(Err(e)) => {
                outcome = TurnStatus::Failed {
                    error: e.to_string(),
                };
                break 'turn;
            }
        };

        let assistant = resp.message.clone();
        history.push(assistant.clone());
        ctx.observer.on_message(thread_id, history.last().unwrap());

        if assistant.tool_calls.is_empty() {
            outcome = TurnStatus::Completed {
                reply: assistant.content.clone(),
            };
            break 'turn;
        }

        // execute tools (sync backend; ms-level DB calls are acceptable on
        // this worker thread)
        for call in &assistant.tool_calls {
            ctx.emit(
                thread_id,
                AppEvent::ToolStarted {
                    thread_id: thread_id.to_string(),
                    tool: call.name.clone(),
                },
            );
            let args: serde_json::Value =
                serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::Value::Null);
            let tool_result = match &backend {
                Some(b) => ctx
                    .registry
                    .dispatch(&call.name, &args, Some(&run_id), b.as_ref())
                    .map_err(|e| e.to_string()),
                None => Err("no tool backend attached".to_string()),
            };
            let mut payload = match tool_result {
                Ok(v) => v,
                Err(msg) => serde_json::json!({ "error": msg }),
            };
            if call.name == "propose_storyboard_patch" {
                if let Some(p) = args.get("proposal").cloned() {
                    turn_proposal = Some(p);
                }
                if let Some(pid) = payload.get("patch_id").and_then(|v| v.as_i64()) {
                    turn_patch_id = pid;
                    turn_report = payload.get("report").cloned();
                }
            }
            if call.name == "validate_storyboard_patch" {
                let by_id = args.get("patch_id").and_then(|v| v.as_i64());
                // By-id validation of the turn's stored patch is the
                // authoritative path; a re-submitted proposal only counts
                // when no stored patch exists yet.
                let validated: Result<serde_json::Value, String> = match (by_id, project_id.as_deref()) {
                    (Some(id), Some(p)) if turn_patch_id <= 0 || id == turn_patch_id => backend
                        .as_ref()
                        .map(|b| b.validate_patch_by_id(p, id))
                        .unwrap_or_else(|| Err("no tool backend".into())),
                    (Some(id), _) => Err(format!(
                        "patch_id {id} does not match this turn's stored patch {turn_patch_id}; validate the patch you proposed"
                    )),
                    _ => Ok(payload.clone()),
                };
                match validated {
                    Ok(v) => {
                        if let Some(rep) = v.get("report").cloned() {
                            turn_report = Some(rep);
                        }
                        if turn_patch_id <= 0 {
                            turn_patch_id = by_id.unwrap_or(-1);
                        }
                        payload = v; // the model sees the authoritative report
                    }
                    Err(msg) => {
                        payload = serde_json::json!({ "error": msg });
                    }
                }
            }
            ctx.emit(
                thread_id,
                AppEvent::ToolCompleted {
                    thread_id: thread_id.to_string(),
                    tool: call.name.clone(),
                    ok: payload.get("error").is_none(),
                    summary: serde_json::to_string(&payload)
                        .unwrap_or_default()
                        .chars()
                        .take(200)
                        .collect(),
                },
            );
            let tm = ChatMessage::tool_result(&call.id, payload.to_string());
            history.push(tm);
            ctx.observer.on_message(thread_id, history.last().unwrap());
        }

        // approval path
        if let Some(report) = &turn_report {
            let passed = report
                .get("passed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ctx.emit(
                thread_id,
                AppEvent::ValidatorCompleted {
                    thread_id: thread_id.to_string(),
                    passed,
                    report_json: report.clone(),
                },
            );
            if passed && turn_patch_id > 0 {
                let proposal = turn_proposal.clone().unwrap_or_default();
                let (auto, risk) = ctx.config.approval.decide(&proposal);
                ctx.emit(
                    thread_id,
                    AppEvent::ApprovalRequested {
                        thread_id: thread_id.to_string(),
                        patch_id: turn_patch_id,
                        risk: risk.to_string(),
                    },
                );
                let mut auto_landed = false;
                if auto {
                    // the controller persists the decision; the runtime only
                    // claims success when the DB update actually landed
                    auto_landed = match project_id.as_deref() {
                        Some(pid) => {
                            ctx.observer
                                .resolve_auto_approval(thread_id, pid, turn_patch_id)
                        }
                        None => false,
                    };
                    ctx.emit(
                        thread_id,
                        AppEvent::ApprovalResolved {
                            thread_id: thread_id.to_string(),
                            patch_id: turn_patch_id,
                            approved: auto_landed,
                        },
                    );
                }
                outcome = TurnStatus::NeedsApproval {
                    patch_id: turn_patch_id,
                    auto_approved: auto_landed,
                    risk: risk.to_string(),
                };
                break 'turn;
            }
            if passed && turn_patch_id <= 0 {
                // validation passed on an un-stored proposal (e.g. previewed
                // but never proposed) — tell the model to propose it first
                if validator_retries < ctx.config.max_validator_retries {
                    validator_retries += 1;
                    history.push(ChatMessage::user(
                        "Validation passed but no stored patch exists. Call propose_storyboard_patch with the same proposal, then validate by its patch_id.".to_string(),
                    ));
                    ctx.observer.on_message(thread_id, history.last().unwrap());
                    continue 'turn;
                }
            }
            // validator retry: feed structured errors back (plan §6.2 step 7)
            if validator_retries < ctx.config.max_validator_retries {
                validator_retries += 1;
                let retry = format!(
                    "{}\n\nValidator report (fix ONLY the failed gates):\n{}",
                    ctx.presets.failure_recovery,
                    serde_json::to_string_pretty(report).unwrap_or_default()
                );
                history.push(ChatMessage::user(retry));
                ctx.observer.on_message(thread_id, history.last().unwrap());
            } else {
                outcome = TurnStatus::ValidationExhausted {
                    failures: vec![serde_json::to_string(report).unwrap_or_default()],
                };
                break 'turn;
            }
        }
    }

    match &outcome {
        TurnStatus::Cancelled => {
            ctx.emit(
                thread_id,
                AppEvent::TurnCancelled {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.clone(),
                },
            );
        }
        TurnStatus::Failed { error } => {
            ctx.emit(
                thread_id,
                AppEvent::TurnFailed {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.clone(),
                    error: error.clone(),
                },
            );
        }
        _ => {
            ctx.emit(
                thread_id,
                AppEvent::TurnCompleted {
                    thread_id: thread_id.to_string(),
                    turn_id: turn_id.clone(),
                },
            );
        }
    }
    outcome
}
