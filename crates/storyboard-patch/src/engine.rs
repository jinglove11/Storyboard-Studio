use crate::token::{count_occurrences, find_occurrences, replace_all};
use storyboard_domain::{
    diff, OperationKind, PatchError, PatchOperation, PatchProposal, ProjectSnapshot, SeedStrategy,
    TextTarget, TokenReplacement,
};
use std::collections::BTreeSet;

/// Result of applying a proposal in memory. Nothing here touches disk.
#[derive(Debug, Clone)]
pub struct PatchApplication {
    pub draft: serde_json::Value,
    pub applied: Vec<String>,
    pub touched_panels: BTreeSet<u32>,
    pub diff: diff::ProjectDiff,
}

/// Apply a proposal to `base` in memory. Precondition failures abort the
/// whole patch — no partial application, no fuzzy matching (plan §12.3).
pub fn apply_proposal(base: &ProjectSnapshot, proposal: &PatchProposal) -> Result<PatchApplication, PatchError> {
    // Gate 0: stale baseline.
    if proposal.base_project_version != base.version {
        return Err(PatchError::StalePatch {
            expected: proposal.base_project_version,
            current: base.version,
        });
    }
    let mut draft = base.raw.clone();
    let mut applied = Vec::new();
    let mut touched: BTreeSet<u32> = BTreeSet::new();

    let panels_len = draft.get("panels").and_then(|p| p.as_array()).map(|a| a.len()).unwrap_or(0);

    for op in &proposal.operations {
        apply_operation(base, &mut draft, op, panels_len, &mut applied, &mut touched)?;
    }

    let d = crate::diff::diff_projects(base.version, base.version + 1, &base.raw, &draft);
    Ok(PatchApplication { draft, applied, touched_panels: touched, diff: d })
}

fn apply_operation(
    base: &ProjectSnapshot,
    draft: &mut serde_json::Value,
    op: &PatchOperation,
    panels_len: usize,
    applied: &mut Vec<String>,
    touched: &mut BTreeSet<u32>,
) -> Result<(), PatchError> {
    let op_id = op.common.operation_id.clone();

    // per-op version precondition
    if op.common.expected_project_version != base.version {
        return Err(PatchError::PreconditionFailed {
            op_id,
            reason: format!(
                "expected_project_version {} != current {}",
                op.common.expected_project_version, base.version
            ),
        });
    }

    // panel targeting preconditions
    if let Some(idx) = op.common.panel_index {
        if idx == 0 || idx as usize > panels_len {
            return Err(PatchError::TargetMissing { op_id, reason: format!("panel index {idx} out of range 1..={panels_len}") });
        }
        if let Some(pid) = &op.common.panel_id {
            let current_id = draft
                .get("panels")
                .and_then(|p| p.as_array())
                .and_then(|a| a.get(idx as usize - 1))
                .and_then(|p| p.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if current_id != pid {
                return Err(PatchError::PreconditionFailed {
                    op_id,
                    reason: format!("panel {idx} id is `{current_id}`, proposal expected `{pid}`"),
                });
            }
        }
    }

    match &op.kind {
        OperationKind::ReplaceCharacterIdentity { replacements, slots } => {
            apply_token_replacements(draft, replacements, slots.as_deref(), &op_id, touched)?;
            applied.push(format!("{op_id}: replace_character_identity ({} mappings)", replacements.len()));
        }
        OperationKind::ReplaceSceneToken { replacements } => {
            apply_token_replacements(draft, replacements, None, &op_id, touched)?;
            applied.push(format!("{op_id}: replace_scene_token ({} mappings)", replacements.len()));
        }
        OperationKind::PatchPromptBlock { target, new_text } => {
            let idx = require_panel(&op_id, op)?;
            let expected = expected_old_checked(base, op)?;
            patch_text_block(draft, idx, target, &expected, new_text, &op_id, false)?;
            touched.insert(idx);
            applied.push(format!("{op_id}: patch_prompt_block panel {idx}"));
        }
        OperationKind::UpdateTitle { new_title } => {
            draft["title"] = serde_json::Value::String(new_title.clone());
            if let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) {
                for p in panels.iter_mut() {
                    p["title"] = serde_json::Value::String(new_title.clone());
                }
            }
            applied.push(format!("{op_id}: update_title `{new_title}`"));
        }
        OperationKind::RegenerateIds => {
            let mut rng = crate::resize::SplitMix64::new(seed_from(&op_id));
            let new_pid = crate::resize::uuid_from_rng(&mut rng);
            draft["id"] = serde_json::Value::String(new_pid.to_string());
            if let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) {
                for p in panels.iter_mut() {
                    p["id"] = serde_json::Value::String(crate::resize::uuid_from_rng(&mut rng).to_string());
                }
            }
            applied.push(format!("{op_id}: regenerate_ids"));
        }
        OperationKind::RegenerateSeeds { strategy } => {
            match strategy {
                SeedStrategy::Keep => {}
                SeedStrategy::Fixed(s) => {
                    if let Some(gp) = draft.get_mut("globalParams") {
                        gp["seed"] = serde_json::Value::Number((*s as u64).into());
                    }
                    if let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) {
                        for p in panels.iter_mut() {
                            p["paramsOverride"]["params"]["seed"] = serde_json::Value::Number((*s as u64).into());
                        }
                    }
                }
                SeedStrategy::RandomNonRepeating => {
                    let mut rng = crate::resize::SplitMix64::new(seed_from(&op_id));
                    let mut used = BTreeSet::new();
                    let mut next = |rng: &mut crate::resize::SplitMix64| loop {
                        let s = rng.next_u64() % 4_000_000_000;
                        if used.insert(s) {
                            break s;
                        }
                    };
                    if let Some(gp) = draft.get_mut("globalParams") {
                        gp["seed"] = serde_json::Value::Number(next(&mut rng).into());
                    }
                    if let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) {
                        for p in panels.iter_mut() {
                            let s = next(&mut rng);
                            p["paramsOverride"]["params"]["seed"] = serde_json::Value::Number(s.into());
                        }
                    }
                }
            }
            applied.push(format!("{op_id}: regenerate_seeds ({:?})", strategy));
        }
        OperationKind::ResizeStoryboard { target_panel_count } => {
            let n = *target_panel_count;
            if n == 0 || n > 500 {
                return Err(PatchError::InvalidOperation { op_id, reason: format!("target panel count {n} out of sane range") });
            }
            crate::resize::resize_panels(draft, n, seed_from(&op_id));
            applied.push(format!("{op_id}: resize_storyboard -> {n} panels"));
        }
        OperationKind::DeleteConflictingBlock { target } => {
            let idx = require_panel(&op_id, op)?;
            let expected = expected_old_checked(base, op)?;
            patch_text_block(draft, idx, target, &expected, "", &op_id, true)?;
            touched.insert(idx);
            applied.push(format!("{op_id}: delete_conflicting_block panel {idx}"));
        }
    }
    Ok(())
}

