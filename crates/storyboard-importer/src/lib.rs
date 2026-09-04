//! Phase 0 importer: turns the frozen `.skill` bundle into immutable,
//! content-addressed template revisions with rebuilt metadata.
//!
//! Plan §2.2 / P0-03: character statistics are always recomputed by a
//! full-panel scan; legacy index counts are kept for audit only and never
//! trusted (Golden Case D).

pub mod metadata;
pub mod scan;
pub mod skill;

pub use metadata::{build_metadata, ImportWarning};
pub use scan::{scan_template, CharacterScan, ScanError, ScannedTemplate};
pub use skill::{IndexEntry, SkillBundle, SkillBundleError};

/// Fixture-relative default path of the frozen skill (plan §30).
pub const FIXTURE_DIR: &str = "fixtures/current-skill";
