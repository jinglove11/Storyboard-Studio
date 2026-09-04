//! Deterministic validators run before every commit (plan §13, Table 11).
//! All gates are pure functions over (template, base, proposal, draft).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use storyboard_domain::{
    schema, OperationKind, PatchIntent, PatchProposal, ProjectSnapshot, TemplateMetadata,
    TemplateSnapshot,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ValidatorConfig {
    /// Non-target text preservation floor for identity-only patches.
    pub identity_preservation_min: f32,
    /// Floor when scene blocks are being remapped.
    pub scene_preservation_min: f32,
    /// Metadata-token leak scan emits failures (true) or warnings (false).
    pub strict_metadata_leak_scan: bool,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            identity_preservation_min: 0.90,
            scene_preservation_min: 0.80,
            strict_metadata_leak_scan: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub gate: String,
    pub passed: bool,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
}

impl GateResult {
    fn new(gate: &'static str) -> Self {
        Self { gate: gate.into(), passed: true, failures: Vec::new(), warnings: Vec::new() }
    }
    fn fail(&mut self, msg: String) {
        self.passed = false;
        self.failures.push(msg);
    }
    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub passed: bool,
    pub schema: GateResult,
    pub scope: GateResult,
    pub anti_rewrite: GateResult,
    pub identity_leak: GateResult,
    pub scene_leak: GateResult,
    pub reference_integrity: GateResult,
    pub json_parse: GateResult,
    pub preservation_ratio: f32,
}

impl ValidationReport {
    pub fn gates(&self) -> Vec<&GateResult> {
        vec![
            &self.schema,
            &self.scope,
            &self.anti_rewrite,
            &self.identity_leak,
            &self.scene_leak,
            &self.reference_integrity,
            &self.json_parse,
        ]
    }
}

/// Inputs the validators need beyond the patch itself.
pub struct ValidationContext<'a> {
    pub template: &'a TemplateSnapshot,
    pub template_metadata: &'a TemplateMetadata,
    pub base: &'a ProjectSnapshot,
    pub proposal: &'a PatchProposal,
    /// The in-memory draft produced by applying the proposal.
    pub draft: &'a serde_json::Value,
    /// Panels the patch engine actually modified (global token ops touch
    /// every panel containing the token; declared panels alone are not enough).
    pub applied_touched_panels: BTreeSet<u32>,
    /// sha256 of the template revision as currently stored — Reference
    /// Integrity compares this with the project's source reference.
    pub current_template_sha: &'a str,
    pub config: ValidatorConfig,
}

pub fn validate(ctx: &ValidationContext) -> ValidationReport {
    let mut schema_gate = gate_schema(ctx);
    let scope_gate = gate_scope(ctx);
    let anti_rewrite = gate_anti_rewrite(ctx);
    let identity_leak = gate_identity_leak(ctx);
    let scene_leak = gate_scene_leak(ctx);
    let reference = gate_reference_integrity(ctx);
    let json_parse = gate_json_parse(ctx);
    let passed = [(&schema_gate), (&scope_gate), (&anti_rewrite), (&identity_leak), (&scene_leak), (&reference), (&json_parse)]
        .iter()
        .all(|g| g.passed);
    let ratio = anti_rewrite_preservation(ctx);
    ValidationReport {
        passed,
        schema: schema_gate,
        scope: scope_gate,
        anti_rewrite,
        identity_leak,
        scene_leak,
        reference_integrity: reference,
        json_parse,
        preservation_ratio: ratio,
    }
}

fn gate_schema(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("schema");
    let issues = schema::validate_storyboard_json(&ctx.draft);
    if issues.is_empty() {
        return g;
    }
    for i in issues {
        g.fail(i.to_string());
    }
    g
}

