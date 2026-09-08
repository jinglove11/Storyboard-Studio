use serde_json::json;
use storyboard_domain::{
    OperationKind, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal, ProjectId,
    ProjectSnapshot, RevisionId, TemplateId, TemplateMetadata, TemplateSnapshot, TextTarget,
    TokenReplacement,
};
use storyboard_patch::apply_proposal;
use storyboard_validator::{validate, ValidationContext, ValidatorConfig};

fn template_snapshot() -> TemplateSnapshot {
    let mut panels = Vec::new();
    for i in 1..=4 {
        panels.push(json!({
            "id": format!("tp-{i}"), "index": i, "title": "テンプレ",
            "prompt": format!("2.6:: masterpiece ::, park, bush, night, pov, nakano miku, {i}"),
            "preciseReferences": [], "charactersMode": "custom", "characterRefs": [],
            "customCharacters": [
                {"prompt": ", official style, nakano miku (school uniform), pink hair",
                 "negativePrompt": "", "useCoords": false, "x": 0.9, "y": 0.3}
            ],
            "paramsOverride": {"enabled": true, "params": {
                "stylePrompt":"", "steps":28, "cfgScale":6, "cfgRescale":0.5, "seed":100+i,
                "sampler":"k_euler_ancestral", "noiseSchedule":"karras", "smea":false,
                "smeaDyn":false, "model":"m", "ucPreset":3, "qualityPreset":"none",
                "variety":false, "seedMode":"fixed"}},
            "status":"ready", "candidates":[], "imageSize":{"width":832,"height":1216}
        }));
    }
    TemplateSnapshot {
        id: TemplateId::new("T010"),
        revision_id: RevisionId::new("rev_t010"),
        sha256: "t010sha".into(),
        raw: json!({
            "schemaVersion": 2, "id": "t010", "title": "テンプレ",
            "globalStylePrompt": "", "globalNegativePrompt": "neg",
            "sizeMode": "uniform", "initialGenerationCount": 1,
            "globalParams": {"model":"m","stylePrompt":"","positivePrompt":"","negativePrompt":"",
                "width":832,"height":1216,"steps":28,"cfgScale":6,"cfgRescale":0.5,
                "sampler":"k_euler_ancestral","noiseSchedule":"karras","seed":1,"seedMode":"fixed",
                "ucPreset":3,"qualityPreset":"none","qualityToggle":false,
                "transparentBackground":false,"smea":false,"smeaDyn":false,"variety":false,
                "fileNamePrefix":""},
            "preciseReferences": [], "characters": [], "panels": panels
        }),
    }
}

fn metadata() -> TemplateMetadata {
    serde_json::from_value(json!({
        "template_id": "T010", "revision_id": "rev_t010", "title": "テンプレ",
        "source_name": "", "sha256": "t010sha", "schema_fingerprint": "f",
        "scene_family": "park", "exact_scene": "night park",
        "scene_tags": ["park", "night"], "location_tags": ["park", "park bushes"],
        "time_tags": ["night"], "environment_tags": ["bush", "tree", "cardboard"],
        "total_role_count": 2, "female_lead_count": 1, "male_lead_count": 1,
        "max_simultaneous_slots": 2,
        "character_anchors": ["nakano miku"],
        "character_anchor_variants": ["nakano miku (school uniform)"],
        "male_identity": "anonymous man", "male_panel_ratio": 90.0,
        "panel_count": 4, "narrative_type": "night park rape",
        "opening_type": null, "ending_type": null, "pace": "standard",
        "first_sex_panel": 6, "pov_ratio": 56.0, "torogao_coverage": 30.0,
        "camera_profile": ["pov", "dutch angle", "close-up"], "camera_profile_freq": {},
        "composition_profile": [], "clothing_arc": null,
        "interaction_profile": ["rape"], "important_props": ["park", "bush", "cardboard"],
        "keywords": ["park", "night"], "aspect_ratio_profile": [],
        "metadata_confidence": 1.0, "warnings": [], "reviewed_at": null, "legacy": null
    }))
    .unwrap()
}

