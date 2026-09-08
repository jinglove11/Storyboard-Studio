//! Tauri bridge: thin commands over the in-process AppServer (plan §17 —
//! React ⇄ Tauri IPC ⇄ UI Facade ⇄ App Server). No domain logic here.
//!
//! Concurrency model: `Arc<AppServer>` is lock-free for callers (Db has its
//! own short-lived mutex per call). Agent turns run on background threads so
//! a slow provider never blocks UI commands; results and per-event telemetry
//! flow back through `app.emit`.
//!
//! Lifecycle truth: the agent turn's result IS the stored patch —
//! `agent_thread_result` returns that patch's detail (proposal, validation,
//! preview diff). Nothing is re-synthesized after the fact.

use app_server::AppServer;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use storyboard_domain::ProjectId;
use storyboard_importer::skill::SkillBundle;
use tauri::{Emitter, Manager};

pub const EVT_AGENT_EVENT: &str = "sbx://agent-event";

/// Locate the default skill bundle without assuming the process CWD is the
/// repo root (packaged installs, launcher launches, changed directories).
/// Probe order: env override → CWD candidates → Tauri resource dir → exe
/// dir. Returns None when absent — the app starts EMPTY and the user
/// imports via the UI (a missing fixture must not panic the app).
pub fn probe_skill_path(resource_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SBX_SKILL_PATH") {
        let p = PathBuf::from(p);
        if p.join("references/template-index.json").is_file() {
            return Some(p);
        }
    }
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from("fixtures/current-skill"),
        PathBuf::from("../../fixtures/current-skill"),
    ];
    if let Some(rd) = resource_dir {
        candidates.push(rd.join("fixtures/current-skill"));
        candidates.push(rd.join("current-skill"));
        candidates.push(rd.join("../../fixtures/current-skill"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("fixtures/current-skill"));
            candidates.push(exe_dir.join("../../fixtures/current-skill"));
            candidates.push(exe_dir.join("../../../fixtures/current-skill"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.join("references/template-index.json").is_file())
}

/// Cross-platform default workspace: Tauri app-data dir (respects
/// %APPDATA% / ~/Library/Application Support / $XDG_DATA_HOME). Env
/// override and repo-relative fallback keep `cargo run` workflows working.
pub fn default_workspace(app_data: Option<&std::path::Path>) -> PathBuf {
    if let Ok(p) = std::env::var("SBX_WORKSPACE") {
        return PathBuf::from(p);
    }
    if let Some(d) = app_data {
        return d.join("StoryboardStudio").join("workspace");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/StoryboardStudio/workspace")
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_data_dir().ok();
            let resource_dir = app.path().resource_dir().ok();
            let workspace = default_workspace(app_data.as_deref());
            let server = if workspace.join("database").is_dir() {
                AppServer::open(&workspace).expect("open workspace (database unreadable)")
            } else {
                // empty start is fine — the user imports templates via the UI
                let server = AppServer::init_empty(&workspace).expect("init workspace");
                if let Some(skill_path) = probe_skill_path(resource_dir.as_deref()) {
                    if let Ok(skill) = SkillBundle::open_dir(&skill_path)
                        .or_else(|_| SkillBundle::open_zip(skill_path.with_extension("skill")))
                    {
                        let _ = server.import_skill(&skill);
                    }
                }
                server
            };
            let server = Arc::new(server);

            // §17 bridge: forward every app-server event to the webview.
            let rx = server.bus.subscribe();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    let _ = handle.emit(EVT_AGENT_EVENT, &event);
                }
            });
            app.manage(server);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace_info,
            list_templates,
            import_skill,
            parse_intent,
            match_templates,
            clone_project,
            list_projects,
            project_versions,
            patch_detail,
            quick_identity_swap,
            approve_patch,
            reject_patch,
            commit_patch,
            rollback,
            export_project,
            persistence_warnings,
            agent_provider_status,
            agent_start,
            agent_steer,
            agent_cancel,
            agent_thread_result,
            providers_list,
            provider_save,
            provider_delete,
            provider_set_api_key,
            provider_test,
            provider_activate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn workspace_info(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    Ok(json!({
        "root": server.workspace.root.display().to_string(),
        "templates": server.template_metadata().map_err(|e| e.to_string())?.len(),
    }))
}

#[tauri::command]
fn list_templates(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    serde_json::to_value(server.template_metadata().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Import a skill bundle (directory or .skill zip) chosen by the user.
#[tauri::command]
fn import_skill(
    server: tauri::State<Arc<AppServer>>,
    path: String,
) -> Result<serde_json::Value, String> {
    let p = PathBuf::from(&path);
    let skill = SkillBundle::open_dir(&p)
        .or_else(|_| SkillBundle::open_zip(&p))
        .map_err(|e| format!("open skill bundle at {path}: {e}"))?;
    let summary = server.import_skill(&skill).map_err(|e| e.to_string())?;
    serde_json::to_value(summary).map_err(|e| e.to_string())
}

#[tauri::command]
fn parse_intent(
    server: tauri::State<Arc<AppServer>>,
    text: String,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(server.parse_intent(&text)).map_err(|e| e.to_string())
}

#[tauri::command]
fn match_templates(
    server: tauri::State<Arc<AppServer>>,
    text: String,
    seed: Option<u64>,
) -> Result<serde_json::Value, String> {
    let intent = server.parse_intent(&text);
    let sel = server
        .match_templates(&intent, seed)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(sel).map_err(|e| e.to_string())
}

#[tauri::command]
fn clone_project(
    server: tauri::State<Arc<AppServer>>,
    template_id: String,
    title: Option<String>,
    seed: Option<u64>,
) -> Result<serde_json::Value, String> {
    let st = server
        .clone_project(&template_id, title, seed.unwrap_or(42))
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "id": st.project_id.to_string(),
        "title": st.title,
        "source_template_id": st.source_template_id.as_str(),
        "current_version": st.current_version,
        "status": st.status,
        "created_at": st.created_at,
        "updated_at": st.updated_at,
    }))
}

