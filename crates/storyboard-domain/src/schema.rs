use serde::{Deserialize, Serialize};

/// Expected JSON key layout shared by all 30 author templates (verified by a
/// full scan of the frozen fixture library — every file, every panel).
pub const EXPECTED_TOP_KEYS: &[&str] = &[
    "schemaVersion",
    "id",
    "title",
    "globalStylePrompt",
    "globalNegativePrompt",
    "sizeMode",
    "initialGenerationCount",
    "globalParams",
    "preciseReferences",
    "characters",
    "panels",
];

pub const EXPECTED_GLOBAL_PARAM_KEYS: &[&str] = &[
    "model",
    "stylePrompt",
    "positivePrompt",
    "negativePrompt",
    "width",
    "height",
    "steps",
    "cfgScale",
    "cfgRescale",
    "sampler",
    "noiseSchedule",
    "seed",
    "seedMode",
    "ucPreset",
    "qualityPreset",
    "qualityToggle",
    "transparentBackground",
    "smea",
    "smeaDyn",
    "variety",
    "fileNamePrefix",
];

pub const EXPECTED_PANEL_KEYS: &[&str] = &[
    "id",
    "index",
    "title",
    "prompt",
    "preciseReferences",
    "charactersMode",
    "characterRefs",
    "customCharacters",
    "paramsOverride",
    "status",
    "candidates",
    "imageSize",
];

pub const EXPECTED_CC_KEYS: &[&str] = &["prompt", "negativePrompt", "useCoords", "x", "y"];

pub const EXPECTED_PARAMS_OVERRIDE_KEYS: &[&str] = &["enabled", "params"];

pub const EXPECTED_OVERRIDE_PARAM_KEYS: &[&str] = &[
    "stylePrompt",
    "steps",
    "cfgScale",
    "cfgRescale",
    "seed",
    "sampler",
    "noiseSchedule",
    "smea",
    "smeaDyn",
    "model",
    "ucPreset",
    "qualityPreset",
    "variety",
    "seedMode",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaFingerprint(pub String);

impl SchemaFingerprint {
    /// Fingerprint of the *key layout*: for the fixed schema all valid
    /// templates share one fingerprint. If a future template revision
    /// introduces new fields the fingerprint changes and the Schema Gate
    /// blocks the export until the layout is explicitly accepted.
    pub fn canonical() -> Self {
        Self(content_layout_hash())
    }
}

fn content_layout_hash() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for set in [
        EXPECTED_TOP_KEYS,
        EXPECTED_GLOBAL_PARAM_KEYS,
        EXPECTED_PANEL_KEYS,
        EXPECTED_CC_KEYS,
        EXPECTED_PARAMS_OVERRIDE_KEYS,
        EXPECTED_OVERRIDE_PARAM_KEYS,
    ] {
        for k in set {
            h.update(k.as_bytes());
            h.update([0u8]);
        }
        h.update([1u8]);
    }
    hex::encode(&h.finalize()[..16])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaIssue {
    NotAnObject { path: String },
    MissingKey { path: String, key: String },
    UnexpectedKey { path: String, key: String },
    WrongType { path: String, expected: String },
    BadSchemaVersion { found: serde_json::Value },
    BadIndexSequence { panel: u32, expected: u32 },
    InconsistentPanelTitle { panel: u32, project_title: String, panel_title: String },
    BadStatus { panel: u32, found: String },
}

impl std::fmt::Display for SchemaIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaIssue::NotAnObject { path } => write!(f, "{path}: expected JSON object"),
            SchemaIssue::MissingKey { path, key } => write!(f, "{path}: missing key `{key}`"),
            SchemaIssue::UnexpectedKey { path, key } => {
                write!(f, "{path}: unexpected key `{key}` (schema forbids new fields)")
            }
            SchemaIssue::WrongType { path, expected } => write!(f, "{path}: expected {expected}"),
            SchemaIssue::BadSchemaVersion { found } => write!(f, "schemaVersion must be 2, found {found}"),
            SchemaIssue::BadIndexSequence { panel, expected } => {
                write!(f, "panel index {panel} out of order (expected {expected})")
            }
            SchemaIssue::InconsistentPanelTitle { panel, project_title, panel_title } => write!(
                f,
                "panel {panel} title `{panel_title}` differs from project title `{project_title}`"
            ),
            SchemaIssue::BadStatus { panel, found } => {
                write!(f, "panel {panel} status must be `ready`, found `{found}`")
            }
        }
    }
}

fn check_keys(
    obj: &serde_json::Map<String, serde_json::Value>,
    expected: &[&str],
    path: &str,
    issues: &mut Vec<SchemaIssue>,
) {
    for k in expected {
        if !obj.contains_key(*k) {
            issues.push(SchemaIssue::MissingKey { path: path.into(), key: (*k).into() });
        }
    }
    for k in obj.keys() {
        if !expected.contains(&k.as_str()) {
            issues.push(SchemaIssue::UnexpectedKey { path: path.into(), key: k.clone() });
        }
    }
}