fn cloned_project(t: &TemplateSnapshot) -> ProjectSnapshot {
    ProjectSnapshot {
        project_id: ProjectId::generate(),
        version: 1,
        title: "テンプレ".into(),
        source: storyboard_domain::SourceTemplateRef {
            template_id: t.id.clone(),
            revision_id: t.revision_id.clone(),
            sha256: t.sha256.clone(),
        },
        raw: t.raw.clone(),
    }
}

fn op_common() -> PatchOperationCommon {
    PatchOperationCommon {
        operation_id: "op-test".into(),
        panel_index: None,
        panel_id: None,
        anchor: None,
        expected_old: None,
        expected_old_hash: None,
        expected_project_version: 1,
    }
}

fn identity_replacements() -> Vec<TokenReplacement> {
    vec![
        TokenReplacement {
            old_token: "nakano miku (school uniform)".into(),
            new_token: "hoshino ai (idol)".into(),
        },
        TokenReplacement {
            old_token: "nakano miku".into(),
            new_token: "hoshino ai".into(),
        },
        TokenReplacement {
            old_token: "pink hair".into(),
            new_token: "purple hair".into(),
        },
    ]
}

fn proposal(intent: PatchIntent, ops: Vec<PatchOperation>, touched: Vec<u32>) -> PatchProposal {
    PatchProposal {
        base_project_version: 1,
        primary_template_id: TemplateId::new("T010"),
        intent_hash: "h".into(),
        intent,
        operations: ops,
        touched_panels: touched,
        expected_preservation_ratio: 0.9,
        rationale: vec![],
        user_requested_resize: false,
    }
}

fn identity_op() -> PatchOperation {
    PatchOperation {
        common: PatchOperationCommon {
            operation_id: "op-identity".into(),
            panel_index: None,
            panel_id: None,
            anchor: None,
            expected_old: None,
            expected_old_hash: None,
            expected_project_version: 1,
        },
        kind: OperationKind::ReplaceCharacterIdentity {
            replacements: identity_replacements(),
            slots: None,
            appearance_replacements: Vec::new(),
        },
    }
}

fn run(p: &PatchProposal, base: &ProjectSnapshot) -> storyboard_validator::ValidationReport {
    let t = template_snapshot();
    let app = apply_proposal(base, p).unwrap();
    let ctx = ValidationContext {
        template: &t,
        template_metadata: &metadata(),
        base,
        proposal: p,
        draft: &app.draft,
        applied_touched_panels: app.touched_panels.clone(),
        current_template_sha: "t010sha",
        config: ValidatorConfig::default(),
    };
    validate(&ctx)
}

#[test]
fn clean_identity_replacement_passes_all_gates() {
    let t = template_snapshot();
    let base = cloned_project(&t);
    let p = proposal(PatchIntent::CharacterReplace, vec![identity_op()], vec![]);
    let report = run(&p, &base);
    assert!(report.passed, "gates: {report:#?}");
    assert!(report.preservation_ratio >= 0.90);
}

#[test]
fn agent_overreach_fails_anti_rewrite() {
    // Golden Case E: agent rewrites prompts wholesale via PatchPromptBlock on
    // undeclared/many panels with fresh text.
    let t = template_snapshot();
    let base = cloned_project(&t);
    let mut ops = Vec::new();
    for i in 1..=4 {
        ops.push(PatchOperation {
            common: PatchOperationCommon {
                operation_id: format!("op-rewrite-{i}"),
                panel_index: Some(i),
                panel_id: Some(format!("tp-{i}")),
                anchor: None,
                expected_old: Some(format!(
                    "2.6:: masterpiece ::, park, bush, night, pov, nakano miku, {i}"
                )),
                expected_old_hash: None,
                expected_project_version: 1,
            },
            kind: OperationKind::PatchPromptBlock {
                target: TextTarget::PanelPrompt,
                new_text: "beautiful cinematic anime scene, dramatic lighting, masterpiece".into(),
            },
        });
    }
    ops.push(identity_op());
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![1, 2, 3, 4]);
    let report = run(&p, &base);
    assert!(
        !report.anti_rewrite.passed,
        "anti-rewrite must fail on wholesale rewrite"
    );
    assert!(report
        .anti_rewrite
        .failures
        .iter()
        .any(|f| f.contains("preservation")));
}

