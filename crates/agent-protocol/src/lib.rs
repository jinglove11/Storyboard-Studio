//! Typed in-process protocol: events emitted by the app server / agent
//! runtime, consumed by the Tauri bridge (plan §17.1). No HTTP, no local
//! server — channels only.

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    ThreadStarted { thread_id: String },
    ThreadResumed { thread_id: String },
    TurnStarted { thread_id: String, turn_id: String, run_id: String },
    TurnCompleted { thread_id: String, turn_id: String },
    TurnFailed { thread_id: String, turn_id: String, error: String },
    ToolStarted { thread_id: String, tool: String },
    ToolCompleted { thread_id: String, tool: String, ok: bool, summary: String },
    TemplateMatchUpdated { thread_id: String, selection_json: serde_json::Value },
    PatchProposed { thread_id: String, project_id: String, operation_count: usize },
    ValidatorCompleted { thread_id: String, passed: bool, report_json: serde_json::Value },
    ApprovalRequested { thread_id: String, patch_id: i64, risk: String },
    ApprovalResolved { thread_id: String, patch_id: i64, approved: bool },
    PatchCommitRequested { thread_id: String, patch_id: i64 },
    PatchCommitCompleted { thread_id: String, new_version: u64 },
    PatchCommitFailed { thread_id: String, reason: String },
    ProjectVersionCreated { project_id: String, version: u64 },
    AgentRunManifestCreated { run_id: String },
    ExportCompleted { project_id: String, path: String },
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
        bus.emit(AppEvent::ThreadStarted { thread_id: "t1".into() });
        assert!(matches!(rx1.recv(), Ok(AppEvent::ThreadStarted { .. })));
        assert!(matches!(rx2.recv(), Ok(AppEvent::ThreadStarted { .. })));
    }
}
