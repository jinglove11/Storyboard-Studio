//! Typed in-process protocol: events emitted by the app server / agent
//! runtime, consumed by the Tauri bridge (plan §17.1). No HTTP, no local
//! server — channels only.

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;

/// Wire contract note: the serde tag of every variant MUST equal
/// `type_name()` — the frontend listens on these dotted names, and the
/// `wire_names_match_type_names` test pins the two together so they can
/// never drift again.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AppEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: String },
    #[serde(rename = "thread.resumed")]
    ThreadResumed { thread_id: String },
    #[serde(rename = "turn.started")]
    TurnStarted {
        thread_id: String,
        turn_id: String,
        run_id: String,
    },
    #[serde(rename = "turn.completed")]
    TurnCompleted { thread_id: String, turn_id: String },
    #[serde(rename = "turn.failed")]
    TurnFailed {
        thread_id: String,
        turn_id: String,
        error: String,
    },
    #[serde(rename = "turn.cancelled")]
    TurnCancelled { thread_id: String, turn_id: String },
    #[serde(rename = "thread.idle")]
    ThreadIdle { thread_id: String },
    #[serde(rename = "tool.started")]
    ToolStarted { thread_id: String, tool: String },
    #[serde(rename = "tool.completed")]
    ToolCompleted {
        thread_id: String,
        tool: String,
        ok: bool,
        summary: String,
    },
    #[serde(rename = "template.match.updated")]
    TemplateMatchUpdated {
        thread_id: String,
        selection_json: serde_json::Value,
    },
    #[serde(rename = "patch.proposed")]
    PatchProposed {
        thread_id: String,
        project_id: String,
        operation_count: usize,
    },
    #[serde(rename = "validator.completed")]
    ValidatorCompleted {
        thread_id: String,
        passed: bool,
        report_json: serde_json::Value,
    },
    #[serde(rename = "approval.requested")]
    ApprovalRequested {
        thread_id: String,
        patch_id: i64,
        risk: String,
    },
    #[serde(rename = "approval.resolved")]
    ApprovalResolved {
        thread_id: String,
        patch_id: i64,
        approved: bool,
    },
    #[serde(rename = "patch.commit.requested")]
    PatchCommitRequested { thread_id: String, patch_id: i64 },
    #[serde(rename = "patch.commit.completed")]
    PatchCommitCompleted { thread_id: String, new_version: u64 },
    #[serde(rename = "patch.commit.failed")]
    PatchCommitFailed { thread_id: String, reason: String },
    #[serde(rename = "project.version.created")]
    ProjectVersionCreated { project_id: String, version: u64 },
    #[serde(rename = "agent.run.manifest.created")]
    AgentRunManifestCreated { run_id: String },
    #[serde(rename = "export.completed")]
    ExportCompleted { project_id: String, path: String },
    #[serde(rename = "message.delta")]
    MessageDelta { thread_id: String, text: String },
}

impl AppEvent {
    pub fn type_name(&self) -> &'static str {
        match self {
            AppEvent::ThreadStarted { .. } => "thread.started",
            AppEvent::ThreadResumed { .. } => "thread.resumed",
            AppEvent::TurnStarted { .. } => "turn.started",
            AppEvent::TurnCompleted { .. } => "turn.completed",
            AppEvent::TurnFailed { .. } => "turn.failed",
            AppEvent::TurnCancelled { .. } => "turn.cancelled",
            AppEvent::ThreadIdle { .. } => "thread.idle",
            AppEvent::ToolStarted { .. } => "tool.started",
            AppEvent::ToolCompleted { .. } => "tool.completed",
            AppEvent::TemplateMatchUpdated { .. } => "template.match.updated",
            AppEvent::PatchProposed { .. } => "patch.proposed",
            AppEvent::ValidatorCompleted { .. } => "validator.completed",
            AppEvent::ApprovalRequested { .. } => "approval.requested",
            AppEvent::ApprovalResolved { .. } => "approval.resolved",
            AppEvent::PatchCommitRequested { .. } => "patch.commit.requested",
            AppEvent::PatchCommitCompleted { .. } => "patch.commit.completed",
            AppEvent::PatchCommitFailed { .. } => "patch.commit.failed",
            AppEvent::ProjectVersionCreated { .. } => "project.version.created",
            AppEvent::AgentRunManifestCreated { .. } => "agent.run.manifest.created",
            AppEvent::ExportCompleted { .. } => "export.completed",
            AppEvent::MessageDelta { .. } => "message.delta",
        }
    }
}

