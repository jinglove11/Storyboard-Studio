use crate::SplitMix64;
use serde::{Deserialize, Serialize};
use storyboard_domain::{QueryIntent, SceneAliasTable, TemplateMetadata};

/// Default weights per plan Table 8; user-adjustable in Settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub scene: f32,
    pub structure: f32,
    pub characters: f32,
    pub time: f32,
    pub pace: f32,
    pub camera_props: f32,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self { scene: 35.0, structure: 20.0, characters: 15.0, time: 10.0, pace: 10.0, camera_props: 10.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MatcherConfig {
    pub weights: ScoreWeights,
    pub top_k: usize,
    /// `top1 - top2 >= threshold` → pick top1 without randomization.
    pub dominance_threshold: f32,
    /// Below this final score the selection is flagged `needs_scene_adaptation`.
    pub min_score: f32,
}

impl Default for MatcherConfig {
    fn default() -> Self {
        Self { weights: ScoreWeights::default(), top_k: 3, dominance_threshold: 0.15, min_score: 0.55 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub scene: f32,
    pub structure: f32,
    pub characters: f32,
    pub time: f32,
    pub pace: f32,
    pub camera_props: f32,
    /// Human-readable explanation per dimension (UI score-breakdown panel).
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub template_id: String,
    pub title: String,
    /// 0.0..=1.0
    pub score: f32,
    pub breakdown: ScoreBreakdown,
    pub scene_family: String,
    pub panel_count: u32,
    pub total_role_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    /// Top1 dominated outright.
    Deterministic,
    /// Weighted random inside Top-K.
    WeightedRandom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub primary: Candidate,
    pub candidates: Vec<Candidate>,
    pub mode: MatchMode,
    /// score < min_score — caller must surface the Scene Adaptation warning
    /// (template-selection.md §2).
    pub needs_scene_adaptation: bool,
}

pub struct Matcher {
    config: MatcherConfig,
    aliases: SceneAliasTable,
    templates: Vec<TemplateMetadata>,
}

impl Matcher {
    pub fn new(config: MatcherConfig, aliases: SceneAliasTable, templates: Vec<TemplateMetadata>) -> Self {
        Self { config, aliases, templates }
    }

    pub fn config(&self) -> &MatcherConfig {
        &self.config
    }

    /// Score every template, apply the scene-family hard filter when the user
    /// named a location, return sorted Top-K (plan §9.3).
    pub fn top_k(&self, q: &QueryIntent) -> Vec<Candidate> {
        let family = q.scene_family.as_deref();
        let mut cands: Vec<Candidate> = self
            .templates
            .iter()
            .filter(|m| family.map(|f| self.family_compatible(f, m)).unwrap_or(true))
            .map(|m| self.score(m, q))
            .collect();
        if cands.is_empty() {
            // No compatible template: fall back to the whole library with the
            // scene component forced to zero — never pretend a perfect match.
            if let Some(f) = family {
                cands = self
                    .templates
                    .iter()
                    .map(|m| {
                        let mut c = self.score(m, q);
                        c.breakdown.scene = 0.0;
                        c.score = self.normalized(&c.breakdown, q);
                        c.breakdown.reasons.push(format!(
                            "no template in family `{f}` — ranked library-wide, scene score zeroed"
                        ));
                        c
                    })
                    .collect();
            }
        }
        cands.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap().then(a.template_id.cmp(&b.template_id)));
        cands.truncate(self.config.top_k.max(1));
        cands
    }

    /// Select the Primary Template. `seed` controls the weighted-random path;
    /// passing the same seed always reproduces the same choice.
    pub fn select(&self, q: &QueryIntent, seed: Option<u64>) -> Option<Selection> {
        let mut candidates = self.top_k(q);
        if candidates.is_empty() {
            return None;
        }
        let top1 = candidates[0].score;
        let top2 = candidates.get(1).map(|c| c.score).unwrap_or(0.0);
        let (mode, chosen_idx) = if top1 - top2 >= self.config.dominance_threshold || candidates.len() == 1 {
            (MatchMode::Deterministic, 0)
        } else {
            let mut rng = SplitMix64::new(seed.unwrap_or(0x5EED_0000_0000_0001));
            let weights: Vec<f32> = candidates.iter().map(|c| c.score.max(1e-6)).collect();
            let total: f32 = weights.iter().sum();
            let pick = rng.next_f64() as f32 * total;
            let mut acc = 0.0f32;
            let mut idx = weights.len() - 1;
            for (i, w) in weights.iter().enumerate() {
                acc += w;
                if pick <= acc {
                    idx = i;
                    break;
                }
            }
            (MatchMode::WeightedRandom, idx)
        };
        let primary = candidates.remove(chosen_idx);
        Some(Selection {
            needs_scene_adaptation: primary.score < self.config.min_score,
            primary,
            candidates,
            mode,
        })
    }

    fn total(&self, b: &ScoreBreakdown) -> f32 {
        let w = &self.config.weights;
        let sum = b.scene.max(0.0) + b.structure + b.characters + b.time + b.pace + b.camera_props;
        (sum / 100.0).clamp(0.0, 1.0)
    }

    /// Normalized score: earned points / points possible given the dimensions
    /// the query actually specified. Ranking is unchanged (same numerator),
    /// but the 0.55 threshold keeps its meaning when e.g. only scene +
    /// narrative were given.
    fn normalized(&self, b: &ScoreBreakdown, q: &QueryIntent) -> f32 {
        let w = &self.config.weights;
        let mut possible = 0.0f32;
        if q.scene_family.is_some() || q.exact_scene.is_some() {
            possible += w.scene;
        }
        if !q.narrative_tags.is_empty() {
            possible += w.structure;
        }
        if q.character_count.is_some() || !q.character_roles.is_empty() {
            possible += w.characters;
        }
        if q.time.is_some() {
            possible += w.time;
        }
        if q.pace_hint.is_some() || q.desired_panel_count.is_some() {
            possible += w.pace;
        }
        if !q.camera_hints.is_empty() || !q.props.is_empty() {
            possible += w.camera_props;
        }
        if possible <= 0.0 {
            return 0.0;
        }
        let earned = b.scene.max(0.0) + b.structure + b.characters + b.time + b.pace + b.camera_props;
        (earned / possible).clamp(0.0, 1.0)
    }

    /// Hard filter (template-selection.md §1.3): same scene family, or the
    /// template's scene/exact/location tags mention any alias of the family
    /// (e.g. 保健室 → school templates whose location mentions medical room).
    fn family_compatible(&self, family: &str, m: &TemplateMetadata) -> bool {
        if m.scene_family.eq_ignore_ascii_case(family) {
            return true;
        }
        let aliases = match self.aliases.families.get(family) {
            Some(a) => a,
            None => return false,
        };
        let hay: Vec<String> = m
            .exact_scene
            .iter()
            .cloned()
            .chain(m.scene_tags.iter().cloned())
            .chain(m.location_tags.iter().cloned())
            .chain(m.environment_tags.iter().cloned())
            .collect();
        let hay_lower: Vec<String> = hay.iter().map(|s| s.to_lowercase()).collect();
        aliases.iter().any(|a| {
            let a_l = a.to_lowercase();
            hay_lower.iter().any(|h| h.contains(&a_l))
        })
    }

    fn score(&self, m: &TemplateMetadata, q: &QueryIntent) -> Candidate {
        #[allow(unused_variables)]
        let w = &self.config.weights;
        let mut reasons = Vec::new();

        // --- scene ---
        let scene = if q.scene_family.is_none() {
            0.0
        } else if let Some(exact) = &q.exact_scene {
            let e = exact.to_lowercase();
            let hit = m.exact_scene.as_deref().map(|s| s.to_lowercase().contains(&e)).unwrap_or(false)
                || m.location_tags.iter().any(|t| t.to_lowercase().contains(&e));
            if hit { w.scene } else { w.scene * 25.0 / 35.0 }
        } else if m.scene_family.eq_ignore_ascii_case(q.scene_family.as_deref().unwrap_or("")) {
            w.scene * 25.0 / 35.0
        } else {
            0.0
        };
        if scene > 0.0 {
            reasons.push(format!("scene: {} (family `{}`)", scene, m.scene_family));
        }

        // --- structure: narrative tags vs narrative_type/interaction/keywords ---
        let structure = if q.narrative_tags.is_empty() {
            0.0
        } else {
            let hay = format!(
                "{} {} {}",
                m.narrative_type.as_deref().unwrap_or(""),
                m.interaction_profile.join(" "),
                m.keywords.join(" ")
            )
            .to_lowercase();
            let hits = q.narrative_tags.iter().filter(|t| hay.contains(&t.to_lowercase())).count();
            let ratio = hits as f32 / q.narrative_tags.len() as f32;
            w.structure * ratio
        };

        // --- characters ---
        let characters = if let Some(n) = q.character_count {
            let diff = (n as i32 - m.total_role_count as i32).unsigned_abs();
            let count_score = match diff {
                0 => 10.0,
                1 => 6.0,
                _ => (10.0 - 2.5 * diff as f32).max(0.0),
            };
            let role_score = if q.character_roles.is_empty() {
                5.0 // neutral when user gave no role info
            } else {
                let male_hay = format!(
                    "{} {}",
                    m.male_identity.as_deref().unwrap_or(""),
                    m.narrative_type.as_deref().unwrap_or("")
                )
                .to_lowercase();
                let hits = q
                    .character_roles
                    .iter()
                    .filter(|r| {
                        let r = r.to_lowercase();
                        let base = r.split('_').next().unwrap_or(&r).to_string();
                        male_hay.contains(&base)
                            || m.character_anchors.iter().any(|a| a.to_lowercase().contains(&base))
                    })
                    .count();
                5.0 * (hits as f32 / q.character_roles.len() as f32)
            };
            count_score + role_score
        } else {
            0.0
        };

        // --- time ---
        let time = match (&q.time, m.time_tags.iter().any(|t| {
            let t = t.to_lowercase();
            q.time.as_deref().map(|qt| t.contains(&qt.to_lowercase()) || qt.to_lowercase().contains(&t)).unwrap_or(false)
        })) {
            (Some(_), true) => w.time,
            _ => 0.0,
        };

        // --- pace / panel count ---
        let pace = {
            let mut s = 0.0f32;
            if let Some(p) = &q.pace_hint {
                if m.pace.eq_ignore_ascii_case(p) {
                    s += 5.0;
                }
            }
            if let Some(desired) = q.desired_panel_count {
                let d = (desired as i32 - m.panel_count as i32).unsigned_abs() as f32;
                let max = desired.max(m.panel_count) as f32;
                s += 5.0 * (1.0 - d / max).max(0.0);
            }
            s.min(w.pace)
        };

        // --- camera / props ---
        let camera_props = {
            let mut s = 0.0f32;
            if !q.camera_hints.is_empty() {
                let hay = m.camera_profile.join(" ").to_lowercase();
                let hits = q.camera_hints.iter().filter(|c| hay.contains(&c.to_lowercase())).count();
                s += 5.0 * (hits as f32 / q.camera_hints.len() as f32);
            }
            if !q.props.is_empty() {
                let hay = format!(
                    "{} {} {}",
                    m.important_props.join(" "),
                    m.environment_tags.join(" "),
                    m.keywords.join(" ")
                )
                .to_lowercase();
                let hits = q.props.iter().filter(|p| hay.contains(&p.to_lowercase())).count();
                s += 5.0 * (hits as f32 / q.props.len() as f32);
            }
            s.min(w.camera_props)
        };

        let breakdown = ScoreBreakdown { scene, structure, characters, time, pace, camera_props, reasons };
        let score = self.normalized(&breakdown, q);
        Candidate {
            template_id: m.template_id.clone(),
            title: m.title.clone(),
            score,
            breakdown,
            scene_family: m.scene_family.clone(),
            panel_count: m.panel_count,
            total_role_count: m.total_role_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, family: &str, panels: u32, roles: u32) -> TemplateMetadata {
        serde_json::from_value(serde_json::json!({
            "template_id": id, "revision_id": "rev_x", "title": id, "source_name": "",
            "sha256": "x", "schema_fingerprint": "f",
            "scene_family": family, "exact_scene": null,
            "scene_tags": [], "location_tags": [], "time_tags": ["night"],
            "environment_tags": [],
            "total_role_count": roles, "female_lead_count": 1, "male_lead_count": 1,
            "max_simultaneous_slots": 2, "character_anchors": [], "character_anchor_variants": [],
            "male_identity": "anonymous man (faceless man)", "male_panel_ratio": 90.0,
            "panel_count": panels, "narrative_type": "night park rape",
            "opening_type": null, "ending_type": null, "pace": "standard",
            "first_sex_panel": 6, "pov_ratio": 50.0, "torogao_coverage": 30.0,
            "camera_profile": ["pov", "dutch angle"], "camera_profile_freq": {},
            "composition_profile": [], "clothing_arc": null,
            "interaction_profile": ["rape"], "important_props": ["park", "bush"],
            "keywords": ["park", "night", "rape"], "aspect_ratio_profile": [],
            "metadata_confidence": 1.0, "warnings": [], "reviewed_at": null, "legacy": null
        }))
        .unwrap()
    }

    fn aliases() -> SceneAliasTable {
        SceneAliasTable::from_pair_list(vec![
            ("park", vec!["park", "公园"]),
            ("office", vec!["office", "办公室"]),
        ])
    }

    #[test]
    fn hard_filter_excludes_other_families() {
        let m = Matcher::new(
            MatcherConfig::default(),
            aliases(),
            vec![meta("T010", "park", 83, 2), meta("T007", "office", 80, 2)],
        );
        let q = QueryIntent { scene_family: Some("park".into()), narrative_tags: vec!["rape".into()], ..Default::default() };
        let top = m.top_k(&q);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].template_id, "T010");
        assert!(top[0].score >= 0.55);
    }

    #[test]
    fn dominance_picks_top1_deterministically() {
        let mut t010 = meta("T010", "park", 83, 2);
        let mut t025 = meta("T025", "park", 90, 2);
        t025.narrative_type = Some("sleep touching".into());
        t025.interaction_profile = vec!["sleep".into()];
        t025.keywords = vec!["sleep".into()];
        let m = Matcher::new(
            MatcherConfig::default(),
            aliases(),
            vec![t010, t025],
        );
        // narrative narrows to T010 → dominance gap >= 0.15
        let q = QueryIntent {
            scene_family: Some("park".into()),
            narrative_tags: vec!["rape".into()],
            ..Default::default()
        };
        let sel = m.select(&q, None).unwrap();
        assert_eq!(sel.mode, MatchMode::Deterministic);
        assert_eq!(sel.primary.template_id, "T010");
    }

    #[test]
    fn weighted_random_is_reproducible() {
        let m = Matcher::new(
            MatcherConfig::default(),
            aliases(),
            vec![meta("T010", "park", 83, 2), meta("T025", "park", 90, 2), meta("T005", "park", 85, 2)],
        );
        // same-family candidates with no other query info → equal scores → random path
        let q = QueryIntent { scene_family: Some("park".into()), ..Default::default() };
        let a = m.select(&q, Some(7)).unwrap();
        let b = m.select(&q, Some(7)).unwrap();
        assert_eq!(a.primary.template_id, b.primary.template_id);
        let c = m.select(&q, Some(123456)).unwrap();
        // different seed may pick another candidate (not asserted, just no panic)
        assert!(c.primary.score >= 0.0);
    }

    #[test]
    fn unknown_family_falls_back_with_low_score() {
        let m = Matcher::new(
            MatcherConfig::default(),
            aliases(),
            vec![meta("T010", "park", 83, 2)],
        );
        let q = QueryIntent { scene_family: Some("volcano".into()), ..Default::default() };
        let sel = m.select(&q, None).unwrap();
        assert!(sel.needs_scene_adaptation);
    }
}
