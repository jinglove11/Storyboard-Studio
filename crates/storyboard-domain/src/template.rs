use crate::ids::{RevisionId, TemplateId, VersionNumber};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An immutable, content-addressed template revision loaded from the
/// originals store. The raw value is never mutated anywhere in the system.
#[derive(Debug, Clone)]
pub struct TemplateSnapshot {
    pub id: TemplateId,
    pub revision_id: RevisionId,
    pub sha256: String,
    /// Raw author JSON (order-preserving). Clone Engine deep-copies this.
    pub raw: serde_json::Value,
}

impl TemplateSnapshot {
    pub fn title(&self) -> &str {
        self.raw.get("title").and_then(|t| t.as_str()).unwrap_or("")
    }

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

    pub fn panel_prompt(&self, index0: usize) -> Option<&str> {
        self.panels()
            .get(index0)?
            .get("prompt")
            .and_then(|p| p.as_str())
    }

    /// `slot` is 0-based. Returns None when the panel has no such slot.
    pub fn cc_prompt(&self, index0: usize, slot: usize) -> Option<&str> {
        self.panels()
            .get(index0)?
            .get("customCharacters")
            .and_then(|c| c.as_array())?
            .get(slot)?
            .get("prompt")
            .and_then(|p| p.as_str())
    }

    pub fn global_negative_prompt(&self) -> &str {
        self.raw.get("globalNegativePrompt").and_then(|t| t.as_str()).unwrap_or("")
    }
}

/// Rebuilt template metadata produced by the Importer's full-panel scan
/// (plan §10.3 / Table 9). Character statistics are recomputed from the
/// actual panels — never trusted from the legacy index (plan §2.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    pub template_id: String,
    pub revision_id: String,
    pub title: String,
    pub source_name: String,
    pub sha256: String,
    pub schema_fingerprint: String,

    // scene
    pub scene_family: String,
    pub exact_scene: Option<String>,
    pub scene_tags: Vec<String>,
    pub location_tags: Vec<String>,
    pub time_tags: Vec<String>,
    pub environment_tags: Vec<String>,

    // characters (rescanned)
    pub total_role_count: u32,
    pub female_lead_count: Option<u32>,
    pub male_lead_count: Option<u32>,
    pub max_simultaneous_slots: u32,
    /// Base identity anchors, e.g. `azki` (parenthetical costume notes stripped).
    pub character_anchors: Vec<String>,
    /// Full anchor strings as they appear in CC prompts, e.g.
    /// `azki (4th costume) (hololive)`. Used by replacement/leak scanning.
    pub character_anchor_variants: Vec<String>,
    pub male_identity: Option<String>,
    pub male_panel_ratio: Option<f32>,

    // structure
    pub panel_count: u32,
    pub narrative_type: Option<String>,
    pub opening_type: Option<String>,
    pub ending_type: Option<String>,
    pub pace: String,
    pub first_sex_panel: Option<u32>,
    pub pov_ratio: Option<f32>,
    pub torogao_coverage: Option<f32>,
    pub camera_profile: Vec<String>,
    pub camera_profile_freq: BTreeMap<String, u32>,
    pub composition_profile: Vec<String>,
    pub clothing_arc: Option<String>,
    pub interaction_profile: Vec<String>,
    pub important_props: Vec<String>,
    pub keywords: Vec<String>,
    /// Aspect ratio profile, e.g. ["landscape(1216x832)", "portrait(832x1216)"]
    /// ordered by frequency.
    pub aspect_ratio_profile: Vec<String>,

    // quality
    pub metadata_confidence: f32,
    pub warnings: Vec<String>,
    pub reviewed_at: Option<String>,

    /// Legacy index entry kept verbatim for audit (Golden Case D).
    pub legacy: Option<LegacyStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyStats {
    pub character_count: u32,
    pub female_character_count: u32,
    pub male_character_count: u32,
    pub female_anchors: Vec<String>,
    pub mismatches: Vec<String>,
}

/// Alias table from the frozen skill's `template-index.json`. The matcher
/// must consult this table at runtime instead of hard-coded synonyms
/// (template-selection.md §1.2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneAliasTable {
    /// family -> aliases (aliases include the family name itself)
    pub families: BTreeMap<String, Vec<String>>,
}

impl SceneAliasTable {
    pub fn from_pairs(pairs: BTreeMap<String, Vec<String>>) -> Self {
        let mut families = pairs;
        for (family, aliases) in families.iter_mut() {
            if !aliases.iter().any(|a| a.eq_ignore_ascii_case(family)) {
                aliases.push(family.clone());
            }
        }
        Self { families }
    }

    /// Convenience constructor from `(family, aliases)` tuples.
    pub fn from_pair_list(pairs: Vec<(&str, Vec<&str>)>) -> Self {
        Self::from_pairs(
            pairs
                .into_iter()
                .map(|(f, a)| (f.to_string(), a.into_iter().map(String::from).collect()))
                .collect(),
        )
    }

    /// Normalize a raw location token to a scene family, if known.
    pub fn normalize(&self, token: &str) -> Option<String> {
        let t = token.trim().to_lowercase();
        for (family, aliases) in &self.families {
            for a in aliases {
                if a.to_lowercase() == t {
                    return Some(family.clone());
                }
            }
        }
        None
    }

    pub fn families(&self) -> Vec<String> {
        self.families.keys().cloned().collect()
    }
}

/// Reference to the project version a patch/clone was based on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTemplateRef {
    pub template_id: TemplateId,
    pub revision_id: RevisionId,
    pub sha256: String,
}

pub type ProjectVersionRef = VersionNumber;
