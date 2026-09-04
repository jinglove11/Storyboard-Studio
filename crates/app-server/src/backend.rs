//! ToolBackend wiring: the agent runtime's tools run against this app
//! server. Only read/propose/preview/validate operations exist here.

use crate::AppServer;
use serde_json::{json, Value};
use storyboard_domain::{PatchProposal, ProjectId};
use storyboard_tools::ToolBackend;

impl ToolBackend for AppServer {
    fn search_templates(&self, query: &Value) -> Result<Value, String> {
        // accept either a QueryIntent object or {"text": "..."} free-form input
        let intent: storyboard_domain::QueryIntent = if let Some(text) = query.get("text").and_then(|t| t.as_str()) {
            self.parse_intent(text)
        } else {
            serde_json::from_value(query.clone()).map_err(|e| format!("bad QueryIntent: {e}"))?
        };
        let selection = self
            .match_templates(&intent, intent.seed)
            .map_err(|e| e.to_string())?
            .ok_or("no templates imported")?;
        Ok(json!({
            "primary": selection.primary,
            "candidates": selection.candidates,
            "mode": selection.mode,
            "needs_scene_adaptation": selection.needs_scene_adaptation,
        }))
    }

    fn read_template_summary(&self, template_id: &str) -> Result<Value, String> {
        let m = self.db.get_template_metadata(template_id).map_err(|e| e.to_string())?;
        serde_json::to_value(m).map_err(|e| e.to_string())
    }

    fn read_template_panels(&self, template_id: &str, from: u32, to: u32) -> Result<Value, String> {
        let snap = self.load_template_snapshot(template_id).map_err(|e| e.to_string())?;
        let panels = snap.panels();
        let from0 = (from.saturating_sub(1) as usize).min(panels.len());
        let to0 = (to as usize).min(panels.len());
        if from0 >= to0 {
            return Ok(json!({ "panels": [], "total": panels.len() }));
        }
        Ok(json!({ "panels": &panels[from0..to0], "total": panels.len() }))
    }

    fn read_project(&self, project_id: &str) -> Result<Value, String> {
        let pid: ProjectId = project_id.parse().map_err(|_| "bad project id".to_string())?;
        let row = self.db.get_project(&pid).map_err(|e| e.to_string())?;
        Ok(json!({
            "project_id": row.id,
            "title": row.title,
            "status": row.status,
            "current_version": row.current_version,
            "source_template_id": row.source_template_id,
            "source_revision_id": row.source_template_revision_id,
        }))
    }

    fn read_diff_context(&self, project_id: &str) -> Result<Value, String> {
        let pid: ProjectId = project_id.parse().map_err(|_| "bad project id".to_string())?;
        let versions = self.db.list_versions(&pid).map_err(|e| e.to_string())?;
        let latest = versions.last().ok_or("project has no versions")?;
        match &latest.diff_path {
            Some(p) => {
                let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
                let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                Ok(v)
            }
            None => Ok(json!({ "versions": versions.len(), "diff": null })),
        }
    }

    fn propose_patch(&self, project_id: &str, proposal: &Value, run_id: Option<&str>) -> Result<Value, String> {
        let pid: ProjectId = project_id.parse().map_err(|_| "bad project id".to_string())?;
        let p: PatchProposal =
            serde_json::from_value(proposal.clone()).map_err(|e| format!("bad PatchProposal: {e}"))?;
        let (patch_id, report) = self.propose_patch(&pid, &p, run_id).map_err(|e| e.to_string())?;
        Ok(json!({ "patch_id": patch_id, "report": report, "run_id": run_id }))
    }

    fn preview_patch(&self, project_id: &str, proposal: &Value) -> Result<Value, String> {
        let pid: ProjectId = project_id.parse().map_err(|_| "bad project id".to_string())?;
        let p: PatchProposal =
            serde_json::from_value(proposal.clone()).map_err(|e| format!("bad PatchProposal: {e}"))?;
        let (_, app) = self.validate_patch(&pid, &p).map_err(|e| e.to_string())?;
        serde_json::to_value(app.diff).map_err(|e| e.to_string())
    }

    fn validate_patch(&self, project_id: &str, proposal: &Value) -> Result<Value, String> {
        let pid: ProjectId = project_id.parse().map_err(|_| "bad project id".to_string())?;
        let p: PatchProposal =
            serde_json::from_value(proposal.clone()).map_err(|e| format!("bad PatchProposal: {e}"))?;
        let (report, _) = self.validate_patch(&pid, &p).map_err(|e| e.to_string())?;
        serde_json::to_value(json!({ "report": report })).map_err(|e| e.to_string())
    }
}
