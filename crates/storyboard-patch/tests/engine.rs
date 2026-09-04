use serde_json::json;
use storyboard_domain::{
    OperationKind, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal, ProjectId,
    ProjectSnapshot, SeedStrategy, TemplateId, TextTarget, TokenReplacement,
};
use storyboard_patch::{apply_proposal, text_hash};

fn project(panels: usize) -> ProjectSnapshot {
    let mut ps = Vec::new();
    for i in 1..=panels {
        ps.push(json!({
            "id": format!("panel-{i:04}"), "index": i, "title": "旧タイトル",
            "prompt": format!("2.6:: masterpiece ::, park, night, pov, official style, nakano miku, {i}"),
            "preciseReferences": [], "charactersMode": "custom", "characterRefs": [],
            "customCharacters": [
                {"prompt": ", official style, nakano miku (school uniform), pink hair, 3::pantyhose::",
                 "negativePrompt": "", "useCoords": false, "x": 0.9, "y": 0.3},
                {"prompt": "boy, standing, 3::standing in front of girl::", "negativePrompt": "", "useCoords": false, "x": 0.5, "y": 0.5}
            ],
            "paramsOverride": {"enabled": true, "params": {
                "stylePrompt":"", "steps":28, "cfgScale":6, "cfgRescale":0.5, "seed":1000+i,
                "sampler":"k_euler_ancestral", "noiseSchedule":"karras", "smea":false,
                "smeaDyn":false, "model":"m", "ucPreset":3, "qualityPreset":"none",
                "variety":false, "seedMode":"fixed"}},
            "status":"ready", "candidates":[], "imageSize":{"width":832,"height":1216}
        }));
    }
    ProjectSnapshot {
        project_id: ProjectId::generate(),
        version: 1,
        title: "旧タイトル".into(),
        source: storyboard_domain::SourceTemplateRef {
            template_id: TemplateId::new("T010"),
            revision_id: storyboard_domain::RevisionId::new("rev_x"),
            sha256: "abc".into(),
        },
        raw: json!({
            "schemaVersion": 2, "id": "proj-1", "title": "旧タイトル",
            "globalStylePrompt": "", "globalNegativePrompt": "neg",
            "sizeMode": "uniform", "initialGenerationCount": 1,
            "globalParams": {"seed": 1},
            "preciseReferences": [], "characters": [], "panels": ps
        }),
    }
}

fn proposal(ops: Vec<PatchOperation>, touched: Vec<u32>) -> PatchProposal {
    PatchProposal {
        base_project_version: 1,
        primary_template_id: TemplateId::new("T010"),
        intent_hash: "h".into(),
        intent: PatchIntent::CharacterReplace,
        operations: ops,
        touched_panels: touched,
        expected_preservation_ratio: 0.9,
        rationale: vec!["swap identity".into()],
        user_requested_resize: false,
    }
}

fn common(op_id: &str, panel: Option<u32>) -> PatchOperationCommon {
    PatchOperationCommon {
        operation_id: op_id.into(),
        panel_index: panel,
        panel_id: panel.map(|i| format!("panel-{:04}", i)),
        anchor: None,
        expected_old: None,
        expected_old_hash: None,
        expected_project_version: 1,
    }
}

#[test]
fn identity_replacement_touches_only_identity_tokens() {
    let base = project(3);
    let p = proposal(
        vec![PatchOperation {
            common: common("op1", None),
            kind: OperationKind::ReplaceCharacterIdentity {
                replacements: vec![
                    TokenReplacement {
                        old_token: "nakano miku".into(),
                        new_token: "hoshino ai".into(),
                    },
                    TokenReplacement { old_token: "pink hair".into(), new_token: "purple hair".into() },
                ],
                slots: None,
            },
        }],
        vec![],
    );
    let app = apply_proposal(&base, &p).unwrap();
    for i in 0..3 {
        let prompt = app.draft["panels"][i]["prompt"].as_str().unwrap();
        assert!(prompt.contains("hoshino ai"));
        assert!(!prompt.contains("nakano miku"));
        // non-identity tokens untouched
        assert!(prompt.contains("park"));
        assert!(prompt.contains("pov"));
        assert!(prompt.contains("2.6:: masterpiece ::"));
        let cc0 = app.draft["panels"][i]["customCharacters"][0]["prompt"].as_str().unwrap();
        assert!(cc0.contains("hoshino ai (school uniform)"));
        assert!(cc0.contains("purple hair"));
        assert!(cc0.contains("3::pantyhose::"));
    }
    // male block untouched
    let cc1 = app.draft["panels"][0]["customCharacters"][1]["prompt"].as_str().unwrap();
    assert_eq!(cc1, "boy, standing, 3::standing in front of girl::");
}

#[test]
fn stale_patch_rejected() {
    let base = project(2);
    let mut p = proposal(vec![], vec![]);
    p.base_project_version = 2; // project is at v1
    let err = apply_proposal(&base, &p).unwrap_err();
    assert!(matches!(err, storyboard_domain::PatchError::StalePatch { expected: 2, current: 1 }));
}