fn require_panel(op_id: &str, op: &PatchOperation) -> Result<u32, PatchError> {
    op.common.panel_index.ok_or_else(|| PatchError::InvalidOperation {
        op_id: op_id.into(),
        reason: "panel-scoped operation without panel_index".into(),
    })
}

/// Resolve `expected_old` and verify its hash precondition when present.
fn expected_old_checked(base: &ProjectSnapshot, op: &PatchOperation) -> Result<String, PatchError> {
    let op_id = &op.common.operation_id;
    let expected = op.common.expected_old.clone().ok_or_else(|| PatchError::PreconditionFailed {
        op_id: op_id.clone(),
        reason: "mutating existing content requires expected_old".into(),
    })?;
    if let Some(h) = &op.common.expected_old_hash {
        if crate::text_hash(&expected) != *h {
            return Err(PatchError::PreconditionFailed {
                op_id: op_id.clone(),
                reason: "expected_old does not match expected_old_hash".into(),
            });
        }
    }
    // The anchor (if any) must be contained in the expected block.
    if let Some(anchor) = &op.common.anchor {
        if !expected.contains(anchor.as_str()) {
            return Err(PatchError::PreconditionFailed {
                op_id: op_id.clone(),
                reason: format!("anchor `{anchor}` not contained in expected_old"),
            });
        }
    }
    let _ = base;
    Ok(expected)
}