#[tauri::command]
fn list_projects(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    let rows = server.db.list_projects().map_err(|e| e.to_string())?;
    let out: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id, "title": r.title, "source_template_id": r.source_template_id,
                "current_version": r.current_version, "status": r.status,
                "created_at": r.created_at, "updated_at": r.updated_at,
            })
        })
        .collect();
    Ok(json!(out))
}

/// Full version rows: number, parent, created_at, diff availability — the
/// history UI needs more than bare numbers.
#[tauri::command]
fn project_versions(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let versions = server.db.list_versions(&pid).map_err(|e| e.to_string())?;
    let out: Vec<serde_json::Value> = versions
        .iter()
        .map(|v| {
            json!({
                "version": v.version_number,
                "parent_version": v.parent_version,
                "created_at": v.created_at,
                "has_diff": v.diff_path.is_some(),
            })
        })
        .collect();
    Ok(json!(out))
}

/// Everything the approval UI shows: proposal, validation, preview diff —
/// the user approves a concrete diff, not a "PASS" badge.
#[tauri::command]
fn patch_detail(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    patch_id: i64,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server
        .patch_detail(&pid, patch_id)
        .map_err(|e| e.to_string())
}

/// Standalone quick action (NOT the agent path): deterministic identity swap
/// from the template's verified anchors.
#[tauri::command]
fn quick_identity_swap(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    new_anchor: String,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let (patch_id, report) = server
        .validate_identity_swap(&pid, &new_anchor)
        .map_err(|e| e.to_string())?;
    Ok(json!({ "patch_id": patch_id, "report": report }))
}