/// Validate the full storyboard JSON against the frozen schemaVersion-2
/// layout. Returns every violation; empty vec == PASS.
pub fn validate_storyboard_json(v: &serde_json::Value) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    let Some(top) = v.as_object() else {
        return vec![SchemaIssue::NotAnObject { path: "$".into() }];
    };
    check_keys(top, EXPECTED_TOP_KEYS, "$", &mut issues);
    match top.get("schemaVersion") {
        Some(serde_json::Value::Number(n)) if n.as_i64() == Some(2) => {}
        Some(other) => issues.push(SchemaIssue::BadSchemaVersion { found: other.clone() }),
        None => {}
    }
    if let Some(gp) = top.get("globalParams").and_then(|g| g.as_object()) {
        check_keys(gp, EXPECTED_GLOBAL_PARAM_KEYS, "$.globalParams", &mut issues);
    }
    let project_title = top.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let Some(panels) = top.get("panels").and_then(|p| p.as_array()) else {
        if !issues.is_empty() || top.get("panels").is_some() {
            issues.push(SchemaIssue::WrongType {
                path: "$.panels".into(),
                expected: "array".into(),
            });
        }
        return issues;
    };
    for (i, panel) in panels.iter().enumerate() {
        let idx = (i + 1) as u32;
        let path = format!("$.panels[{i}]");
        let Some(po) = panel.as_object() else {
            issues.push(SchemaIssue::NotAnObject { path });
            continue;
        };
        check_keys(po, EXPECTED_PANEL_KEYS, &path, &mut issues);
        match po.get("index").and_then(|x| x.as_u64()) {
            Some(n) if n as u32 == idx => {}
            Some(n) => issues.push(SchemaIssue::BadIndexSequence { panel: n as u32, expected: idx }),
            None => issues.push(SchemaIssue::WrongType { path: format!("{path}.index"), expected: "u32".into() }),
        }
        if let Some(t) = po.get("title").and_then(|t| t.as_str()) {
            if !project_title.is_empty() && t != project_title {
                issues.push(SchemaIssue::InconsistentPanelTitle {
                    panel: idx,
                    project_title: project_title.clone(),
                    panel_title: t.into(),
                });
            }
        }
        if let Some(s) = po.get("status").and_then(|s| s.as_str()) {
            if s != "ready" {
                issues.push(SchemaIssue::BadStatus { panel: idx, found: s.into() });
            }
        }
        if let Some(ccs) = po.get("customCharacters").and_then(|c| c.as_array()) {
            for (j, cc) in ccs.iter().enumerate() {
                let cc_path = format!("{path}.customCharacters[{j}]");
                if let Some(cco) = cc.as_object() {
                    check_keys(cco, EXPECTED_CC_KEYS, &cc_path, &mut issues);
                } else {
                    issues.push(SchemaIssue::NotAnObject { path: cc_path });
                }
            }
        }
        if let Some(po_override) = po.get("paramsOverride") {
            let po_path = format!("{path}.paramsOverride");
            if let Some(poo) = po_override.as_object() {
                check_keys(poo, EXPECTED_PARAMS_OVERRIDE_KEYS, &po_path, &mut issues);
                if let Some(params) = poo.get("params").and_then(|p| p.as_object()) {
                    check_keys(params, EXPECTED_OVERRIDE_PARAM_KEYS, &format!("{po_path}.params"), &mut issues);
                }
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 2,
            "id": "u", "title": "t", "globalStylePrompt": "", "globalNegativePrompt": "",
            "sizeMode": "uniform", "initialGenerationCount": 1,
            "globalParams": {
                "model":"m","stylePrompt":"","positivePrompt":"","negativePrompt":"",
                "width":832,"height":1216,"steps":28,"cfgScale":6,"cfgRescale":0.5,
                "sampler":"k_euler_ancestral","noiseSchedule":"karras","seed":0,"seedMode":"fixed",
                "ucPreset":3,"qualityPreset":"none","qualityToggle":false,
                "transparentBackground":false,"smea":false,"smeaDyn":false,"variety":false,
                "fileNamePrefix":""
            },
            "preciseReferences": [], "characters": [],
            "panels": [{
                "id":"p1","index":1,"title":"t","prompt":"x","preciseReferences":[],
                "charactersMode":"custom","characterRefs":[],"customCharacters":[],
                "paramsOverride":{"enabled":true,"params":{
                    "stylePrompt":"","steps":28,"cfgScale":6,"cfgRescale":0.5,"seed":1,
                    "sampler":"k_euler_ancestral","noiseSchedule":"karras","smea":false,
                    "smeaDyn":false,"model":"m","ucPreset":3,"qualityPreset":"none",
                    "variety":false,"seedMode":"fixed"}},
                "status":"ready","candidates":[],"imageSize":{"width":832,"height":1216}
            }]
        })
    }

    #[test]
    fn canonical_sample_passes() {
        assert!(validate_storyboard_json(&sample()).is_empty());
    }

    #[test]
    fn detects_new_field_and_index_gap() {
        let mut v = sample();
        v["panels"][0]["prompt_extra"] = serde_json::json!("illegal");
        v["panels"][0]["index"] = serde_json::json!(7);
        let issues = validate_storyboard_json(&v);
        assert!(issues.iter().any(|i| matches!(i, SchemaIssue::UnexpectedKey { key, .. } if key == "prompt_extra")));
        assert!(issues.iter().any(|i| matches!(i, SchemaIssue::BadIndexSequence { .. })));
    }
}