#[test]
fn missing_token_fails_precondition() {
    let base = project(2);
    let p = proposal(
        vec![PatchOperation {
            common: common("op1", None),
            kind: OperationKind::ReplaceCharacterIdentity {
                replacements: vec![TokenReplacement {
                    old_token: "nonexistent girl".into(),
                    new_token: "x".into(),
                }],
                slots: None,
            },
        }],
        vec![],
    );
    let err = apply_proposal(&base, &p).unwrap_err();
    assert!(matches!(err, storyboard_domain::PatchError::PreconditionFailed { .. }));
}

#[test]
fn wrong_panel_id_fails_precondition() {
    let base = project(2);
    let mut c = common("op1", Some(1));
    c.panel_id = Some("someone-elses-id".into());
    let p = proposal(
        vec![PatchOperation { common: c, kind: OperationKind::RegenerateIds }],
        vec![],
    );
    assert!(matches!(
        apply_proposal(&base, &p).unwrap_err(),
        storyboard_domain::PatchError::PreconditionFailed { .. }
    ));
}

#[test]
fn patch_prompt_block_replaces_exact_block_once() {
    let base = project(2);
    let expected = "park, night";
    let mut c = common("op1", Some(1));
    c.expected_old = Some(expected.into());
    c.expected_old_hash = Some(text_hash(expected));
    c.anchor = Some("park".into());
    let p = proposal(
        vec![PatchOperation {
            common: c,
            kind: OperationKind::PatchPromptBlock {
                target: TextTarget::PanelPrompt,
                new_text: "office, night".into(),
            },
        }],
        vec![1],
    );
    let app = apply_proposal(&base, &p).unwrap();
    let prompt = app.draft["panels"][0]["prompt"].as_str().unwrap();
    assert!(prompt.contains("office, night"));
    assert!(!prompt.contains("park"));
    // other panel untouched
    assert!(app.draft["panels"][1]["prompt"].as_str().unwrap().contains("park"));
}

#[test]
fn anchor_not_found_is_stale() {
    let base = project(2);
    let mut c = common("op1", Some(1));
    c.expected_old = Some("this text was already changed".into());
    let p = proposal(
        vec![PatchOperation {
            common: c,
            kind: OperationKind::PatchPromptBlock { target: TextTarget::PanelPrompt, new_text: "x".into() },
        }],
        vec![1],
    );
    assert!(matches!(
        apply_proposal(&base, &p).unwrap_err(),
        storyboard_domain::PatchError::AnchorNotFound { .. }
    ));
}

#[test]
fn update_title_updates_all_panels() {
    let base = project(3);
    let p = proposal(
        vec![PatchOperation {
            common: common("op1", None),
            kind: OperationKind::UpdateTitle { new_title: "新タイトル".into() },
        }],
        vec![],
    );
    let app = apply_proposal(&base, &p).unwrap();
    assert_eq!(app.draft["title"], "新タイトル");
    for i in 0..3 {
        assert_eq!(app.draft["panels"][i]["title"], "新タイトル");
    }
}

#[test]
fn resize_compress_and_expand() {
    let base = project(10);
    let p = proposal(
        vec![PatchOperation {
            common: common("op1", None),
            kind: OperationKind::ResizeStoryboard { target_panel_count: 5 },
        }],
        vec![],
    );
    let app = apply_proposal(&base, &p).unwrap();
    let n = app.draft["panels"].as_array().unwrap().len();
    assert_eq!(n, 5);
    // indexes renumbered, first kept, last kept
    assert_eq!(app.draft["panels"][0]["prompt"].as_str().unwrap().trim_end_matches('1'), base.raw["panels"][0]["prompt"].as_str().unwrap().trim_end_matches('1'));
    let last_new = app.draft["panels"][4]["prompt"].as_str().unwrap();
    assert!(last_new.ends_with("10"));
    for (i, p) in app.draft["panels"].as_array().unwrap().iter().enumerate() {
        assert_eq!(p["index"].as_u64().unwrap(), (i + 1) as u64);
    }

    // expand back to 12
    let p2 = proposal(
        vec![PatchOperation {
            common: common("op2", None),
            kind: OperationKind::ResizeStoryboard { target_panel_count: 12 },
        }],
        vec![],
    );
    let app2 = apply_proposal(&base, &p2).unwrap();
    assert_eq!(app2.draft["panels"].as_array().unwrap().len(), 12);
}

#[test]
fn seeds_regenerate_strategies() {
    let base = project(2);
    let p = proposal(
        vec![PatchOperation {
            common: common("op1", None),
            kind: OperationKind::RegenerateSeeds { strategy: SeedStrategy::Fixed(777) },
        }],
        vec![],
    );
    let app = apply_proposal(&base, &p).unwrap();
    assert_eq!(app.draft["panels"][0]["paramsOverride"]["params"]["seed"].as_u64().unwrap(), 777);
}