fn gate_scope(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("scope");
    let p = ctx.proposal;
    let touched: BTreeSet<u32> = p.effective_touched_panels();

    for op in &p.operations {
        match &op.kind {
            OperationKind::ResizeStoryboard { .. } => {
                if !p.user_requested_resize {
                    g.fail(format!(
                        "{}: ResizeStoryboard requires an explicit user request",
                        op.common.operation_id
                    ));
                }
            }
            OperationKind::ReplaceSceneToken { .. } => {
                if !matches!(p.intent, PatchIntent::SceneAdapt | PatchIntent::CharacterAndScene) {
                    g.fail(format!(
                        "{}: scene replacement outside a scene-intent patch",
                        op.common.operation_id
                    ));
                }
            }
            OperationKind::ReplaceCharacterIdentity { .. } => {
                if !matches!(
                    p.intent,
                    PatchIntent::CharacterReplace | PatchIntent::CharacterAndScene
                ) {
                    g.fail(format!(
                        "{}: identity replacement outside a character-intent patch",
                        op.common.operation_id
                    ));
                }
            }
            _ => {}
        }
        // panel-scoped edits must be declared
        if let Some(idx) = op.common.panel_index {
            if matches!(
                op.kind,
                OperationKind::PatchPromptBlock { .. } | OperationKind::DeleteConflictingBlock { .. }
            ) && !touched.contains(&idx)
            {
                g.fail(format!(
                    "{}: panel {idx} edited but not declared in touched_panels",
                    op.common.operation_id
                ));
            }
        }
    }
    g
}

