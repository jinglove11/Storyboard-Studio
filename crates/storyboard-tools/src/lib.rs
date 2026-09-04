//! Typed Storyboard Tool Registry (plan §15, Table 13).
//!
//! The Production profile registers exactly:
//! `search_templates`, `read_template_summary`, `read_template_panels`,
//! `read_project`, `read_diff_context`, `propose_storyboard_patch`,
//! `preview_storyboard_patch`, `validate_storyboard_patch`.
//!
//! `commit_storyboard_patch`, `rollback_version` and `export_json` are
//! **not tools** — they are Application Controller / user actions and are not
//! reachable through this registry at all (F02, Golden Case H).

use model_providers::ToolSchema;
use serde_json::Value;

pub const TOOL_REGISTRY_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentProfile {
    StoryboardProduction,
    Developer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ReadOnly,
    Propose,
    /// Only invokable by the Application Controller, never registered on an
    /// agent profile.
    AppOnly,
}

/// Backend capabilities exposed to tools. Implemented by the app server.
/// Notice what is *absent*: commit / rollback / export / file access.
pub trait ToolBackend: Sync {
    fn search_templates(&self, query: &Value) -> Result<Value, String>;
    fn read_template_summary(&self, template_id: &str) -> Result<Value, String>;
    fn read_template_panels(&self, template_id: &str, from: u32, to: u32) -> Result<Value, String>;
    fn read_project(&self, project_id: &str) -> Result<Value, String>;
    fn read_diff_context(&self, project_id: &str) -> Result<Value, String>;
    fn propose_patch(&self, project_id: &str, proposal: &Value, run_id: Option<&str>) -> Result<Value, String>;
    fn preview_patch(&self, project_id: &str, proposal: &Value) -> Result<Value, String>;
    fn validate_patch(&self, project_id: &str, proposal: &Value) -> Result<Value, String>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("unknown tool `{0}` — not registered on this profile")]
    UnknownTool(String),
    #[error("bad arguments for {tool}: {message}")]
    BadArguments { tool: String, message: String },
    #[error("tool `{tool}` failed: {message}")]
    Execution { tool: String, message: String },
}

pub struct ToolRegistry {
    entries: Vec<RegisteredTool>,
}

struct RegisteredTool {
    name: &'static str,
    description: &'static str,
    parameters: Value,
    permission: Permission,
    handler: fn(&dyn ToolBackend, &Value, Option<&str>) -> Result<Value, ToolError>,
}

fn str_field(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| ToolError::BadArguments {
            tool: String::new(),
            message: format!("missing string field `{key}`"),
        })
}

fn backend_err(tool: &'static str, message: String) -> ToolError {
    ToolError::Execution { tool: tool.into(), message }
}

macro_rules! tool {
    ($name:literal, $desc:literal, $perm:expr, $params:expr, $handler:expr) => {
        RegisteredTool {
            name: $name,
            description: $desc,
            parameters: $params,
            permission: $perm,
            handler: $handler,
        }
    };
}

