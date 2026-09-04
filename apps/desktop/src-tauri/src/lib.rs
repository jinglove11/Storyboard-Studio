//! Tauri bridge: thin commands over the in-process AppServer (plan §17 —
//! React ⇄ Tauri IPC ⇄ UI Facade ⇄ App Server). No domain logic here.
//!
//! Concurrency model: `Arc<AppServer>` is lock-free for callers (Db has its
//! own short-lived mutex per call). Agent turns run on background threads so
//! a slow provider never blocks UI commands; results and per-event telemetry
//! flow back through `app.emit`.

use app_server::AppServer;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use storyboard_domain::ProjectId;
use storyboard_importer::skill::SkillBundle;
use tauri::Emitter;

pub const EVT_AGENT_EVENT: &str = "sbx://agent-event";
pub const EVT_AGENT_RESULT: &str = "sbx://agent-turn-result";

pub fn fixture_skill_path() -> PathBuf {
    if let Ok(p) = std::env::var("SBX_SKILL_PATH") {
        return PathBuf::from(p);
    }
    for cand in ["../../fixtures/current-skill", "fixtures/current-skill"] {
        let p = PathBuf::from(cand);
        if p.join("references/template-index.json").is_file() {
            return p;
        }
    }
    PathBuf::from("fixtures/current-skill")
}

fn default_workspace() -> PathBuf {
    if let Ok(p) = std::env::var("SBX_WORKSPACE") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local/share/StoryboardStudio/workspace")
}

pub fn run() {
    let workspace = default_workspace();
    let server = if workspace.join("database").is_dir() {
        AppServer::open(&workspace).expect("open workspace")
    } else {
        let skill = SkillBundle::open_dir(fixture_skill_path())
            .or_else(|_| SkillBundle::open_zip(fixture_skill_path().with_extension("skill")))
            .expect("open skill fixture");
        AppServer::init(&workspace, &skill).expect("init workspace")
    };
    let server = Arc::new(server);

    tauri::Builder::default()
        .manage(server.clone())
        .setup(move |app| {
            // §17 bridge: forward every app-server event to the webview.
            let rx = server.bus.subscribe();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    let _ = handle.emit(EVT_AGENT_EVENT, &event);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace_info,
            list_templates,
            parse_intent,
            match_templates,
            clone_project,
            list_projects,
            project_versions,
            build_identity_patch,
            approve_patch,
            reject_patch,
            commit_patch,
            rollback,
            export_project,
            agent_start,
            agent_steer,
            agent_cancel,
            agent_thread_result,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
fn workspace_info(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    Ok(json!({
        "root": server.workspace.root.display().to_string(),
        "templates": server.template_metadata().map(|t| t.len()).unwrap_or(0),
    }))
}

#[tauri::command]
fn list_templates(server: tauri::State<Arc<AppServer>>) -> Result<serde_json::Value, String> {
    serde_json::to_value(server.template_metadata().unwrap_or_default()).map_err(|e| e.to_string())
}

#[tauri::command]
fn parse_intent(server: tauri::State<Arc<AppServer>>, text: String) -> Result<serde_json::Value, String> {
    serde_json::to_value(server.parse_intent(&text)).map_err(|e| e.to_string())
}

#[tauri::command]
fn match_templates(
    server: tauri::State<Arc<AppServer>>,
    text: String,
    seed: Option<u64>,
) -> Result<serde_json::Value, String> {
    let intent = server.parse_intent(&text);
    let sel = server.match_templates(&intent, seed).map_err(|e| e.to_string())?;
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
    let rows = server.db.list_projects().unwrap_or_default();
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

#[tauri::command]
fn project_versions(server: tauri::State<Arc<AppServer>>, project_id: String) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let versions = server.db.list_versions(&pid).map_err(|e| e.to_string())?;
    Ok(json!(versions.iter().map(|v| v.version_number).collect::<Vec<_>>()))
}

/// Deterministic identity patch built from the template's verified anchors —
/// what a well-behaved agent would emit for "把角色换成 <new>".
#[tauri::command]
fn build_identity_patch(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    new_anchor: String,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let (patch_id, report) =
        server.validate_identity_swap(&pid, &new_anchor).map_err(|e| e.to_string())?;
    Ok(json!({ "patch_id": patch_id, "report": report }))
}

#[tauri::command]
fn approve_patch(server: tauri::State<Arc<AppServer>>, project_id: String, patch_id: i64) -> Result<(), String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server.resolve_approval(&pid, patch_id, true).map_err(|e| e.to_string())
}

#[tauri::command]
fn reject_patch(server: tauri::State<Arc<AppServer>>, project_id: String, patch_id: i64) -> Result<(), String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server.resolve_approval(&pid, patch_id, false).map_err(|e| e.to_string())
}

#[tauri::command]
fn commit_patch(
    server: tauri::State<Arc<AppServer>>,
    project_id: String,
    patch_id: i64,
) -> Result<serde_json::Value, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let out = server.commit_patch(&pid, patch_id).map_err(|e| e.to_string())?;
    serde_json::to_value(out).map_err(|e| e.to_string())
}