/// Fan-out event bus. Subscribers are std mpsc receivers; the Tauri bridge
/// forwards each event to the webview.
#[derive(Debug, Default)]
pub struct EventBus {
    subscribers: Mutex<Vec<Sender<AppEvent>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self) -> Receiver<AppEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    pub fn emit(&self, event: AppEvent) {
        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|s| s.send(event.clone()).is_ok());
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_delivers_to_subscribers() {
        let bus = EventBus::new();
        let rx1 = bus.subscribe();
        let rx2 = bus.subscribe();
        bus.emit(AppEvent::ThreadStarted {
            thread_id: "t1".into(),
        });
        assert!(matches!(rx1.recv(), Ok(AppEvent::ThreadStarted { .. })));
        assert!(matches!(rx2.recv(), Ok(AppEvent::ThreadStarted { .. })));
    }

    /// Wire contract: the serde tag of every event must equal its dotted
    /// `type_name()` — the React listener switches on these names.
    #[test]
    fn wire_names_match_type_names() {
        let samples = vec![
            AppEvent::ThreadStarted {
                thread_id: "t".into(),
            },
            AppEvent::ThreadResumed {
                thread_id: "t".into(),
            },
            AppEvent::TurnStarted {
                thread_id: "t".into(),
                turn_id: "x".into(),
                run_id: "r".into(),
            },
            AppEvent::TurnCompleted {
                thread_id: "t".into(),
                turn_id: "x".into(),
            },
            AppEvent::TurnFailed {
                thread_id: "t".into(),
                turn_id: "x".into(),
                error: "e".into(),
            },
            AppEvent::TurnCancelled {
                thread_id: "t".into(),
                turn_id: "x".into(),
            },
            AppEvent::ThreadIdle {
                thread_id: "t".into(),
            },
            AppEvent::ToolStarted {
                thread_id: "t".into(),
                tool: "g".into(),
            },
            AppEvent::ToolCompleted {
                thread_id: "t".into(),
                tool: "g".into(),
                ok: true,
                summary: "s".into(),
            },
            AppEvent::TemplateMatchUpdated {
                thread_id: "t".into(),
                selection_json: serde_json::Value::Null,
            },
            AppEvent::PatchProposed {
                thread_id: "t".into(),
                project_id: "p".into(),
                operation_count: 1,
            },
            AppEvent::ValidatorCompleted {
                thread_id: "t".into(),
                passed: true,
                report_json: serde_json::Value::Null,
            },
            AppEvent::ApprovalRequested {
                thread_id: "t".into(),
                patch_id: 1,
                risk: "low".into(),
            },
            AppEvent::ApprovalResolved {
                thread_id: "t".into(),
                patch_id: 1,
                approved: true,
            },
            AppEvent::PatchCommitRequested {
                thread_id: "t".into(),
                patch_id: 1,
            },
            AppEvent::PatchCommitCompleted {
                thread_id: "t".into(),
                new_version: 2,
            },
            AppEvent::PatchCommitFailed {
                thread_id: "t".into(),
                reason: "r".into(),
            },
            AppEvent::ProjectVersionCreated {
                project_id: "p".into(),
                version: 2,
            },
            AppEvent::AgentRunManifestCreated { run_id: "r".into() },
            AppEvent::ExportCompleted {
                project_id: "p".into(),
                path: "/x".into(),
            },
            AppEvent::MessageDelta {
                thread_id: "t".into(),
                text: "hi".into(),
            },
        ];
        assert_eq!(samples.len(), 21);
        for e in samples {
            let v = serde_json::to_value(&e).unwrap();
            let tag = v
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("<missing>");
            assert_eq!(tag, e.type_name(), "serde tag drifted from type_name()");
        }
    }
}
