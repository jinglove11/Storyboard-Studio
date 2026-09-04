use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use storyboard_domain::{content_hash, schema, RevisionId, TemplateId, TemplateSnapshot};

/// Result of the full-panel character scan (P0-03). These numbers replace the
/// legacy `character_count` trio everywhere in matching and UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterScan {
    /// Distinct female identity anchors (base names, e.g. `azki`).
    pub female_anchors: Vec<String>,
    /// Full anchor strings found in CC prompts, e.g. `azki (4th costume) (hololive)`.
    pub anchor_variants: Vec<String>,
    /// Panels containing at least one male-marked slot.
    pub male_slot_panels: u32,
    pub male_lead_count: Option<u32>,
    /// Max customCharacters length over all panels.
    pub max_simultaneous_slots: u32,
    /// total_role_count = distinct female anchors + male leads.
    pub total_role_count: u32,
    /// Panels whose slots could not be classified.
    pub unclassified_slot_panels: u32,
}

fn strip_parentheticals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the female anchor token following `official style,` in a CC prompt.
pub fn extract_anchor(cc_prompt: &str) -> Option<(String, String)> {
    let lower = cc_prompt.to_lowercase();
    let pos = lower.find("official style")?;
    let rest = &cc_prompt[pos + "official style".len()..];
    let rest = rest.strip_prefix(',').unwrap_or(rest);
    // anchor = text up to the next top-level comma
    let end = rest.find(',').unwrap_or(rest.len());
    let variant = rest[..end].trim().to_string();
    if variant.is_empty() {
        return None;
    }
    let base = strip_parentheticals(&variant);
    if base.is_empty() {
        return None;
    }
    Some((base, variant))
}

fn is_male_slot(cc_prompt: &str) -> bool {
    // Male blocks look like `boy, ...` / `man, ...` / `1boy, ...`.
    // Female blocks contain `official style` and are checked first by callers.
    for tok in cc_prompt.split(|c: char| c == ',' || c == ':' || c.is_whitespace()) {
        let t = tok.trim().to_lowercase();
        let t = t.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
        match t {
            "boy" | "boys" | "man" | "men" | "male" | "males" | "faceless" => return true,
            _ => {}
        }
    }
    false
}

/// Full-panel scan over the raw template JSON.
pub fn scan_characters(raw: &serde_json::Value) -> CharacterScan {
    let mut female_anchors: Vec<String> = Vec::new();
    let mut anchor_variants: Vec<String> = Vec::new();
    let mut male_slot_panels = 0u32;
    let mut unclassified = 0u32;
    let mut max_slots = 0u32;
    let panels = raw
        .get("panels")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    for panel in &panels {
        let ccs = panel
            .get("customCharacters")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        max_slots = max_slots.max(ccs.len() as u32);
        let mut panel_has_male = false;
        let mut panel_has_female = false;
        let mut panel_unclassified = false;
        for cc in &ccs {
            let prompt = cc.get("prompt").and_then(|p| p.as_str()).unwrap_or("");
            if prompt.to_lowercase().contains("official style") {
                panel_has_female = true;
                if let Some((base, variant)) = extract_anchor(prompt) {
                    if !female_anchors.contains(&base) {
                        female_anchors.push(base.clone());
                    }
                    if !anchor_variants.contains(&variant) {
                        anchor_variants.push(variant);
                    }
                }
            } else if is_male_slot(prompt) {
                panel_has_male = true;
            } else if !prompt.trim().is_empty() {
                panel_unclassified = true;
            }
        }
        if panel_has_male {
            male_slot_panels += 1;
        }
        if panel_unclassified {
            unclassified += 1;
        }
        let _ = panel_has_female;
    }
    female_anchors.sort();
    anchor_variants.sort();
    let male_leads = if male_slot_panels > 0 { Some(1u32) } else { None };
    let total = female_anchors.len() as u32 + male_leads.unwrap_or(0);
    CharacterScan {
        female_anchors,
        anchor_variants,
        male_slot_panels,
        male_lead_count: male_leads,
        max_simultaneous_slots: max_slots,
        total_role_count: total,
        unclassified_slot_panels: unclassified,
    }
}

/// A fully scanned template ready for storage import.
#[derive(Debug, Clone)]
pub struct ScannedTemplate {
    pub snapshot: TemplateSnapshot,
    pub character_scan: CharacterScan,
    pub schema_issues: Vec<schema::SchemaIssue>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("JSON parse failed in {0}: {1}")]
    Parse(String, #[source] serde_json::Error),
    #[error("schema violations in {0}: {1}")]
    Schema(String, String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse + schema-validate + hash a template file. `strict` rejects files
/// with schema violations (import-time behaviour); non-strict keeps issues
/// for diagnostics.
pub fn scan_template(
    template_id: &str,
    source_name: &str,
    bytes: &[u8],
    strict: bool,
) -> Result<ScannedTemplate, ScanError> {
    let raw: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| ScanError::Parse(source_name.into(), e))?;
    let issues = schema::validate_storyboard_json(&raw);
    if strict && !issues.is_empty() {
        let msg = issues.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; ");
        return Err(ScanError::Schema(source_name.into(), msg));
    }
    let sha256 = content_hash(bytes);
    let revision_id = RevisionId::new(format!("rev_{}", &sha256[..16]));
    let snapshot = TemplateSnapshot {
        id: TemplateId::new(template_id),
        revision_id,
        sha256,
        raw,
    };
    let character_scan = scan_characters(&snapshot.raw);
    Ok(ScannedTemplate { snapshot, character_scan, schema_issues: issues })
}

/// Aspect ratio profile ordered by frequency (metadata field).
pub fn aspect_profile_from_counts(counts: &BTreeMap<String, u32>) -> Vec<String> {
    let mut v: Vec<(String, u32)> = counts.iter().map(|(k, c)| (k.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().map(|(k, _)| k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_extraction() {
        let cc = ", official style, azki (4th costume) (hololive),, 4::completely nude:: , long hair";
        let (base, variant) = extract_anchor(cc).unwrap();
        assert_eq!(base, "azki");
        assert_eq!(variant, "azki (4th costume) (hololive)");
    }

    #[test]
    fn male_slot_detection() {
        assert!(is_male_slot("boy, 3::standing in front of girl, hug::"));
        assert!(is_male_slot("1boy, man, faceless"));
        assert!(!is_male_slot("girl, long hair, blue eyes"));
    }

    #[test]
    fn scan_counts_roles() {
        let raw = serde_json::json!({
            "panels": [
                {"customCharacters": [
                    {"prompt": ", official style, nakano miku (school uniform), blue eyes"},
                    {"prompt": "boy, standing"}
                ]},
                {"customCharacters": [
                    {"prompt": ", official style, nakano miku (school uniform), blue eyes"}
                ]}
            ]
        });
        let s = scan_characters(&raw);
        assert_eq!(s.female_anchors, vec!["nakano miku"]);
        assert_eq!(s.total_role_count, 2);
        assert_eq!(s.max_simultaneous_slots, 2);
        assert_eq!(s.male_slot_panels, 1);
    }
}