#[test]
fn scene_token_replacement_outside_scene_intent_fails_scope() {
    let t = template_snapshot();
    let base = cloned_project(&t);
    let ops = vec![PatchOperation {
        common: PatchOperationCommon {
            operation_id: "op-scene".into(),
            panel_index: None,
            panel_id: None,
            anchor: None,
            expected_old: None,
            expected_old_hash: None,
            expected_project_version: 1,
        },
        kind: OperationKind::ReplaceSceneToken {
            replacements: vec![TokenReplacement {
                old_token: "park".into(),
                new_token: "office".into(),
            }],
            kept_tokens: Vec::new(),
        },
    }];
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![]);
    let report = run(&p, &base);
    assert!(!report.scope.passed);
    assert!(report
        .scope
        .failures
        .iter()
        .any(|f| f.contains("scene replacement outside")));
}

#[test]
fn identity_leak_detected_when_old_token_remains() {
    // Replace only the variant anchor, leaving bare `nakano miku` in prompts.
    let t = template_snapshot();
    let base = cloned_project(&t);
    let ops = vec![PatchOperation {
        common: PatchOperationCommon {
            operation_id: "op-partial".into(),
            panel_index: None,
            panel_id: None,
            anchor: None,
            expected_old: None,
            expected_old_hash: None,
            expected_project_version: 1,
        },
        kind: OperationKind::ReplaceCharacterIdentity {
            replacements: vec![TokenReplacement {
                old_token: "nakano miku (school uniform)".into(),
                new_token: "hoshino ai (idol)".into(),
            }],
            slots: Some(vec![0]),
            appearance_replacements: Vec::new(),
        },
    }];
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![]);
    let report = run(&p, &base);
    assert!(!report.identity_leak.passed);
    assert!(report
        .identity_leak
        .failures
        .iter()
        .any(|f| f.contains("nakano miku")));
}

#[test]
fn resize_without_user_request_fails_scope() {
    let t = template_snapshot();
    let base = cloned_project(&t);
    let ops = vec![PatchOperation {
        common: PatchOperationCommon {
            operation_id: "op-resize".into(),
            panel_index: None,
            panel_id: None,
            anchor: None,
            expected_old: None,
            expected_old_hash: None,
            expected_project_version: 1,
        },
        kind: OperationKind::ResizeStoryboard {
            target_panel_count: 2,
        },
    }];
    let p = proposal(PatchIntent::Resize, ops, vec![]);
    let report = run(&p, &base);
    assert!(!report.scope.passed);
}

#[test]
fn stale_template_revision_fails_reference_integrity() {
    let t = template_snapshot();
    let mut base = cloned_project(&t);
    base.source.sha256 = "old-sha".into();
    let p = proposal(PatchIntent::CharacterReplace, vec![identity_op()], vec![]);
    let app = apply_proposal(&base, &p).unwrap();
    let ctx = ValidationContext {
        template: &t,
        template_metadata: &metadata(),
        base: &base,
        proposal: &p,
        draft: &app.draft,
        applied_touched_panels: app.touched_panels.clone(),
        current_template_sha: "t010sha", // storage now has a different revision
        config: ValidatorConfig::default(),
    };
    let report = validate(&ctx);
    assert!(!report.reference_integrity.passed);
}

#[test]
fn appearance_replacement_swaps_inherent_traits() {
    let t = template_snapshot();
    let base = cloned_project(&t);
    // old hair colour stays -> identity leak FAIL
    let ops = vec![PatchOperation {
        common: op_common(),
        kind: OperationKind::ReplaceCharacterIdentity {
            replacements: identity_replacements(),
            slots: None,
            appearance_replacements: vec![],
        },
    }];
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![]);
    let report = run(&p, &base);
    assert!(
        report.identity_leak.passed,
        "name swap alone passes (traits inherited, warned)"
    );
    assert!(
        report
            .identity_leak
            .warnings
            .iter()
            .any(|w| w.contains("appearance_replacements")),
        "name-only swap must warn about inherited traits"
    );

    // declared appearance mapping removes the old trait AND its leak
    let ops = vec![PatchOperation {
        common: op_common(),
        kind: OperationKind::ReplaceCharacterIdentity {
            replacements: identity_replacements(),
            slots: None,
            appearance_replacements: vec![TokenReplacement {
                old_token: "pink hair".into(),
                new_token: "blue hair".into(),
            }],
        },
    }];
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![]);
    let report = run(&p, &base);
    assert!(report.identity_leak.passed);
    assert!(!report
        .identity_leak
        .warnings
        .iter()
        .any(|w| w.contains("appearance_replacements")));
}