impl ToolRegistry {
    pub fn for_profile(profile: AgentProfile) -> Self {
        let all = vec![
            tool!(
                "search_templates",
                "Deterministic Top-K template search. Input: {query: {scene_family?, time?, character_count?, narrative_tags[], props[], desired_panel_count?, keywords[]}}. Returns candidates with score breakdown.",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "object"}
                    },
                    "required": ["query"]
                }),
                |b, args, _run_id| {
                    let q = args.get("query").cloned().unwrap_or(Value::Null);
                    b.search_templates(&q).map_err(|m| backend_err("search_templates", m))
                }
            ),
            tool!(
                "read_template_summary",
                "Read one template's rebuilt metadata (scene, stats, camera profile, warnings).",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {"template_id": {"type": "string"}},
                    "required": ["template_id"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "template_id")?;
                    b.read_template_summary(&id).map_err(|m| backend_err("read_template_summary", m))
                }
            ),
            tool!(
                "read_template_panels",
                "Read full panel payloads of a template (inclusive 1-based range).",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "template_id": {"type": "string"},
                        "from": {"type": "integer", "minimum": 1},
                        "to": {"type": "integer", "minimum": 1}
                    },
                    "required": ["template_id"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "template_id")?;
                    let from = args.get("from").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                    let to = args.get("to").and_then(|v| v.as_u64()).unwrap_or(from as u64 + 9) as u32;
                    b.read_template_panels(&id, from, to).map_err(|m| backend_err("read_template_panels", m))
                }
            ),
            tool!(
                "read_project",
                "Read the current project state (status, version, panel count, source template).",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {"project_id": {"type": "string"}},
                    "required": ["project_id"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "project_id")?;
                    b.read_project(&id).map_err(|m| backend_err("read_project", m))
                }
            ),
            tool!(
                "read_diff_context",
                "Read the diff context between the project's current version and its parent.",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {"project_id": {"type": "string"}},
                    "required": ["project_id"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "project_id")?;
                    b.read_diff_context(&id).map_err(|m| backend_err("read_diff_context", m))
                }
            ),
            tool!(
                "propose_storyboard_patch",
                "Submit a PatchProposal for validation. Does NOT write anything. Input: {project_id, proposal: PatchProposal}.",
                Permission::Propose,
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string"},
                        "proposal": {"type": "object"}
                    },
                    "required": ["project_id", "proposal"]
                }),
                |b, args, run_id| {
                    let id = str_field(args, "project_id")?;
                    let p = args.get("proposal").cloned().unwrap_or(Value::Null);
                    b.propose_patch(&id, &p, run_id).map_err(|m| backend_err("propose_storyboard_patch", m))
                }
            ),
            tool!(
                "preview_storyboard_patch",
                "Apply a PatchProposal in memory and return the preview diff. Read-only computation.",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string"},
                        "proposal": {"type": "object"}
                    },
                    "required": ["project_id", "proposal"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "project_id")?;
                    let p = args.get("proposal").cloned().unwrap_or(Value::Null);
                    b.preview_patch(&id, &p).map_err(|m| backend_err("preview_storyboard_patch", m))
                }
            ),
            tool!(
                "validate_storyboard_patch",
                "Run all deterministic gates on a PatchProposal. Returns the full ValidationReport.",
                Permission::ReadOnly,
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_id": {"type": "string"},
                        "proposal": {"type": "object"}
                    },
                    "required": ["project_id", "proposal"]
                }),
                |b, args, _run_id| {
                    let id = str_field(args, "project_id")?;
                    let p = args.get("proposal").cloned().unwrap_or(Value::Null);
                    b.validate_patch(&id, &p).map_err(|m| backend_err("validate_storyboard_patch", m))
                }
            ),
        ];
        // v1: developer profile exposes the same storyboard tools; its extra
        // powers (apply_patch/shell) arrive with Developer Mode in a later
        // phase and are NOT part of this registry.
        let _ = profile;
        Self { entries: all }
    }

    pub fn version(&self) -> &'static str {
        TOOL_REGISTRY_VERSION
    }

    pub fn tool_names(&self) -> Vec<&'static str> {
        self.entries.iter().map(|t| t.name).collect()
    }

    pub fn permission_of(&self, name: &str) -> Option<Permission> {
        self.entries.iter().find(|t| t.name == name).map(|t| t.permission)
    }

    pub fn schemas_for_provider(&self) -> Vec<ToolSchema> {
        self.entries
            .iter()
            .map(|t| ToolSchema {
                name: t.name.into(),
                description: t.description.into(),
                parameters_json: t.parameters.clone(),
            })
            .collect()
    }

    pub fn dispatch(
        &self,
        name: &str,
        args: &Value,
        run_id: Option<&str>,
        backend: &dyn ToolBackend,
    ) -> Result<Value, ToolError> {
        let tool = self
            .entries
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| ToolError::UnknownTool(name.into()))?;
        (tool.handler)(backend, args, run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FakeBackend;
    impl ToolBackend for FakeBackend {
        fn search_templates(&self, q: &Value) -> Result<Value, String> {
            Ok(json!({"echo": q}))
        }
        fn read_template_summary(&self, id: &str) -> Result<Value, String> {
            Ok(json!({"id": id}))
        }
        fn read_template_panels(&self, _id: &str, _f: u32, _t: u32) -> Result<Value, String> {
            Ok(json!([]))
        }
        fn read_project(&self, id: &str) -> Result<Value, String> {
            Ok(json!({"project": id}))
        }
        fn read_diff_context(&self, _id: &str) -> Result<Value, String> {
            Ok(json!({}))
        }
        fn propose_patch(&self, _id: &str, p: &Value, _run: Option<&str>) -> Result<Value, String> {
            Ok(json!({"stored": p}))
        }
        fn preview_patch(&self, _id: &str, _p: &Value) -> Result<Value, String> {
            Ok(json!({"diff": true}))
        }
        fn validate_patch(&self, _id: &str, _p: &Value) -> Result<Value, String> {
            Ok(json!({"passed": true}))
        }
    }

    #[test]
    fn production_registry_has_no_commit_tool() {
        let reg = ToolRegistry::for_profile(AgentProfile::StoryboardProduction);
        let names = reg.tool_names();
        assert!(!names.contains(&"commit_storyboard_patch"));
        assert!(!names.contains(&"rollback_version"));
        assert!(!names.contains(&"export_json"));
        assert!(names.contains(&"search_templates"));
        assert!(names.contains(&"propose_storyboard_patch"));
    }

    #[test]
    fn commit_lookup_fails_with_unknown_tool() {
        let reg = ToolRegistry::for_profile(AgentProfile::StoryboardProduction);
        let err = reg
            .dispatch("commit_storyboard_patch", &json!({}), None, &FakeBackend)
            .unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(_)));
    }

    #[test]
    fn dispatch_search_templates() {
        let reg = ToolRegistry::for_profile(AgentProfile::StoryboardProduction);
        let out = reg
            .dispatch("search_templates", &json!({"query": {"scene_family": "park"}}), None, &FakeBackend)
            .unwrap();
        assert_eq!(out["echo"]["scene_family"], "park");
    }
}