fn token_set(text: &str) -> Vec<String> {
    text.split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

#[derive(Clone, Copy)]
enum Side {
    Old,
    New,
}

fn replacement_tokens(p: &PatchProposal, side: Side) -> Vec<String> {
    p.operations
        .iter()
        .filter_map(|o| match &o.kind {
            OperationKind::ReplaceCharacterIdentity { replacements, .. }
            | OperationKind::ReplaceSceneToken { replacements } => Some(replacements.iter()),
            _ => None,
        })
        .flatten()
        .map(|r| {
            let s = match side {
                Side::Old => r.old_token.as_str(),
                Side::New => r.new_token.as_str(),
            };
            s.to_lowercase()
        })
        .collect()
}

/// Preservation ratio over non-target blocks (plan §13.1). Target tokens are
/// the mapping old/new sides; everything else must survive.
fn anti_rewrite_preservation(ctx: &ValidationContext) -> f32 {
    let p = &ctx.proposal;
    let touched: BTreeSet<u32> = p.effective_touched_panels().union(&ctx.applied_touched_panels).cloned().collect();
    let target_tokens: BTreeSet<String> = replacement_tokens(p, Side::Old)
        .into_iter()
        .chain(replacement_tokens(p, Side::New))
        .collect();

    let base_panels = ctx.base.raw.get("panels").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let new_panels = ctx.draft.get("panels").and_then(|x| x.as_array()).cloned().unwrap_or_default();

    let mut kept = 0usize;
    let mut total = 0usize;
    let is_target = |tok: &str| target_tokens.iter().any(|t| tok.contains(t.as_str()) || t.contains(tok));
    for (i, (b, n)) in base_panels.iter().zip(new_panels.iter()).enumerate() {
        if !touched.contains(&((i + 1) as u32)) {
            continue; // handled by the byte-identity rule in the gate
        }
        let mut texts: Vec<(String, String)> = Vec::new();
        if let (Some(bp), Some(np)) = (
            b.get("prompt").and_then(|v| v.as_str()),
            n.get("prompt").and_then(|v| v.as_str()),
        ) {
            texts.push((bp.to_lowercase(), np.to_lowercase()));
        }
        let bcc = b.get("customCharacters").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        let ncc = n.get("customCharacters").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        for (bc, nc) in bcc.iter().zip(ncc.iter()) {
            if let (Some(bp), Some(np)) = (
                bc.get("prompt").and_then(|v| v.as_str()),
                nc.get("prompt").and_then(|v| v.as_str()),
            ) {
                texts.push((bp.to_lowercase(), np.to_lowercase()));
            }
        }
        for (before, after) in texts {
            let bt = token_set(&before);
            let at: BTreeSet<String> = token_set(&after).into_iter().collect();
            for t in bt {
                if is_target(&t) {
                    continue;
                }
                total += 1;
                if at.contains(&t) {
                    kept += 1;
                }
            }
        }
    }
    if total == 0 {
        1.0
    } else {
        kept as f32 / total as f32
    }
}

fn gate_anti_rewrite(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("anti_rewrite");
    let p = &ctx.proposal;
    let touched: BTreeSet<u32> = p.effective_touched_panels().union(&ctx.applied_touched_panels).cloned().collect();

    let base_panels = ctx.base.raw.get("panels").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let new_panels = ctx.draft.get("panels").and_then(|x| x.as_array()).cloned().unwrap_or_default();

    // 1. untouched panels must be byte-stable (prompt + CC prompts + camera)
    for (i, (b, n)) in base_panels.iter().zip(new_panels.iter()).enumerate() {
        let idx = (i + 1) as u32;
        if touched.contains(&idx) {
            continue;
        }
        if b.get("prompt") != n.get("prompt") {
            g.fail(format!("panel {idx} is not a declared target but its prompt changed"));
        }
        if b.get("customCharacters") != n.get("customCharacters") {
            g.fail(format!("panel {idx} is not a declared target but its customCharacters changed"));
        }
    }

    // 2. camera tokens must survive everywhere
    let cam_tokens: Vec<String> = ctx
        .template_metadata
        .camera_profile
        .iter()
        .filter(|c| c.len() >= 4)
        .cloned()
        .collect();
    if !cam_tokens.is_empty() {
        let cam_base = count_tokens_in_panels(&base_panels, &cam_tokens);
        let cam_new = count_tokens_in_panels(&new_panels, &cam_tokens);
        if cam_new < cam_base {
            g.fail(format!(
                "camera profile tokens dropped: {} -> {} (camera schedule must inherit)",
                cam_base, cam_new
            ));
        }
    }

    // 3. globalNegativePrompt untouched
    if ctx.base.raw.get("globalNegativePrompt") != ctx.draft.get("globalNegativePrompt") {
        g.fail("globalNegativePrompt must inherit verbatim".into());
    }

    // 4. panel count stability unless a resize op is in scope
    let has_resize = p.operations.iter().any(|o| matches!(o.kind, OperationKind::ResizeStoryboard { .. }));
    if !has_resize && base_panels.len() != new_panels.len() {
        g.fail(format!("panel count changed without a resize operation ({} -> {})", base_panels.len(), new_panels.len()));
    }

    // 5. preservation ratio on touched panels
    let ratio = anti_rewrite_preservation(ctx);
    let min = match p.intent {
        PatchIntent::CharacterReplace | PatchIntent::UserDelta => ctx.config.identity_preservation_min,
        PatchIntent::SceneAdapt | PatchIntent::CharacterAndScene => ctx.config.scene_preservation_min,
        PatchIntent::Resize => ctx.config.scene_preservation_min,
    };
    if ratio < min {
        g.fail(format!(
            "non-target token preservation {:.3} below threshold {:.2}",
            ratio, min
        ));
    } else if ratio < min + 0.03 {
        g.warn(format!("preservation {:.3} close to threshold {:.2}", ratio, min));
    }
    g
}

fn count_tokens_in_panels(panels: &[serde_json::Value], tokens: &[String]) -> usize {
    let mut count = 0;
    for p in panels {
        let hay = p.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        for t in tokens {
            count += hay.matches(t.as_str()).count();
        }
    }
    count
}

fn gate_identity_leak(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("identity_leak");
    let p = &ctx.proposal;
    let draft_text = project_text(&ctx.draft);

    // Old tokens that were explicitly replaced must not remain.
    let old_tokens = replacement_tokens(p, Side::Old);
    for t in &old_tokens {
        let hits = whole_token_hits(&draft_text, t);
        if hits > 0 {
            g.fail(format!("replaced identity token `{t}` still present ({hits} hit(s))"));
        }
    }
    let new_tokens: BTreeSet<String> = replacement_tokens(p, Side::New).into_iter().collect();

    // When identity is being replaced, template anchors must be gone — unless
    // they live on inside a new anchor (kept character variant).
    let identity_intent = matches!(p.intent, PatchIntent::CharacterReplace | PatchIntent::CharacterAndScene);
    if identity_intent {
        for anchor in ctx
            .template_metadata
            .character_anchor_variants
            .iter()
            .chain(ctx.template_metadata.character_anchors.iter())
        {
            let a = anchor.to_lowercase();
            let survives_in_new = new_tokens.iter().any(|n| n.contains(&a));
            if survives_in_new {
                continue;
            }
            if whole_token_hits(&draft_text, &a) > 0 {
                g.fail(format!("template identity anchor `{anchor}` leaked after replacement"));
            }
        }
    }
    g
}

fn gate_scene_leak(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("scene_leak");
    let p = &ctx.proposal;
    let draft_text = project_text(&ctx.draft);

    // explicitly replaced scene tokens must be gone
    let scene_replaced: Vec<String> = p
        .operations
        .iter()
        .filter_map(|o| match &o.kind {
            OperationKind::ReplaceSceneToken { replacements } => {
                Some(replacements.iter().map(|r| r.old_token.to_lowercase()))
            }
            _ => None,
        })
        .flatten()
        .collect();
    for t in &scene_replaced {
        let hits = whole_token_hits(&draft_text, t);
        if hits > 0 {
            g.fail(format!("replaced scene token `{t}` still present ({hits} hit(s))"));
        }
    }

    // metadata scene scan: warnings (v1) — generic words would over-block
    let scene_intent = matches!(p.intent, PatchIntent::SceneAdapt | PatchIntent::CharacterAndScene);
    if scene_intent {
        for tok in ctx
            .template_metadata
            .location_tags
            .iter()
            .chain(ctx.template_metadata.important_props.iter())
        {
            let t = tok.to_lowercase();
            if t.len() < 4 {
                continue;
            }
            let survived_as_new = replacement_tokens(p, Side::New).iter().any(|n| n.contains(&t));
            if survived_as_new {
                continue;
            }
            if whole_token_hits(&draft_text, &t) > 0 {
                let msg = format!("old scene token `{tok}` still present in draft");
                if ctx.config.strict_metadata_leak_scan {
                    g.fail(msg);
                } else {
                    g.warn(msg);
                }
            }
        }
    }
    g
}

fn gate_reference_integrity(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("reference_integrity");
    if ctx.base.source.sha256 != ctx.current_template_sha {
        g.fail(format!(
            "project baselines on template sha `{}` but the stored revision is `{}` — re-clone required",
            ctx.base.source.sha256, ctx.current_template_sha
        ));
    }
    if ctx.proposal.primary_template_id.as_str() != ctx.base.source.template_id.as_str() {
        // primary template mismatch between proposal and project source
        g.fail(format!(
            "proposal names primary template {} but the project was cloned from {}",
            ctx.proposal.primary_template_id, ctx.base.source.template_id
        ));
    }
    g
}

fn gate_json_parse(ctx: &ValidationContext) -> GateResult {
    let mut g = GateResult::new("json_parse");
    match serde_json::to_vec(&ctx.draft) {
        Ok(bytes) => {
            if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                g.fail("draft does not round-trip through serde_json".into());
            }
        }
        Err(e) => g.fail(format!("draft serialization failed: {e}")),
    }
    g
}

