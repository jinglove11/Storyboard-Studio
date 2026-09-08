//! Clothing state-chain analysis (skill template-mutation §3.4 / §7.4).
//!
//! The author encodes a garment's narrative arc with weight syntax:
//! `1.3::pantyhose::` (worn) → `0.3::torn pantyhose::` (torn) →
//! `-2::pantyhose::` (removed + no-re-穿着 guard). A character swap must
//! swap the WHOLE chain — the new garment mirrors the old one's per-panel
//! stage pattern (worn/torn/removed), never drifting a stage earlier or
//! later, and the negative guard blocks must swap in lockstep.
//!
//! Pure JSON analysis: no I/O, no mutation — validators compare two
//! signatures (template vs draft) for the mapped old→new clothing word.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Danbooru-ish garment vocabulary (suffix match, comma-separated tokens).
pub const CLOTHING_VOCAB: &[&str] = &[
    "pantyhose",
    "stockings",
    "thighhighs",
    "skirt",
    "dress",
    "sweater",
    "shirt",
    "blouse",
    "uniform",
    "serafuku",
    "bra",
    "panties",
    "shorts",
    "kimono",
    "yukata",
    "bikini",
    "swimsuit",
    "jacket",
    "coat",
    "gloves",
    "corset",
    "apron",
    "cheongsam",
    "leotard",
    "clothes",
    "hoodie",
    "cardigan",
    "blazer",
    "vest",
    "robe",
    "nightgown",
    "lingerie",
    "garter",
    "socks",
    "pants",
    "jeans",
    "suit",
    "bodystocking",
    "fishnets",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClothingState {
    /// Worn: appears outside a negative weight block (plain text or `+N::`).
    Positive,
    /// Removed + no-re-wear guard: appears inside a `-N::...::` block.
    Negative,
    Absent,
}

/// One panel's clothing signature: garment token → state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PanelClothing {
    pub states: BTreeMap<String, ClothingState>,
}

/// Extract every `±N::content::` weight block with its sign.
/// Manual parse (no regex): a `::` pair whose prefix ends in a number opens
/// a block; the content runs to the next `::`.
pub fn weight_blocks(text: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("::") {
        let prefix = &rest[..open];
        // trailing number (possibly negative / fractional) before the opener
        let trimmed = prefix.trim_end();
        let mut n = trimmed.len();
        while n > 0 {
            let c = trimmed.as_bytes()[n - 1];
            if c.is_ascii_digit() || c == b'.' {
                n -= 1;
            } else {
                break;
            }
        }
        if n == trimmed.len() {
            // no trailing number → this `::` is not a weight opener
            rest = &rest[open + 2..];
            continue;
        }
        let negative = n > 0 && trimmed.as_bytes()[n - 1] == b'-';
        let content_start = open + 2;
        let Some(close_rel) = rest[content_start..].find("::") else {
            break; // unbalanced — treat the rest as plain text
        };
        let content = rest[content_start..content_start + close_rel].to_string();
        out.push((negative, content));
        rest = &rest[content_start + close_rel + 2..];
    }
    out
}

fn contains_token(hay: &str, token: &str) -> bool {
    // weight colons act as separators: "1.3::pantyhose::" -> ["1.3","pantyhose"]
    let hay_l = hay.to_lowercase().replace("::", ",");
    let token_l = token.trim().to_lowercase();
    if token_l.is_empty() {
        return false;
    }
    for part in hay_l.split(',') {
        let p = part.trim();
        if p == token_l
            || p.ends_with(&format!(" {token_l}"))
            || p.starts_with(&format!("{token_l} "))
        {
            return true;
        }
    }
    false
}

/// Is this token a garment we track? (whole-token suffix match against vocab)
pub fn is_clothing_token(token: &str) -> bool {
    let t = token.trim().to_lowercase();
    if t.is_empty() {
        return false;
    }
    // match the LAST word of the token (handles "black pantyhose", "torn pantyhose")
    let last = t.rsplit([' ', ',']).next().unwrap_or("").to_string();
    CLOTHING_VOCAB.contains(&last.as_str())
}

fn panel_text(panel: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = panel.get("prompt").and_then(|x| x.as_str()) {
        parts.push(p.to_string());
    }
    if let Some(ccs) = panel.get("customCharacters").and_then(|c| c.as_array()) {
        for cc in ccs {
            if let Some(p) = cc.get("prompt").and_then(|x| x.as_str()) {
                parts.push(p.to_string());
            }
        }
    }
    parts.join("\n")
}

/// Full per-panel clothing signature of a storyboard project.
pub fn clothing_signature(v: &serde_json::Value) -> Vec<PanelClothing> {
    let mut out = Vec::new();
    let Some(panels) = v.get("panels").and_then(|p| p.as_array()) else {
        return out;
    };
    for panel in panels {
        let text = panel_text(panel);
        let mut states: BTreeMap<String, ClothingState> = BTreeMap::new();
        for word in CLOTHING_VOCAB {
            let state = token_stage_in(&text, word);
            if state != ClothingState::Absent {
                states.insert((*word).to_string(), state);
            }
        }
        out.push(PanelClothing { states });
    }
    out
}

