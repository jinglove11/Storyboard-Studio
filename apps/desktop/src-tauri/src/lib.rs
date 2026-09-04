//! Tauri bridge: thin commands over the in-process AppServer (plan §17 —
//! React ⇄ Tauri IPC ⇄ UI Facade ⇄ App Server). No domain logic here.

use agent_protocol::EventBus;
use app_server::AppServer;
use model_providers::MockProvider;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use storyboard_domain::ProjectId;
use storyboard_importer::skill::SkillBundle;

pub struct State {
    pub server: AppServer,
}

pub fn fixture_skill_path() -> PathBuf {
    if let Ok(p) = std::env::var("SBX_SKILL_PATH") {
        return PathBuf::from(p);
    }
    // dev runs from apps/desktop or repo root
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

    tauri::Builder::default()
        .manage(Mutex::new(State { server }))
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
            agent_swap_identity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn with_server<T>(state: &tauri::State<Mutex<State>>, f: impl FnOnce(&AppServer) -> Result<T, String>) -> Result<T, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    f(&guard.server)
}

#[tauri::command]
fn workspace_info(state: tauri::State<Mutex<State>>) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        Ok(json!({
            "root": s.workspace.root.display().to_string(),
            "templates": s.template_metadata().map(|t| t.len()).unwrap_or(0),
        }))
    })
}

#[tauri::command]
fn list_templates(state: tauri::State<Mutex<State>>) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        serde_json::to_value(s.template_metadata().unwrap_or_default()).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn parse_intent(state: tauri::State<Mutex<State>>, text: String) -> Result<serde_json::Value, String> {
    with_server(&state, |s| serde_json::to_value(s.parse_intent(&text)).map_err(|e| e.to_string()))
}

#[tauri::command]
fn match_templates(
    state: tauri::State<Mutex<State>>,
    text: String,
    seed: Option<u64>,
) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let intent = s.parse_intent(&text);
        let sel = s.match_templates(&intent, seed).map_err(|e| e.to_string())?;
        serde_json::to_value(sel).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn clone_project(
    state: tauri::State<Mutex<State>>,
    template_id: String,
    title: Option<String>,
    seed: Option<u64>,
) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let st = s.clone_project(&template_id, title, seed.unwrap_or(42)).map_err(|e| e.to_string())?;
        serde_json::to_value(json!({
            "id": st.project_id.to_string(),
            "title": st.title,
            "source_template_id": st.source_template_id.as_str(),
            "current_version": st.current_version,
            "status": st.status,
            "created_at": st.created_at,
            "updated_at": st.updated_at,
        }))
        .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn list_projects(state: tauri::State<Mutex<State>>) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let rows = s.db.list_projects().unwrap_or_default();
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
    })
}

#[tauri::command]
fn project_versions(state: tauri::State<Mutex<State>>, project_id: String) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        let versions = s.db.list_versions(&pid).map_err(|e| e.to_string())?;
        Ok(json!(versions.iter().map(|v| v.version_number).collect::<Vec<_>>()))
    })
}

/// Deterministic identity patch built from the template's verified anchors —
/// what a well-behaved agent would emit for "把角色换成 <new>".
#[tauri::command]
fn build_identity_patch(
    state: tauri::State<Mutex<State>>,
    project_id: String,
    new_anchor: String,
) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        let (patch_id, report) = s.validate_identity_swap(&pid, &new_anchor).map_err(|e| e.to_string())?;
        Ok(json!({ "patch_id": patch_id, "report": report }))
    })
}

#[tauri::command]
fn approve_patch(state: tauri::State<Mutex<State>>, project_id: String, patch_id: i64) -> Result<(), String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        s.resolve_approval(&pid, patch_id, true).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn reject_patch(state: tauri::State<Mutex<State>>, project_id: String, patch_id: i64) -> Result<(), String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        s.resolve_approval(&pid, patch_id, false).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn commit_patch(
    state: tauri::State<Mutex<State>>,
    project_id: String,
    patch_id: i64,
) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        let out = s.commit_patch(&pid, patch_id).map_err(|e| e.to_string())?;
        serde_json::to_value(out).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn rollback(state: tauri::State<Mutex<State>>, project_id: String, to_version: u64) -> Result<u64, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        s.rollback(&pid, to_version).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn export_project(state: tauri::State<Mutex<State>>, project_id: String) -> Result<String, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        let out = s.workspace.exports_dir(&pid).join("export.json");
        s.export_json(&pid, &out).map(|p| p.display().to_string()).map_err(|e| e.to_string())
    })
}

/// Agent turn with the scripted provider (until real providers are configured
/// in Settings). Exercises the full production tool chain.
#[tauri::command]
fn agent_swap_identity(
    state: tauri::State<Mutex<State>>,
    project_id: String,
    new_anchor: String,
) -> Result<serde_json::Value, String> {
    with_server(&state, |s| {
        let pid: ProjectId = project_id.parse().map_err(|e| format!("{e}"))?;
        let runtime = agent_runtime::AgentRuntime::new(
            agent_runtime::RuntimeConfig::default(),
            std::sync::Arc::new(MockProvider::simple_text(&format!("identity swap acknowledged: {new_anchor}"))),
            EventBus::new(),
        );
        let out = runtime.run_turn("ui-thread", Some(&project_id), &format!("把角色换成 {new_anchor}"), s);
        // deterministic pipeline result alongside the mock turn
        let (_, report) = s.validate_identity_swap(&pid, &new_anchor).map_err(|e| e.to_string())?;
        Ok(json!({
            "status": format!("{:?}", out.status),
            "run_id": out.run_id,
            "report": report,
        }))
    })
}
