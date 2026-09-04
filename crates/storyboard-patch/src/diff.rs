use storyboard_domain::diff::{
    ChangeCategory, DiffSummary, FieldChange, PanelChange, PanelDiff, ProjectDiff, TokenDiff,
};
use storyboard_domain::VersionNumber;

/// Structural diff between two project JSONs (base vs draft/next version).
pub fn diff_projects(
    from_version: VersionNumber,
    to_version: VersionNumber,
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> ProjectDiff {

    let mut global = Vec::new();
    for key in ["title", "globalNegativePrompt", "sizeMode", "globalStylePrompt"] {
        let b = before.get(key).cloned().unwrap_or(serde_json::Value::Null);
        let a = after.get(key).cloned().unwrap_or(serde_json::Value::Null);
        if a != b {
            global.push(FieldChange { path: format!("$.{key}"), category: ChangeCategory::Meta, before: b, after: a });
        }
    }

    let b_panels = before.get("panels").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let a_panels = after.get("panels").and_then(|p| p.as_array()).cloned().unwrap_or_default();

    // match panels by position for v1 (resize renumbers, order preserved)
    let mut panels = Vec::new();
    let max = b_panels.len().max(a_panels.len());
    let mut chars_added = 0u64;
    let mut chars_removed = 0u64;
    let mut total_chars = 0u64;
    let mut modified = 0u32;

    for i in 0..max {
        let bp = b_panels.get(i);
        let ap = a_panels.get(i);
        let mut changes = Vec::new();
        match (bp, ap) {
            (None, Some(_)) => {
                changes.push(PanelChange::Added);
            }
            (Some(_), None) => {
                changes.push(PanelChange::Removed);
            }
            (None, None) => {}
            (Some(bp), Some(ap)) => {
                let b_prompt = bp.get("prompt").and_then(|p| p.as_str()).unwrap_or("");
                let a_prompt = ap.get("prompt").and_then(|p| p.as_str()).unwrap_or("");
                if b_prompt != a_prompt {
                    modified += 1;
                    let td = TokenDiff::compute(b_prompt, a_prompt);
                    chars_added += td.added() as u64;
                    chars_removed += td.removed() as u64;
                    changes.push(PanelChange::Prompt { before: b_prompt.into(), after: a_prompt.into(), tokens: td });
                }
                total_chars += b_prompt.chars().count() as u64;
                let b_ccs = bp.get("customCharacters").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                let a_ccs = ap.get("customCharacters").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                for slot in 0..b_ccs.len().max(a_ccs.len()) {
                    let bcc = b_ccs.get(slot).and_then(|c| c.get("prompt")).and_then(|p| p.as_str()).unwrap_or("");
                    let acc = a_ccs.get(slot).and_then(|c| c.get("prompt")).and_then(|p| p.as_str()).unwrap_or("");
                    if bcc != acc {
                        modified += 1;
                        let td = TokenDiff::compute(bcc, acc);
                        chars_added += td.added() as u64;
                        chars_removed += td.removed() as u64;
                        changes.push(PanelChange::CharacterSlot {
                            slot: slot as u32,
                            before: bcc.into(),
                            after: acc.into(),
                            tokens: td,
                        });
                    }
                }
                for field in ["id", "index", "title", "imageSize"] {
                    let b = bp.get(field).cloned().unwrap_or(serde_json::Value::Null);
                    let a = ap.get(field).cloned().unwrap_or(serde_json::Value::Null);
                    if a != b {
                        changes.push(PanelChange::Field { field: field.into(), before: b, after: a });
                    }
                }
            }
        }
        if !changes.is_empty() {
            panels.push(PanelDiff {
                index: Some(
                    ap.and_then(|p| p.get("index"))
                        .and_then(|i| i.as_u64())
                        .map(|v| v as u32)
                        .unwrap_or(i as u32 + 1),
                ),
                old_index: bp
                    .and_then(|p| p.get("index"))
                    .and_then(|i| i.as_u64())
                    .map(|v| v as u32),
                panel_id: ap
                    .and_then(|p| p.get("id"))
                    .and_then(|v| v.as_str())
                    .map(String::from),
                changes,
            });
        }
    }

    let preservation = if total_chars == 0 {
        1.0
    } else {
        let changed = chars_added.max(chars_removed) as f32;
        (1.0 - (changed / total_chars as f32)).max(0.0)
    };

    ProjectDiff {
        from_version,
        to_version,
        global,
        panels,
        summary: DiffSummary {
            panels_added: a_panels.len().saturating_sub(b_panels.len()) as u32,
            panels_removed: b_panels.len().saturating_sub(a_panels.len()) as u32,
            panels_modified: modified,
            prompt_chars_added: chars_added,
            prompt_chars_removed: chars_removed,
            preservation_ratio: preservation,
        },
    }
}
