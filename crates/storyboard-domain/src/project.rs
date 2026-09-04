use crate::ids::{ProjectId, TemplateId, VersionNumber};
use crate::template::SourceTemplateRef;
use serde::{Deserialize, Serialize};

/// A loaded project version snapshot. `raw` is the full storyboard JSON
/// exactly as committed (order-preserving).
#[derive(Debug, Clone)]
pub struct ProjectSnapshot {
    pub project_id: ProjectId,
    pub version: VersionNumber,
    pub title: String,
    pub source: SourceTemplateRef,
    pub raw: serde_json::Value,
}

impl ProjectSnapshot {
    pub fn panels(&self) -> &[serde_json::Value] {
        self.raw
            .get("panels")
            .and_then(|p| p.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[])
    }

    pub fn panel_count(&self) -> u32 {
        self.panels().len() as u32
    }

    pub fn primary_template_id(&self) -> TemplateId {
        TemplateId::new(self.source.template_id.as_str())
    }
}

/// Project state machine (plan §21). Transitions are driven by the Rust core
/// only; React renders states, never infers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Draft,
    Matched,
    Cloned,
    PatchProposed,
    Validating,
    PatchRejected,
    AwaitingApproval,
    AutoApproved,
    CommitRequested,
    Committed,
    Versioned,
    Exported,
}

#[derive(Debug, thiserror::Error)]
#[error("illegal status transition {from:?} -> {to:?}")]
pub struct StatusTransitionError {
    pub from: ProjectStatus,
    pub to: ProjectStatus,
}

impl ProjectStatus {
    /// Allowed transitions from the frozen plan §21 diagram (plus the retry
    /// edge PatchRejected -> PatchProposed).
    pub fn can_transition_to(&self, to: ProjectStatus) -> bool {
        use ProjectStatus::*;
        matches!(
            (self, to),
            (Draft, Matched)
                | (Matched, Cloned)
                | (Draft, Cloned)
                | (Cloned, PatchProposed)
                | (PatchProposed, Validating)
                | (Validating, PatchRejected)
                | (Validating, AwaitingApproval)
                | (Validating, AutoApproved)
                | (PatchRejected, PatchProposed)
                | (PatchRejected, Cloned)
                | (AwaitingApproval, CommitRequested)
                | (AutoApproved, CommitRequested)
                | (PatchRejected, CommitRequested) // user overrides a FAIL
                | (CommitRequested, Committed)
                | (Committed, Versioned)
                | (Versioned, Exported)
                | (Committed, PatchProposed) // next edit round
                | (Versioned, PatchProposed)
                | (Versioned, Cloned) // rollback target reuse
        )
    }

    pub fn transition(&self, to: ProjectStatus) -> Result<ProjectStatus, StatusTransitionError> {
        if self.can_transition_to(to) {
            Ok(to)
        } else {
            Err(StatusTransitionError { from: *self, to })
        }
    }
}

/// Persisted per-project state row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectState {
    pub project_id: ProjectId,
    pub title: String,
    pub status: ProjectStatus,
    pub current_version: VersionNumber,
    pub source_template_id: TemplateId,
    pub source_revision_id: String,
    pub created_at: String,
    pub updated_at: String,
}
