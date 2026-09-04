use crate::scan::{aspect_profile_from_counts, CharacterScan, ScannedTemplate};
use crate::skill::IndexEntry;
use serde::{Deserialize, Serialize};
use storyboard_domain::{LegacyStats, SchemaFingerprint, TemplateMetadata};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
}

/// Build rebuilt metadata: semantic fields come from the (human-verified)
/// legacy index, character statistics from the fresh full-panel scan.
/// Every disagreement between the two is recorded as a warning + legacy
/// mismatch (P0-04 / Golden Case D).
pub fn build_metadata(scanned: &ScannedTemplate, legacy: Option<&IndexEntry>) -> TemplateMetadata {
    let snapshot = &scanned.snapshot;
    let cs: &CharacterScan = &scanned.character_scan;
    let mut warnings = Vec::new();
    let mut mismatches = Vec::new();

    let mut scene_family = String::new();
    let mut exact_scene = None;
    let mut scene_tags = Vec::new();
    let mut location_tags = Vec::new();
    let mut time_tags = Vec::new();
    let mut environment_tags = Vec::new();
    let mut narrative_type = None;
    let mut opening_type = None;
    let mut ending_type = None;
    let mut pace = "standard".to_string();
    let mut first_sex_panel = None;
    let mut pov_ratio = None;
    let mut torogao_coverage = None;
    let mut camera_profile = Vec::new();
    let mut camera_profile_freq = Default::default();
    let mut composition_profile = Vec::new();
    let mut clothing_arc = None;
    let mut interaction_profile = Vec::new();
    let mut important_props = Vec::new();
    let mut keywords = Vec::new();
    let mut aspect_profile = Vec::new();
    let mut male_identity = None;
    let mut male_panel_ratio = None;
    let mut legacy_stats = None;

    // Legacy anchor names are HINTS: accept one only when the actual template
    // text contains it (verified against the original JSON). Pure scans that
    // find nothing (authors not using `official style`) get low confidence.
    let template_text = serde_json::to_string(&snapshot.raw).unwrap_or_default().to_lowercase();
    let mut female_anchors = cs.female_anchors.clone();
    let mut anchor_variants = cs.anchor_variants.clone();
    if let Some(e) = legacy {
        for a in &e.female_anchors {
            let a_l = a.to_lowercase();
            if template_text.contains(&a_l)
                && !female_anchors.iter().any(|x| x.to_lowercase().contains(&a_l))
            {
                female_anchors.push(a.clone());
                if !anchor_variants.iter().any(|x| x.to_lowercase().contains(&a_l)) {
                    anchor_variants.push(a.clone());
                }
                warnings.push(ImportWarning {
                    code: "ANCHOR_FROM_LEGACY_VERIFIED".into(),
                    message: format!("anchor `{a}` recovered from legacy index after text verification"),
                });
            }
        }
    }
    female_anchors.sort();
    anchor_variants.sort();
    let verified_female_count = female_anchors.len() as u32;
    let verified_total = verified_female_count + cs.male_lead_count.unwrap_or(0);

    if let Some(e) = legacy {
        scene_family = e.scene_family.clone();
        exact_scene = e.exact_scene.clone();
        scene_tags = e.scene.clone();
        location_tags = e.location.clone();
        time_tags = e.time.clone();
        environment_tags = e.environment.clone();
        narrative_type = e.narrative_type.clone();
        opening_type = e.opening_type.clone();
        ending_type = e.ending_type.clone();
        pace = e.pace.clone().unwrap_or_else(|| "standard".into());
        first_sex_panel = e.first_sex_panel;
        pov_ratio = e.pov_ratio;
        torogao_coverage = e.torogao_coverage;
        camera_profile = e.camera_profile.clone();
        camera_profile_freq = e.camera_profile_freq.clone();
        composition_profile = e.composition_profile.clone();
        clothing_arc = e.clothing_arc.clone();
        interaction_profile = e.interaction_profile.clone();
        important_props = e.important_props.clone();
        keywords = e.keywords.clone();
        aspect_profile = aspect_profile_from_counts(&e.aspect_ratio_counts);
        male_identity = e.male_identity.clone();
        male_panel_ratio = e.male_panel_ratio;

        // P0-03 audit: compare legacy counts against the verified scan.
        let legacy_cc = e.character_count;
        let legacy_sum = e.female_character_count + e.male_character_count;
        if legacy_cc != legacy_sum {
            let msg = format!(
                "legacy character_count={legacy_cc} != female({}) + male({}) = {legacy_sum}",
                e.female_character_count, e.male_character_count
            );
            warnings.push(ImportWarning { code: "LEGACY_COUNT_MISMATCH".into(), message: msg.clone() });
            mismatches.push(msg);
        }
        if legacy_cc != verified_total {
            let msg = format!(
                "legacy character_count={legacy_cc} != rescanned total_role_count={verified_total}"
            );
            warnings.push(ImportWarning { code: "ROLE_COUNT_MISMATCH".into(), message: msg.clone() });
            mismatches.push(msg);
        }
        let scanned_female: Vec<String> =
            female_anchors.iter().map(|a| a.to_lowercase()).collect();
        let legacy_female: Vec<String> =
            e.female_anchors.iter().map(|a| a.to_lowercase()).collect();
        if scanned_female != legacy_female {
            let msg = format!("female anchors legacy={legacy_female:?} rescanned={scanned_female:?}");
            warnings.push(ImportWarning { code: "ANCHORS_MISMATCH".into(), message: msg.clone() });
            mismatches.push(msg);
        }
        if e.panel_count != snapshot.panel_count() {
            let msg = format!(
                "legacy panel_count={} != actual={}",
                e.panel_count,
                snapshot.panel_count()
            );
            warnings.push(ImportWarning { code: "PANEL_COUNT_MISMATCH".into(), message: msg });
        }
        legacy_stats = Some(LegacyStats {
            character_count: e.character_count,
            female_character_count: e.female_character_count,
            male_character_count: e.male_character_count,
            female_anchors: e.female_anchors.clone(),
            mismatches: mismatches.clone(),
        });
    } else {
        warnings.push(ImportWarning {
            code: "NO_LEGACY_INDEX".into(),
            message: "template not present in legacy index; all index-derived fields are empty"
                .into(),
        });
    }

    if !scanned.schema_issues.is_empty() {
        warnings.push(ImportWarning {
            code: "SCHEMA_ISSUES".into(),
            message: format!("{} schema issue(s) on import", scanned.schema_issues.len()),
        });
    }
    if cs.unclassified_slot_panels > 0 {
        warnings.push(ImportWarning {
            code: "UNCLASSIFIED_SLOTS".into(),
            message: format!("{} panel(s) contain slots neither female-anchored nor male-marked", cs.unclassified_slot_panels),
        });
    }

    // confidence: start at 1.0, lose 0.15 per warning category, floor 0.2
    let mut confidence = 1.0f32;
    for w in &warnings {
        match w.code.as_str() {
            "ROLE_COUNT_MISMATCH" | "ANCHORS_MISMATCH" | "NO_LEGACY_INDEX" => confidence -= 0.15,
            "LEGACY_COUNT_MISMATCH" | "PANEL_COUNT_MISMATCH" => confidence -= 0.10,
            _ => confidence -= 0.05,
        }
    }
    let confidence = confidence.max(0.2);

    TemplateMetadata {
        template_id: snapshot.id.as_str().to_string(),
        revision_id: snapshot.revision_id.as_str().to_string(),
        title: snapshot.title().to_string(),
        source_name: legacy.map(|e| e.source_file.clone()).unwrap_or_default(),
        sha256: snapshot.sha256.clone(),
        schema_fingerprint: SchemaFingerprint::canonical().0,
        scene_family,
        exact_scene,
        scene_tags,
        location_tags,
        time_tags,
        environment_tags,
        total_role_count: verified_total,
        female_lead_count: Some(verified_female_count),
        male_lead_count: cs.male_lead_count,
        max_simultaneous_slots: cs.max_simultaneous_slots,
        character_anchors: female_anchors.clone(),
        character_anchor_variants: anchor_variants.clone(),
        male_identity,
        male_panel_ratio: male_panel_ratio.or(Some(
            (cs.male_slot_panels as f32 / snapshot.panel_count().max(1) as f32) * 100.0,
        )),
        panel_count: snapshot.panel_count(),
        narrative_type,
        opening_type,
        ending_type,
        pace,
        first_sex_panel,
        pov_ratio,
        torogao_coverage,
        camera_profile,
        camera_profile_freq,
        composition_profile,
        clothing_arc,
        interaction_profile,
        important_props,
        keywords,
        aspect_ratio_profile: aspect_profile,
        metadata_confidence: confidence,
        warnings: warnings.iter().map(|w| format!("{}: {}", w.code, w.message)).collect(),
        reviewed_at: None,
        legacy: legacy_stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_template;
    use crate::skill::IndexEntry;
    use storyboard_domain::content_hash;

    fn template_bytes() -> Vec<u8> {
        let v = serde_json::json!({
            "schemaVersion": 2, "id": "u", "title": "t",
            "globalStylePrompt": "", "globalNegativePrompt": "g",
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
            "panels": [
                {"id":"p1","index":1,"title":"t","prompt":"a","preciseReferences":[],
                 "charactersMode":"custom","characterRefs":[],
                 "customCharacters":[
                    {"prompt":", official style, azki (4th costume) (hololive), long hair","negativePrompt":"","useCoords":false,"x":0.5,"y":0.5},
                    {"prompt":"boy, standing","negativePrompt":"","useCoords":false,"x":0.5,"y":0.5}],
                 "paramsOverride":{"enabled":true,"params":{
                    "stylePrompt":"","steps":28,"cfgScale":6,"cfgRescale":0.5,"seed":1,
                    "sampler":"k_euler_ancestral","noiseSchedule":"karras","smea":false,
                    "smeaDyn":false,"model":"m","ucPreset":3,"qualityPreset":"none",
                    "variety":false,"seedMode":"fixed"}},
                 "status":"ready","candidates":[],"imageSize":{"width":832,"height":1216}},
                {"id":"p2","index":2,"title":"t","prompt":"b","preciseReferences":[],
                 "charactersMode":"custom","characterRefs":[],
                 "customCharacters":[
                    {"prompt":", official style, azki (4th costume) (hololive), long hair","negativePrompt":"","useCoords":false,"x":0.5,"y":0.5}],
                 "paramsOverride":{"enabled":true,"params":{
                    "stylePrompt":"","steps":28,"cfgScale":6,"cfgRescale":0.5,"seed":2,
                    "sampler":"k_euler_ancestral","noiseSchedule":"karras","smea":false,
                    "smeaDyn":false,"model":"m","ucPreset":3,"qualityPreset":"none",
                    "variety":false,"seedMode":"fixed"}},
                 "status":"ready","candidates":[],"imageSize":{"width":832,"height":1216}}
            ]
        });
        serde_json::to_vec_pretty(&v).unwrap()
    }

    #[test]
    fn metadata_rebuild_fixes_wrong_legacy_counts() {
        let bytes = template_bytes();
        let scanned = scan_template("T900", "test.json", &bytes, false).unwrap();
        let mut legacy: IndexEntry = serde_json::from_value(serde_json::json!({
            "template_id": "T900", "source_file": "test.json", "title": "t",
            "scene_family": "apartment",
            "character_count": 5, "female_character_count": 2, "male_character_count": 1,
            "female_anchors": ["azki"], "panel_count": 2, "pace": "standard"
        }))
        .unwrap();
        legacy.character_count = 3; // 2+1 passes internal check but != scan (2)
        let meta = build_metadata(&scanned, Some(&legacy));
        assert_eq!(meta.total_role_count, 2);
        assert_eq!(meta.character_anchors, vec!["azki"]);
        assert_eq!(meta.character_anchor_variants, vec!["azki (4th costume) (hololive)"]);
        assert!(meta.legacy.as_ref().unwrap().mismatches.iter().any(|m| m.contains("rescanned")));
        assert!(meta.metadata_confidence < 1.0);
        // audit hash recorded
        assert_eq!(meta.sha256, content_hash(&bytes));
    }
}
