//! Clone Engine (plan §11). Fully deterministic — no LLM involved.
//!
//! Guarantees:
//! - schema fields preserved (key layout identical to the template)
//! - panel ordering preserved
//! - original prompt text preserved byte-for-byte
//! - template file untouched (engine only reads the snapshot)
//! - v1 persisted atomically by the storage layer, never by the agent

use storyboard_domain::{ProjectId, SeedStrategy, TemplateSnapshot};
use storyboard_domain::template::SourceTemplateRef;
use uuid::Uuid;

/// Deterministic v4-format UUID derived from a seeded RNG so clones are
/// reproducible in tests and audit replays.
fn uuid_from_rng(rng: &mut storyboard_matcher_free::SplitMix64) -> Uuid {
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
    b[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 1
    Uuid::from_bytes(b)
}

// The matcher crate owns the RNG type; re-declare a private copy here so the
// clone engine stays dependency-light and self-contained.
mod storyboard_matcher_free {
    #[derive(Debug, Clone)]
    pub struct SplitMix64 {
        state: u64,
    }
    impl SplitMix64 {
        pub fn new(seed: u64) -> Self {
            Self { state: seed }
        }
        pub fn next_u64(&mut self) -> u64 {
            self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = self.state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        }
    }
}

#[derive(Debug, Clone)]
pub struct CloneOptions {
    /// New project title; also written to every panel title (schema requires
    /// consistency). Defaults to the template title.
    pub title: Option<String>,
    pub seed_strategy: SeedStrategy,
    /// Drives UUID + seed generation; identical seeds reproduce identical
    /// clones (tests, audit replay).
    pub rng_seed: u64,
}

impl Default for CloneOptions {
    fn default() -> Self {
        Self {
            title: None,
            seed_strategy: SeedStrategy::RandomNonRepeating,
            rng_seed: 0xC10E_5EED_0000_0001,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CloneSummary {
    pub project_id: String,
    pub new_panel_ids: usize,
    pub seeds_regenerated: usize,
    pub title: String,
    pub panel_count: u32,
}

#[derive(Debug, Clone)]
pub struct ClonedProject {
    pub project_id: ProjectId,
    pub raw: serde_json::Value,
    pub source: SourceTemplateRef,
    pub summary: CloneSummary,
}

#[derive(Debug, thiserror::Error)]
pub enum CloneError {
    #[error("template snapshot is missing panels")]
    NoPanels,
}

pub struct CloneEngine;

impl CloneEngine {
    /// Deep-copy the template and produce the project draft (v1 content).
    pub fn clone_template(
        snapshot: &TemplateSnapshot,
        opts: &CloneOptions,
    ) -> Result<ClonedProject, CloneError> {
        if snapshot.panels().is_empty() {
            return Err(CloneError::NoPanels);
        }
        let mut rng = storyboard_matcher_free::SplitMix64::new(opts.rng_seed);
        let mut raw = snapshot.raw.clone(); // deep copy, order-preserving

        // fresh project id + title
        let project_uuid = uuid_from_rng(&mut rng);
        raw["id"] = serde_json::Value::String(project_uuid.to_string());
        let title = opts.title.clone().unwrap_or_else(|| snapshot.title().to_string());
        raw["title"] = serde_json::Value::String(title.clone());

        // seeds
        let mut seeds_regenerated = 0usize;
        match opts.seed_strategy {
            SeedStrategy::Keep => {}
            SeedStrategy::Fixed(s) => {
                if let Some(gp) = raw.get_mut("globalParams") {
                    gp["seed"] = serde_json::Value::Number(s.into());
                    seeds_regenerated += 1;
                }
                if let Some(panels) = raw.get_mut("panels").and_then(|p| p.as_array_mut()) {
                    for panel in panels.iter_mut() {
                        set_panel_seed(panel, s);
                        seeds_regenerated += 1;
                    }
                }
            }
            SeedStrategy::RandomNonRepeating => {
                let mut used = std::collections::HashSet::new();
                let mut next_seed = |rng: &mut storyboard_matcher_free::SplitMix64| loop {
                    let s = rng.next_u64() % 4_000_000_000;
                    if used.insert(s) {
                        break s;
                    }
                };
                if let Some(gp) = raw.get_mut("globalParams") {
                    gp["seed"] = serde_json::Value::Number((next_seed(&mut rng) as u64).into());
                    seeds_regenerated += 1;
                }
                if let Some(panels) = raw.get_mut("panels").and_then(|p| p.as_array_mut()) {
                    for panel in panels.iter_mut() {
                        let s = next_seed(&mut rng);
                        set_panel_seed(panel, s);
                        seeds_regenerated += 1;
                    }
                }
            }
        }

        // fresh panel ids + consistent titles
        let mut new_panel_ids = 0usize;
        if let Some(panels) = raw.get_mut("panels").and_then(|p| p.as_array_mut()) {
            for panel in panels.iter_mut() {
                panel["id"] = serde_json::Value::String(uuid_from_rng(&mut rng).to_string());
                panel["title"] = serde_json::Value::String(title.clone());
                new_panel_ids += 1;
            }
        }

        let project_id = ProjectId(project_uuid);
        Ok(ClonedProject {
            summary: CloneSummary {
                project_id: project_id.to_string(),
                new_panel_ids,
                seeds_regenerated,
                title,
                panel_count: raw.get("panels").and_then(|p| p.as_array()).map(|a| a.len() as u32).unwrap_or(0),
            },
            project_id,
            raw,
            source: SourceTemplateRef {
                template_id: snapshot.id.clone(),
                revision_id: snapshot.revision_id.clone(),
                sha256: snapshot.sha256.clone(),
            },
        })
    }
}

fn set_panel_seed(panel: &mut serde_json::Value, seed: u64) {
    if let Some(po) = panel.get_mut("paramsOverride") {
        if let Some(params) = po.get_mut("params") {
            params["seed"] = serde_json::Value::Number(seed.into());
        }
    }
}

/// Verify the clone guarantees (plan §11 + Golden Case A). Returns every
/// violation found; empty = all guarantees hold.
pub fn verify_clone_guarantees(template: &serde_json::Value, cloned: &serde_json::Value) -> Vec<String> {
    let mut issues = Vec::new();
    let t_panels = template.get("panels").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    let c_panels = cloned.get("panels").and_then(|p| p.as_array()).cloned().unwrap_or_default();
    if t_panels.len() != c_panels.len() {
        issues.push(format!("panel count {} != {}", t_panels.len(), c_panels.len()));
    }
    for (i, (t, c)) in t_panels.iter().zip(c_panels.iter()).enumerate() {
        if t.get("prompt") != c.get("prompt") {
            issues.push(format!("panel {} prompt text altered", i + 1));
        }
        if t.get("index") != c.get("index") {
            issues.push(format!("panel {} index changed", i + 1));
        }
        if t.get("imageSize") != c.get("imageSize") {
            issues.push(format!("panel {} imageSize changed", i + 1));
        }
        if t.get("customCharacters") != c.get("customCharacters") {
            issues.push(format!("panel {} customCharacters altered (coords/structure must inherit)", i + 1));
        }
        if t.get("id") == c.get("id") {
            issues.push(format!("panel {} id not regenerated", i + 1));
        }
    }
    if template.get("globalNegativePrompt") != cloned.get("globalNegativePrompt") {
        issues.push("globalNegativePrompt must inherit verbatim".into());
    }
    if template.get("schemaVersion") != cloned.get("schemaVersion") {
        issues.push("schemaVersion changed".into());
    }
    if cloned.get("id") == template.get("id") {
        issues.push("project id not regenerated".into());
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use storyboard_domain::{RevisionId, TemplateId};

    fn sample_template() -> TemplateSnapshot {
        let raw = serde_json::json!({
            "schemaVersion": 2,
            "id": "11111111-1111-1111-1111-111111111111",
            "title": "template title",
            "globalStylePrompt": "",
            "globalNegativePrompt": "neg, prompt",
            "sizeMode": "uniform",
            "initialGenerationCount": 1,
            "globalParams": {
                "model":"m","stylePrompt":"","positivePrompt":"","negativePrompt":"",
                "width":832,"height":1216,"steps":28,"cfgScale":6,"cfgRescale":0.5,
                "sampler":"k_euler_ancestral","noiseSchedule":"karras","seed":123,"seedMode":"fixed",
                "ucPreset":3,"qualityPreset":"none","qualityToggle":false,
                "transparentBackground":false,"smea":false,"smeaDyn":false,"variety":false,
                "fileNamePrefix":""
            },
            "preciseReferences": [], "characters": [],
            "panels": [
                {"id":"aaaaaaaa-0000-0000-0000-000000000001","index":1,"title":"template title",
                 "prompt":"2.6:: masterpiece ::, park, night",
                 "preciseReferences":[],"charactersMode":"custom","characterRefs":[],
                 "customCharacters":[{"prompt":", official style, test girl","negativePrompt":"","useCoords":false,"x":0.5,"y":0.5}],
                 "paramsOverride":{"enabled":true,"params":{
                    "stylePrompt":"","steps":28,"cfgScale":6,"cfgRescale":0.5,"seed":111,
                    "sampler":"k_euler_ancestral","noiseSchedule":"karras","smea":false,
                    "smeaDyn":false,"model":"m","ucPreset":3,"qualityPreset":"none",
                    "variety":false,"seedMode":"fixed"}},
                 "status":"ready","candidates":[],"imageSize":{"width":832,"height":1216}},
                {"id":"aaaaaaaa-0000-0000-0000-000000000002","index":2,"title":"template title",
                 "prompt":"pov, dutch angle, park",
                 "preciseReferences":[],"charactersMode":"custom","characterRefs":[],
                 "customCharacters":[{"prompt":", official style, test girl","negativePrompt":"","useCoords":false,"x":0.5,"y":0.5}],
                 "paramsOverride":{"enabled":true,"params":{
                    "stylePrompt":"","steps":28,"cfgScale":6,"cfgRescale":0.5,"seed":222,
                    "sampler":"k_euler_ancestral","noiseSchedule":"karras","smea":false,
                    "smeaDyn":false,"model":"m","ucPreset":3,"qualityPreset":"none",
                    "variety":false,"seedMode":"fixed"}},
                 "status":"ready","candidates":[],"imageSize":{"width":1216,"height":832}}
            ]
        });
        TemplateSnapshot {
            id: TemplateId::new("T001"),
            revision_id: RevisionId::new("rev_deadbeef"),
            sha256: "deadbeef".into(),
            raw,
        }
    }

    #[test]
    fn clone_preserves_everything_but_ids_title_seed() {
        let t = sample_template();
        let c = CloneEngine::clone_template(&t, &CloneOptions::default()).unwrap();
        assert!(verify_clone_guarantees(&t.raw, &c.raw).is_empty());
        assert_eq!(c.raw["title"], "template title");
        assert_ne!(c.raw["panels"][0]["id"], t.raw["panels"][0]["id"]);
        // seeds actually re-rolled and unique
        let s1 = c.raw["panels"][0]["paramsOverride"]["params"]["seed"].as_u64().unwrap();
        let s2 = c.raw["panels"][1]["paramsOverride"]["params"]["seed"].as_u64().unwrap();
        assert_ne!(s1, s2);
    }

    #[test]
    fn clone_is_deterministic_given_seed() {
        let t = sample_template();
        let opts = CloneOptions { title: Some("新套名".into()), rng_seed: 99, ..Default::default() };
        let a = CloneEngine::clone_template(&t, &opts).unwrap();
        let b = CloneEngine::clone_template(&t, &opts).unwrap();
        assert_eq!(a.raw, b.raw);
        assert_eq!(a.raw["title"], "新套名");
        assert_eq!(a.raw["panels"][0]["title"], "新套名");
    }

    #[test]
    fn original_template_file_untouched() {
        let t = sample_template();
        let before = t.raw.clone();
        let _ = CloneEngine::clone_template(&t, &CloneOptions::default()).unwrap();
        assert_eq!(before, t.raw);
    }
}