#[tauri::command]
fn rollback(server: tauri::State<Arc<AppServer>>, project_id: String, to_version: u64) -> Result<u64, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    server.rollback(&pid, to_version).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_project(server: tauri::State<Arc<AppServer>>, project_id: String) -> Result<String, String> {
    let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
    let out = server.workspace.exports_dir(&pid).join("export.json");
    server.export_json(&pid, &out).map(|p| p.display().to_string()).map_err(|e| e.to_string())
}

/// Lifecycle 2.0 agent commands: submit through the thread Op queue.
/// Telemetry streams via `sbx://agent-event` (incl. token-level
/// MessageDelta); pull the terminal outcome with `agent_thread_result`.

#[tauri::command]
fn agent_start(
    server: tauri::State<Arc<AppServer>>,
    thread_id: String,
    project_id: String,
    text: String,
) -> Result<serde_json::Value, String> {
    let manager = server.agent_manager();
    let handle = manager.spawn_thread(&thread_id);
    handle.clear_result();
    handle
        .try_submit(agent_runtime::ThreadOp::UserTurn {
            text,
            project_id: Some(project_id),
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({ "started": true, "thread_id": thread_id }))
}

#[tauri::command]
fn agent_steer(server: tauri::State<Arc<AppServer>>, thread_id: String, text: String) -> Result<(), String> {
    let manager = server.agent_manager();
    let handle = manager.get(&thread_id).ok_or("unknown thread")?;
    handle.try_submit(agent_runtime::ThreadOp::Steer { text }).map_err(|e| e.to_string())
}

#[tauri::command]
fn agent_cancel(server: tauri::State<Arc<AppServer>>, thread_id: String) -> Result<(), String> {
    let manager = server.agent_manager();
    let handle = manager.get(&thread_id).ok_or("unknown thread")?;
    handle.try_submit(agent_runtime::ThreadOp::Cancel).map_err(|e| e.to_string())
}

#[tauri::command]
fn agent_thread_result(
    server: tauri::State<Arc<AppServer>>,
    thread_id: String,
    project_id: String,
    new_anchor: String,
) -> Result<serde_json::Value, String> {
    let manager = server.agent_manager();
    let Some(handle) = manager.get(&thread_id) else {
        return Ok(json!({ "lifecycle": "unknown" }));
    };
    let lifecycle = format!("{:?}", handle.lifecycle());
    let result = match handle.last_result() {
        Some(agent_runtime::TurnStatus::NeedsApproval { patch_id, .. }) => {
            // attach the deterministic pipeline report (mock provider until
            // Settings wires a real one)
            let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
            match server.validate_identity_swap(&pid, &new_anchor) {
                Ok((pid_num, report)) => json!({
                    "kind": "needs_approval",
                    "patch_id": pid_num,
                    "report": report,
                }),
                Err(e) => json!({ "kind": "needs_approval", "patch_id": patch_id, "error": e.to_string() }),
            }
        }
        Some(other) => serde_json::to_value(format!("{other:?}")).map(|v| json!({ "kind": "status", "detail": v })).unwrap_or(json!({ "kind": "status" })),
        None => json!({ "kind": "pending" }),
    };
    Ok(json!({ "lifecycle": lifecycle, "result": result }))
}