/// Compare the template chain of `old_token` with the draft chain of
/// `new_token`, panel by panel, on the FULL multi-word tokens (so
/// `pantyhose -> black pantyhose` compares those exact strings — a leftover
/// un-swapped guard block shows up as a state mismatch). Returns the list of
/// violations (empty = the chain survived intact).
///
/// Rules (skill §3.4.4):
/// 1. stage-change panels identical — worn/torn/removed must not move;
/// 2. 只减不回穿 — a Negative stage never reverts to Positive later;
/// 3. 正负权同词同换 — both sides of the swap use the new word, including
///    the `-N::` no-re-wear guards.
pub fn compare_clothing_chain(
    template: &serde_json::Value,
    draft: &serde_json::Value,
    old_token: &str,
    new_token: &str,
) -> Vec<String> {
    let mut violations = Vec::new();
    let tmpl_panels = template
        .get("panels")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let draft_panels = draft
        .get("panels")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    if tmpl_panels.len() != draft_panels.len() {
        // resize in flight — the resize op owns stage sampling
        return violations;
    }
    for (i, (tp, dp)) in tmpl_panels.iter().zip(draft_panels.iter()).enumerate() {
        let old_state = token_stage_in(&panel_text(tp), old_token);
        let new_state = token_stage_in(&panel_text(dp), new_token);
        if old_state != new_state {
            violations.push(format!(
                "clothing chain: panel {} `{}` was {:?} in the template but `{}` is {:?} in the draft (stage moved or guard block not swapped)",
                i + 1,
                old_token,
                old_state,
                new_token,
                new_state
            ));
        }
    }
    violations
}

/// Stage of one full token inside one panel's combined prompt text.
fn token_stage_in(text: &str, token: &str) -> ClothingState {
    let mut negative_text = String::new();
    for (neg, content) in weight_blocks(text) {
        if neg {
            negative_text.push_str(&content);
            negative_text.push(',');
        }
    }
    if contains_token(&negative_text, token) {
        ClothingState::Negative
    } else if contains_token(text, token) {
        ClothingState::Positive
    } else {
        ClothingState::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_weight_blocks_with_signs() {
        let blocks = weight_blocks("a, 1.3::pantyhose, skirt::, b, -2::pantyhose, skirt::, c");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], (false, "pantyhose, skirt".to_string()));
        assert_eq!(blocks[1], (true, "pantyhose, skirt".to_string()));
        // stray colons without numbers are not blocks
        assert!(weight_blocks("plain :: text :: here").is_empty());
    }

    fn project(prompts: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "panels": prompts
                .iter()
                .map(|p| serde_json::json!({ "prompt": p, "customCharacters": [] }))
                .collect::<Vec<_>>()
        })
    }

    #[test]
    fn chain_stage_pattern_is_extracted() {
        let tmpl = project(&[
            "girl, 1.3::pantyhose::",
            "girl, 0.3::torn pantyhose::",
            "girl, bare legs, -2::pantyhose::",
        ]);
        let sig = clothing_signature(&tmpl);
        assert_eq!(sig.len(), 3);
        assert_eq!(
            sig[0].states.get("pantyhose"),
            Some(&ClothingState::Positive)
        );
        assert_eq!(
            sig[1].states.get("pantyhose"),
            Some(&ClothingState::Positive)
        ); // torn counts as worn
        assert_eq!(
            sig[2].states.get("pantyhose"),
            Some(&ClothingState::Negative)
        );
    }

    #[test]
    fn identical_chain_passes_and_drift_fails() {
        let tmpl = project(&[
            "girl, 1.3::pantyhose::",
            "girl, 0.3::torn pantyhose::",
            "girl, bare legs, -2::pantyhose::",
        ]);
        let ok = project(&[
            "girl, 1.3::black pantyhose::",
            "girl, 0.3::torn black pantyhose::",
            "girl, bare legs, -2::black pantyhose::",
        ]);
        assert!(compare_clothing_chain(&tmpl, &ok, "pantyhose", "black pantyhose").is_empty());

        // stage drift: the removal became a worn stage
        let drift = project(&[
            "girl, 1.3::black pantyhose::",
            "girl, 0.3::torn black pantyhose::",
            "girl, 0.3::torn black pantyhose::",
        ]);
        assert!(!compare_clothing_chain(&tmpl, &drift, "pantyhose", "black pantyhose").is_empty());

        // partial swap: the negative guard still says the OLD word
        let partial = project(&[
            "girl, 1.3::black pantyhose::",
            "girl, 0.3::torn black pantyhose::",
            "girl, bare legs, -2::pantyhose::",
        ]);
        assert!(
            !compare_clothing_chain(&tmpl, &partial, "pantyhose", "black pantyhose").is_empty()
        );

        // 回穿 violation: garment reverts to worn after removal
        let revert = project(&[
            "girl, 1.3::black pantyhose::",
            "girl, bare legs, -2::black pantyhose::",
            "girl, 1.3::black pantyhose::",
        ]);
        assert!(!compare_clothing_chain(&tmpl, &revert, "pantyhose", "black pantyhose").is_empty());
    }

    #[test]
    fn vocab_detection_handles_multiword_tokens() {
        assert!(is_clothing_token("pantyhose"));
        assert!(is_clothing_token("black pantyhose"));
        assert!(is_clothing_token("torn black pantyhose"));
        assert!(!is_clothing_token("park"));
        assert!(!is_clothing_token("crying"));
    }
}