#[tauri::command]
fn approve_patch(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    patch_id: i64,
) -> Result<(), String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server
        .resolve_approval(&pid, patch_id, true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reject_patch(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    patch_id: i64,
) -> Result<(), String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server
        .resolve_approval(&pid, patch_id, false)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn commit_patch(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    patch_id: i64,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let out = server
        .commit_patch(&pid, patch_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

#[tauri::command]
fn rollback(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    to_version: u64,
) -> Result<u64, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server.rollback(&pid, to_version).map_err(|e| e.to_string())
}

/// Export to a user-chosen path; when none is given the workspace default
/// (exports/<project>/export-v<version>.json) is used and returned.
#[tauri::command]
fn export_project(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    out_path: Option<String>,
) -> Result<String, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let row = server.db.get_project(&pid).map_err(|e| e.to_string())?;
    let out = match out_path.filter(|p| !p.trim().is_empty()) {
        Some(p) => PathBuf::from(p),
        None => server
            .workspace
            .exports_dir(&pid)
            .join(format!("export-v{}.json", row.current_version)),
    };
    server
        .export_json(&pid, &out)
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn persistence_warnings(server: tauri::State<Arc<AppServer>>) -> Result<Vec<String>, String> {
    Ok(server.take_persistence_warnings())
}

/// Whether the agent can run and on which provider — the UI gates the Agent
/// page on this instead of silently running a mock.
#[tauri::command]
fn agent_provider_status(
    server: tauri::State<Arc<AppServer>>,
) -> Result<serde_json::Value, String> {
    let active = server
        .db
        .get_setting("agent.active_provider")
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(String::from));
    let configured = match active.as_deref() {
        Some("mock") => true,
        Some(_) => true,
        None => false,
    };
    Ok(json!({ "configured": configured, "active": active }))
}

/// Lifecycle 2.0 agent commands: submit through the thread Op queue.
/// Telemetry streams via `sbx://agent-event` (incl. token-level
/// MessageDelta); pull the terminal outcome with `agent_thread_result`.
/// The thread is REHYDRATED from persisted history (durable sessions) and
/// is expected to be per-project (`ui-<projectId>`) — the frontend owns that
/// key so projects never share conversation context.

#[tauri::command]
fn agent_start(
    server: tauri::State<Arc<AppServer>>,
    thread_id: String,
    project_id: String,
    text: String,
) -> Result<serde_json::Value, String> {
    let manager = server.agent_manager().map_err(|e| e.to_string())?;
    // durable sessions: rehydrate persisted history on first spawn
    let history = server.agent_thread_history(&thread_id);
    let resumed = !history.is_empty();
    let handle = if resumed {
        manager.spawn_thread_with_history(&thread_id, history)
    } else {
        manager.spawn_thread(&thread_id)
    };
    handle.clear_result();
    handle
        .try_submit(agent_runtime::ThreadOp::UserTurn {
            text,
            project_id: Some(project_id),
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({ "started": true, "thread_id": thread_id, "resumed": resumed }))
}

#[tauri::command]
fn agent_steer(
    server: tauri::State<Arc<AppServer>>,
    thread_id: String,
    text: String,
) -> Result<(), String> {
    let manager = server.agent_manager().map_err(|e| e.to_string())?;
    let handle = manager.get(&thread_id).ok_or("unknown thread")?;
    handle
        .try_submit(agent_runtime::ThreadOp::Steer { text })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn agent_cancel(server: tauri::State<Arc<AppServer>>, thread_id: String) -> Result<(), String> {
    let manager = server.agent_manager().map_err(|e| e.to_string())?;
    let handle = manager.get(&thread_id).ok_or("unknown thread")?;
    handle
        .try_submit(agent_runtime::ThreadOp::Cancel)
        .map_err(|e| e.to_string())
}

/// The turn's REAL result. `NeedsApproval` carries the stored patch_id —
/// the frontend then pulls `patch_detail` for the diff and approves THAT
/// patch. No re-synthesis: what the agent proposed is what gets approved.
#[tauri::command]
fn agent_thread_result(
    server: tauri::State<Arc<AppServer>>,
    thread_id: String,
    project_id: String,
) -> Result<serde_json::Value, String> {
    let manager = server.agent_manager().map_err(|e| e.to_string())?;
    let Some(handle) = manager.get(&thread_id) else {
        return Ok(json!({ "lifecycle": "unknown" }));
    };
    let lifecycle = format!("{:?}", handle.lifecycle());
    let result = match handle.last_result() {
        Some(agent_runtime::TurnStatus::NeedsApproval {
            patch_id,
            auto_approved,
            risk,
        }) => {
            let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
            let detail = server
                .patch_detail(&pid, patch_id)
                .map_err(|e| e.to_string())?;
            json!({
                "kind": "needs_approval",
                "patch_id": patch_id,
                "auto_approved": auto_approved,
                "risk": risk,
                "detail": detail,
            })
        }
        Some(agent_runtime::TurnStatus::Completed { reply }) => {
            json!({ "kind": "completed", "reply": reply })
        }
        Some(agent_runtime::TurnStatus::ValidationExhausted { failures }) => {
            json!({ "kind": "validation_exhausted", "failures": failures })
        }
        Some(agent_runtime::TurnStatus::Cancelled) => json!({ "kind": "cancelled" }),
        Some(agent_runtime::TurnStatus::Failed { error }) => {
            json!({ "kind": "failed", "error": error })
        }
        None => json!({ "kind": "pending" }),
    };
    Ok(json!({ "lifecycle": lifecycle, "result": result }))
}

// ---- provider configuration (Settings) ------------------------------------

#[tauri::command]
fn providers_list(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    server.provider_summaries().map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_save(
    server: tauri::State<Arc<AppServer>>,
    id: String,
    name: String,
    base_url: String,
    model: String,
) -> Result<(), String> {
    server
        .save_provider(&id, &name, &base_url, &model)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_delete(server: tauri::State<Arc<AppServer>>, id: String) -> Result<(), String> {
    server.delete_provider(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_set_api_key(
    server: tauri::State<Arc<AppServer>>,
    id: String,
    key: String,
) -> Result<(), String> {
    server
        .set_provider_api_key(&id, &key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_test(
    server: tauri::State<Arc<AppServer>>,
    id: String,
) -> Result<serde_json::Value, String> {
    server.test_provider(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn provider_activate(server: tauri::State<Arc<AppServer>>, id: String) -> Result<(), String> {
    server.set_active_provider(&id).map_err(|e| e.to_string())
}
