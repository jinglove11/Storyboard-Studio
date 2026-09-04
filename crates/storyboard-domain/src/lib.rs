//! Storyboard domain model.
//!
//! Core rule from the v2.1 architecture freeze: template originals are the
//! single source of truth and are immutable. Everything in this crate is a
//! typed view over the original JSON (`serde_json::Value` with
//! `preserve_order`), so round-tripping a file never reorders author fields.

pub mod diff;
pub mod events;
pub mod ids;
pub mod manifest;
pub mod patch;
pub mod project;
pub mod query;
pub mod schema;
pub mod template;

pub use diff::{FieldChange, PanelChange, PanelDiff, ProjectDiff, TokenChange, TokenDiff};
pub use events::AuditEvent;
pub use ids::{ProjectId, RevisionId, TemplateId, VersionNumber};
pub use manifest::AgentRunManifest;
pub use patch::{
    OperationKind, PatchError, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal,
    SeedStrategy, TextTarget, TokenReplacement,
};
pub use project::{ProjectSnapshot, ProjectState, ProjectStatus, StatusTransitionError};
pub use query::QueryIntent;
pub use schema::{SchemaFingerprint, SchemaIssue, EXPECTED_CC_KEYS, EXPECTED_GLOBAL_PARAM_KEYS,
    EXPECTED_PANEL_KEYS, EXPECTED_PARAMS_OVERRIDE_KEYS, EXPECTED_TOP_KEYS};
pub use template::{LegacyStats, SceneAliasTable, SourceTemplateRef, TemplateMetadata, TemplateSnapshot};

/// sha256 of the raw bytes, hex encoded (lowercase). Used for template
/// immutability checks and content-addressed storage.
pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}