/// Locate `expected` in the target text; require exactly one occurrence.
fn patch_text_block(
    draft: &mut serde_json::Value,
    panel_index: u32,
    target: &TextTarget,
    expected: &str,
    new_text: &str,
    op_id: &str,
    delete: bool,
) -> Result<(), PatchError> {
    let panels = draft
        .get_mut("panels")
        .and_then(|p| p.as_array_mut())
        .ok_or_else(|| PatchError::TargetMissing { op_id: op_id.into(), reason: "no panels array".into() })?;
    let panel = panels
        .get_mut(panel_index as usize - 1)
        .ok_or_else(|| PatchError::TargetMissing { op_id: op_id.into(), reason: format!("panel {panel_index} missing") })?;
    let field = match target {
        TextTarget::PanelPrompt => panel.get_mut("prompt"),
        TextTarget::CharacterSlot { slot } => panel
            .get_mut("customCharacters")
            .and_then(|c| c.as_array_mut())
            .and_then(|a| a.get_mut(*slot as usize))
            .and_then(|cc| cc.get_mut("prompt")),
    }
    .ok_or_else(|| PatchError::TargetMissing {
        op_id: op_id.into(),
        reason: format!("text target {target:?} missing on panel {panel_index}"),
    })?;
    let text = field.as_str().ok_or_else(|| PatchError::TargetMissing {
        op_id: op_id.into(),
        reason: "target field is not a string".into(),
    })?;
    let hits = find_occurrences(text, expected);
    if hits.is_empty() {
        return Err(PatchError::AnchorNotFound {
            op_id: op_id.into(),
            reason: format!("expected_old no longer present on panel {panel_index} (stale patch)"),
        });
    }
    if hits.len() > 1 {
        return Err(PatchError::AmbiguousAnchor { op_id: op_id.into(), count: hits.len() });
    }
    let mut new_full = String::with_capacity(text.len());
    let p = hits[0];
    new_full.push_str(&text[..p]);
    new_full.push_str(new_text);
    new_full.push_str(&text[p + expected.len()..]);
    let _ = delete;
    *field = serde_json::Value::String(new_full);
    Ok(())
}

/// Apply token replacements across title / panel prompts / (optionally
/// restricted) CC prompts. Every `old_token` must exist somewhere in scope,
/// otherwise the operation is stale (precondition).
fn apply_token_replacements(
    draft: &mut serde_json::Value,
    replacements: &[TokenReplacement],
    slots: Option<&[u32]>,
    op_id: &str,
    touched: &mut BTreeSet<u32>,
) -> Result<(), PatchError> {
    // precondition: each old_token occurs at least once in scope
    for r in replacements {
        let mut found = 0usize;
        if let Some(title) = draft.get("title").and_then(|t| t.as_str()) {
            found += count_occurrences(title, &r.old_token);
        }
        if let Some(panels) = draft.get("panels").and_then(|p| p.as_array()) {
            for p in panels {
                if let Some(prompt) = p.get("prompt").and_then(|x| x.as_str()) {
                    found += count_occurrences(prompt, &r.old_token);
                }
                if let Some(ccs) = p.get("customCharacters").and_then(|c| c.as_array()) {
                    for (i, cc) in ccs.iter().enumerate() {
                        let in_scope = slots.map(|s| s.contains(&(i as u32))).unwrap_or(true);
                        if in_scope {
                            if let Some(prompt) = cc.get("prompt").and_then(|x| x.as_str()) {
                                found += count_occurrences(prompt, &r.old_token);
                            }
                        }
                    }
                }
            }
        }
        if found == 0 {
            return Err(PatchError::PreconditionFailed {
                op_id: op_id.into(),
                reason: format!("token `{}` not present in scope — stale or wrong mapping", r.old_token),
            });
        }
    }

    // apply
    if let Some(title) = draft.get("title").and_then(|t| t.as_str()).map(String::from) {
        let mut t = title;
        for r in replacements {
            t = replace_all(&t, &r.old_token, &r.new_token).0;
        }
        draft["title"] = serde_json::Value::String(t);
    }
    if let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) {
        for (i, panel) in panels.iter_mut().enumerate() {
            let mut changed = false;
            if let Some(prompt) = panel.get("prompt").and_then(|x| x.as_str()).map(String::from) {
                let mut t = prompt;
                for r in replacements {
                    t = replace_all(&t, &r.old_token, &r.new_token).0;
                }
                panel["prompt"] = serde_json::Value::String(t);
                changed = true;
            }
            if let Some(ccs) = panel.get_mut("customCharacters").and_then(|c| c.as_array_mut()) {
                for (j, cc) in ccs.iter_mut().enumerate() {
                    let in_scope = slots.map(|s| s.contains(&(j as u32))).unwrap_or(true);
                    if !in_scope {
                        continue;
                    }
                    if let Some(prompt) = cc.get("prompt").and_then(|x| x.as_str()).map(String::from) {
                        let mut t = prompt;
                        for r in replacements {
                            t = replace_all(&t, &r.old_token, &r.new_token).0;
                        }
                        cc["prompt"] = serde_json::Value::String(t);
                        changed = true;
                    }
                }
            }
            if changed {
                touched.insert((i + 1) as u32);
            }
        }
    }
    Ok(())
}

fn seed_from(op_id: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in op_id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
