use serde::{Deserialize, Serialize};

/// Deterministic query vector parsed from user intent (plan §9.1).
/// v1 parsing is rule-based (P2-01); the agent may refine semantics later.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryIntent {
    pub scene_family: Option<String>,
    pub exact_scene: Option<String>,
    pub time: Option<String>,
    pub character_count: Option<u32>,
    pub character_roles: Vec<String>,
    pub narrative_tags: Vec<String>,
    pub pace_hint: Option<String>,
    pub desired_panel_count: Option<u32>,
    pub props: Vec<String>,
    pub camera_hints: Vec<String>,
    pub seed: Option<u64>,
    /// free-form tokens kept for keyword fallback
    pub keywords: Vec<String>,
}