fn project_text(v: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = v.get("title").and_then(|t| t.as_str()) {
        parts.push(t.to_lowercase());
    }
    if let Some(panels) = v.get("panels").and_then(|p| p.as_array()) {
        for p in panels {
            if let Some(s) = p.get("prompt").and_then(|x| x.as_str()) {
                parts.push(s.to_lowercase());
            }
            if let Some(ccs) = p.get("customCharacters").and_then(|c| c.as_array()) {
                for cc in ccs {
                    if let Some(s) = cc.get("prompt").and_then(|x| x.as_str()) {
                        parts.push(s.to_lowercase());
                    }
                }
            }
        }
    }
    parts.join(" , ")
}

/// Count occurrences of `token` with non-word boundaries on both sides.
fn whole_token_hits(text: &str, token: &str) -> usize {
    if token.is_empty() {
        return 0;
    }
    let is_word = |c: char| c.is_alphanumeric();
    let mut count = 0;
    let bytes = text.as_bytes();
    let tb = token.as_bytes();
    let mut i = 0;
    while i + tb.len() <= bytes.len() {
        if &text.as_bytes()[i..i + tb.len()] == tb {
            let before_ok = i == 0 || !text[..i].chars().next_back().map(is_word).unwrap_or(false);
            let after_ok = text[i + tb.len()..].chars().next().map(|c| !is_word(c)).unwrap_or(true);
            if before_ok && after_ok {
                count += 1;
            }
            i += tb.len().max(1);
        } else {
            i += 1;
        }
    }
    count
}
