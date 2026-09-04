//! Lifecycle 2.0 verification: cancellation, steering, durable resume,
//! streaming deltas — the Codex-parity capabilities.

use agent_protocol::{AppEvent, EventBus};
use agent_runtime::{NoopObserver, RuntimeConfig, ThreadLifecycle, ThreadManager, ThreadOp, TurnStatus};
use model_providers::{MockProvider, TurnResponse};
use std::sync::{Arc, Mutex};

/// Observer that records every message (rollout) + emits nothing.
#[derive(Default)]
struct RecordingObserver {
    messages: Mutex<Vec<(String, String)>>, // (thread_id, content)
}

impl agent_runtime::RunObserver for RecordingObserver {
    fn on_message(&self, thread_id: &str, _seq: usize, message: &model_providers::ChatMessage) {
        self.messages
            .lock()
            .unwrap()
            .push((thread_id.to_string(), message.content.clone()));
    }
}

fn manager_with(provider: MockProvider, observer: Arc<dyn agent_runtime::RunObserver>) -> ThreadManager {
    ThreadManager::new(
        RuntimeConfig::default(),
        Arc::new(provider),
        Arc::new(EventBus::new()),
        observer,
        None,
    )
}

fn wait_result(h: &agent_runtime::ThreadHandle) -> TurnStatus {
    loop {
        if let Some(r) = h.last_result() {
            return r;
        }
        if matches!(h.lifecycle(), ThreadLifecycle::Stopped) {
            return TurnStatus::Failed { error: "stopped".into() };
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn cancel_aborts_hanging_turn() {
    let m = manager_with(MockProvider::hanging(), Arc::new(NoopObserver));
    let h = m.spawn_thread("t-cancel");
    h.submit(ThreadOp::UserTurn { text: "go".into(), project_id: None });
    // wait until the turn is actually running, then cancel
    loop {
        if matches!(h.lifecycle(), ThreadLifecycle::Running { .. }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    h.submit(ThreadOp::Cancel);
    let outcome = wait_result(&h);
    assert_eq!(outcome, TurnStatus::Cancelled);
    // thread survives cancellation: back to Idle, still accepting ops
    assert!(matches!(h.lifecycle(), ThreadLifecycle::Idle));
    assert!(h.try_submit(ThreadOp::Steer { text: "queued after cancel".into() }).is_ok());
}

#[test]
fn steer_mid_turn_reroutes_the_model() {
    // First response is slow (long text, big chunk delay) so the steer lands
    // while it streams; the steered retry consumes the next script entry.
    let mut provider = MockProvider::new(vec![
        TurnResponse { message: model_providers::ChatMessage::assistant("initial answer that streams slowly word by word"), finish_reason: "stop".into(), usage: None },
        TurnResponse { message: model_providers::ChatMessage::assistant("steered answer"), finish_reason: "stop".into(), usage: None },
    ]);
    provider.chunk_delay = std::time::Duration::from_millis(150);
    let observer = Arc::new(RecordingObserver::default());
    let m = ThreadManager::new(
        RuntimeConfig::default(),
        Arc::new(provider),
        Arc::new(EventBus::new()),
        observer.clone(),
        None,
    );
    // make the first model call slow by overriding after construction is not
    // possible; instead rely on chunk_delay of the constructed provider.
    let h = m.spawn_thread("t-steer");
    h.submit(ThreadOp::UserTurn { text: "first".into(), project_id: None });
    loop {
        if matches!(h.lifecycle(), ThreadLifecycle::Running { .. }) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.submit(ThreadOp::Steer { text: "wait, do it differently".into() });
    let outcome = wait_result(&h);
    match &outcome {
        TurnStatus::Completed { reply } => assert_eq!(reply, "steered answer"),
        other => panic!("expected steered completion, got {other:?}"),
    }
    let msgs = observer.messages.lock().unwrap();
    assert!(
        msgs.iter().any(|(_, c)| c.contains("[steer] wait, do it differently")),
        "steer text must be appended to the conversation: {msgs:?}"
    );
}

#[test]
fn resume_rehydrates_history() {
    let observer = Arc::new(RecordingObserver::default());
    let provider = MockProvider::simple_text("continued");
    let m = ThreadManager::new(
        RuntimeConfig::default(),
        Arc::new(provider),
        Arc::new(EventBus::new()),
        observer.clone(),
        None,
    );
    let prior = vec![
        model_providers::ChatMessage::system("previous system prompt"),
        model_providers::ChatMessage::user("previous question"),
        model_providers::ChatMessage::assistant("previous answer"),
    ];
    let h = m.spawn_thread_with_history("t-resume", prior);
    h.submit(ThreadOp::UserTurn { text: "continue".into(), project_id: None });
    let outcome = wait_result(&h);
    assert!(matches!(outcome, TurnStatus::Completed { .. }), "{outcome:?}");
    // history kept: the prior system prompt is REUSED — no new preset system
    // message may appear (a fresh insertion would contain CORE_CONTRACT).
    let msgs = observer.messages.lock().unwrap();
    assert!(
        !msgs.iter().any(|(_, c)| c.contains("CORE_CONTRACT")),
        "system prompt must be reused, not duplicated: {msgs:?}"
    );
    assert!(msgs.iter().any(|(_, c)| c == "continue"));
}

#[test]
fn deltas_stream_to_the_event_bus() {
    let bus = Arc::new(EventBus::new());
    let rx = bus.subscribe();
    let provider = MockProvider::simple_text("one two three four five");
    let m = ThreadManager::new(
        RuntimeConfig::default(),
        Arc::new(provider),
        bus,
        Arc::new(NoopObserver),
        None,
    );
    let h = m.spawn_thread("t-stream");
    h.submit(ThreadOp::UserTurn { text: "hi".into(), project_id: None });
    let outcome = wait_result(&h);
    assert!(matches!(outcome, TurnStatus::Completed { .. }));
    let mut deltas = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if let AppEvent::MessageDelta { text, .. } = e {
            if !text.is_empty() && !text.starts_with("[context") {
                deltas.push(text);
            }
        }
    }
    assert_eq!(deltas.len(), 5, "five word-level deltas expected: {deltas:?}");
    assert_eq!(deltas.join("").trim(), "one two three four five");
}
