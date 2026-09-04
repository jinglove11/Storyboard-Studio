//! Storyboard Semantic Patch engine (plan §12).
//!
//! Agents never rewrite project JSON directly; they emit typed
//! `PatchProposal`s. This engine verifies preconditions (F03) and applies
//! operations to an **in-memory draft only**. Persistence (atomic commit,
//! versioning) belongs to the Application Controller.

pub mod diff;
mod engine;
mod resize;
mod token;

pub use diff::diff_projects;
pub use engine::{apply_proposal, PatchApplication};

/// sha256 hex of a string (for expected_old_hash preconditions).
pub fn text_hash(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}
