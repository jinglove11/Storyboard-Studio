use crate::ids::{TemplateId, VersionNumber};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Why the patch exists; drives the Scope Gate and preservation thresholds
/// (plan §13.1: identity mode >= 0.90, scene mode >= 0.80).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchIntent {
    /// Mode A — only swap character identity.
    CharacterReplace,
    /// Mode B — swap identity + full scene mapping.
    SceneAdapt,
    /// User explicitly requested additions/removals.
    UserDelta,
    /// Character + scene.
    CharacterAndScene,
    /// Panel count change; high risk, always requires explicit user request.
    Resize,
}

/// Preconditions shared by every operation (plan §12.1, F03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOperationCommon {
    pub operation_id: String,
    /// 1-based panel index this op targets; None = project-global op.
    pub panel_index: Option<u32>,
    /// Current UUID of the targeted panel (when applicable). If set, it must
    /// still match the panel at `panel_index` — mismatch = stale patch.
    pub panel_id: Option<String>,
    /// Text anchor locating the block inside the target text (when the op
    /// mutates existing content).
    pub anchor: Option<String>,
    /// Exact current text expected at the anchor.
    pub expected_old: Option<String>,
    /// sha256(expected_old), alternative to expected_old.
    pub expected_old_hash: Option<String>,
    /// Project version this operation was authored against.
    pub expected_project_version: VersionNumber,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenReplacement {
    /// Whole-token match, e.g. `nakano miku` or `official style, nakano miku (school uniform)`.
    pub old_token: String,
    pub new_token: String,
}

/// Where a text edit lands inside a panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum TextTarget {
    PanelPrompt,
    CharacterSlot { slot: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedStrategy {
    /// Fresh random seeds, no duplicates (clone default).
    RandomNonRepeating,
    /// One fixed seed everywhere.
    Fixed(u64),
    /// Leave seeds untouched.
    Keep,
}

/// v1 typed operations (plan Table 10). No ArbitraryJsonReplace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationKind {
    /// Replace identity anchors/inherent looks/inherent outfit tokens.
    ReplaceCharacterIdentity {
        replacements: Vec<TokenReplacement>,
        /// CC slot indices the replacement applies to (default: all).
        slots: Option<Vec<u32>>,
    },
    /// Scene mapping replacements (location/environment/scene props).
    ReplaceSceneToken { replacements: Vec<TokenReplacement> },
    /// Minimal text block edit on one panel at an explicit anchor.
    PatchPromptBlock { target: TextTarget, new_text: String },
    /// Update project title (and every panel title — schema requires consistency).
    UpdateTitle { new_title: String },
    /// New project + panel UUIDs.
    RegenerateIds,
    /// Re-roll seeds per strategy.
    RegenerateSeeds { strategy: SeedStrategy },
    /// User-requested panel count change. Requires `user_requested_resize`.
    ResizeStoryboard { target_panel_count: u32 },
    /// Remove a block that conflicts with an explicit user request.
    DeleteConflictingBlock { target: TextTarget },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOperation {
    pub common: PatchOperationCommon,
    #[serde(flatten)]
    pub kind: OperationKind,
}

impl PatchOperation {
    pub fn touches_panel(&self) -> Option<u32> {
        match &self.kind {
            OperationKind::UpdateTitle { .. }
            | OperationKind::RegenerateIds
            | OperationKind::RegenerateSeeds { strategy: SeedStrategy::Keep }
            | OperationKind::ResizeStoryboard { .. } => None,
            _ => self.common.panel_index,
        }
    }

    /// Panels whose *content* is modified (excludes pure id/seed re-rolls,
    /// which do not affect prompt preservation accounting).
    pub fn content_touched_panel(&self) -> Option<u32> {
        match &self.kind {
            OperationKind::RegenerateIds | OperationKind::RegenerateSeeds { .. } => None,
            _ => self.common.panel_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchProposal {
    pub base_project_version: VersionNumber,
    pub primary_template_id: TemplateId,
    pub intent_hash: String,
    pub intent: PatchIntent,
    pub operations: Vec<PatchOperation>,
    /// Panels the proposal declares as modification targets. The Anti-Rewrite
    /// gate holds every other panel to byte-level stability.
    pub touched_panels: Vec<u32>,
    pub expected_preservation_ratio: f32,
    pub rationale: Vec<String>,
    /// Must be true for ResizeStoryboard ops to pass the Scope Gate.
    pub user_requested_resize: bool,
}

impl PatchProposal {
    /// Union of declared + actually-touched panels; validators compare against this.
    pub fn effective_touched_panels(&self) -> BTreeSet<u32> {
        let mut s: BTreeSet<u32> = self.touched_panels.iter().copied().collect();
        for op in &self.operations {
            if let Some(p) = op.content_touched_panel() {
                s.insert(p);
            }
        }
        s
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum PatchError {
    /// base_project_version != current version (plan §12.3).
    #[error("STALE_PATCH: proposal targets v{expected} but project is at v{current}")]
    StalePatch { expected: VersionNumber, current: VersionNumber },
    /// expected_old / expected_old_hash / panel_id no longer match (F03).
    #[error("PRECONDITION_FAILED: {op_id}: {reason}")]
    PreconditionFailed { op_id: String, reason: String },
    #[error("ANCHOR_NOT_FOUND: {op_id}: {reason}")]
    AnchorNotFound { op_id: String, reason: String },
    #[error("AMBIGUOUS_ANCHOR: {op_id}: anchor occurs {count} times in target")]
    AmbiguousAnchor { op_id: String, count: usize },
    #[error("TARGET_MISSING: {op_id}: {reason}")]
    TargetMissing { op_id: String, reason: String },
    #[error("INVALID_OPERATION: {op_id}: {reason}")]
    InvalidOperation { op_id: String, reason: String },
    #[error("IO: {0}")]
    Io(String),
}