#[test]
fn scene_op_present_makes_metadata_leak_strict() {
    let t = template_snapshot();
    let base = cloned_project(&t);
    // map only `park`; `bush` (environment tag) is left unmapped -> FAIL
    let ops = vec![PatchOperation {
        common: op_common(),
        kind: OperationKind::ReplaceSceneToken {
            replacements: vec![TokenReplacement {
                old_token: "park".into(),
                new_token: "office".into(),
            }],
            kept_tokens: vec![],
        },
    }];
    let p = proposal(PatchIntent::SceneAdapt, ops, vec![]);
    let report = run(&p, &base);
    assert!(
        !report.scene_leak.passed,
        "unmapped old environment token must FAIL while a scene op is in flight: {:?}",
        report.scene_leak.failures
    );

    // declaring the survivor in kept_tokens keeps it legitimately
    let ops = vec![PatchOperation {
        common: op_common(),
        kind: OperationKind::ReplaceSceneToken {
            replacements: vec![TokenReplacement {
                old_token: "park".into(),
                new_token: "office".into(),
            }],
            kept_tokens: vec!["bush".into()],
        },
    }];
    let p = proposal(PatchIntent::SceneAdapt, ops, vec![]);
    let report = run(&p, &base);
    assert!(
        report.scene_leak.passed,
        "declared keeps are legal: {:?}",
        report.scene_leak.failures
    );
}

#[test]
fn clothing_chain_gate_blocks_guard_block_left_behind() {
    let mut t = template_snapshot();
    // build a real clothing arc into the template panels
    if let Some(panels) = t.raw["panels"].as_array_mut() {
        panels[0]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, 1.3::pantyhose::");
        panels[1]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, 0.3::torn pantyhose::");
        panels[2]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, bare legs, -2::pantyhose::");
        panels[3]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, bare legs, -4::pantyhose::");
    }
    let base = cloned_project(&t);

    // consistent swap: full chain mirrored -> gate passes
    let mut good = base.raw.clone();
    if let Some(panels) = good["panels"].as_array_mut() {
        panels[0]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, 1.3::black pantyhose::");
        panels[1]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, 0.3::torn black pantyhose::");
        panels[2]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, bare legs, -2::black pantyhose::");
        panels[3]["customCharacters"][0]["prompt"] =
            json!(", official style, nakano miku, bare legs, -4::black pantyhose::");
    }
    let ops = vec![PatchOperation {
        common: op_common(),
        kind: OperationKind::ReplaceCharacterIdentity {
            replacements: vec![TokenReplacement {
                old_token: "pantyhose".into(),
                new_token: "black pantyhose".into(),
            }],
            slots: None,
            appearance_replacements: vec![],
        },
    }];
    let p = proposal(PatchIntent::CharacterReplace, ops, vec![]);
    let ctx = ValidationContext {
        template: &t,
        template_metadata: &metadata(),
        base: &base,
        proposal: &p,
        draft: &good,
        applied_touched_panels: [1u32, 2, 3, 4].into_iter().collect(),
        current_template_sha: "t010sha",
        config: ValidatorConfig::default(),
    };
    let report = validate(&ctx);
    assert!(
        report.clothing_chain.passed,
        "mirrored chain must pass: {:?}",
        report.clothing_chain.failures
    );

    // partial swap: one guard block still says the old word -> gate fails
    let mut partial = good.clone();
    partial["panels"][2]["customCharacters"][0]["prompt"] =
        json!(", official style, nakano miku, bare legs, -2::pantyhose::");
    let ctx = ValidationContext {
        template: &t,
        template_metadata: &metadata(),
        base: &base,
        proposal: &p,
        draft: &partial,
        applied_touched_panels: [1u32, 2, 3, 4].into_iter().collect(),
        current_template_sha: "t010sha",
        config: ValidatorConfig::default(),
    };
    let report = validate(&ctx);
    assert!(
        !report.clothing_chain.passed,
        "un-swapped negative guard must fail the chain gate: {:?}",
        report.clothing_chain.failures
    );
}
