//! Golden Cases A–H (plan Table 19) against the real 30-template fixture.

use app_server::AppServer;
use model_providers::{MockProvider, TurnResponse};
use serde_json::json;
use std::sync::Arc;
use agent_runtime::{AgentRuntime, ApprovalMode, ApprovalPolicy, RuntimeConfig};
use storyboard_domain::{
    diff, OperationKind, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal,
    ProjectId, TemplateId, TextTarget, TokenReplacement,
};
use storyboard_importer::skill::SkillBundle;
use storyboard_tools::{AgentProfile, ToolBackend};

fn fixture_skill() -> SkillBundle {
    let root = env!("CARGO_MANIFEST_DIR").to_string() + "/../../fixtures/current-skill";
    SkillBundle::open_dir(root).expect("fixture skill")
}

fn server(tag: &str) -> AppServer {
    let dir = std::env::temp_dir().join(format!("sbx-golden-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    AppServer::init(&dir, &fixture_skill()).expect("init workspace")
}

fn identity_patch(base_version: u64, template_id: &str, mappings: Vec<(String, String)>) -> PatchProposal {
    PatchProposal {
        base_project_version: base_version,
        primary_template_id: TemplateId::new(template_id),
        intent_hash: "golden".into(),
        intent: PatchIntent::CharacterReplace,
        touched_panels: vec![],
        expected_preservation_ratio: 0.90,
        rationale: vec![],
        user_requested_resize: false,
        operations: vec![PatchOperation {
            common: PatchOperationCommon {
                operation_id: "op-identity".into(),
                panel_index: None,
                panel_id: None,
                anchor: None,
                expected_old: None,
                expected_old_hash: None,
                expected_project_version: base_version,
            },
            kind: OperationKind::ReplaceCharacterIdentity {
                replacements: mappings
                    .into_iter()
                    .map(|(old_token, new_token)| TokenReplacement { old_token, new_token })
                    .collect(),
                slots: None,
            },
        }],
    }
}

/// Case A — Exact Clone: same scene + new character. Same-family template is
/// selected; only identity blocks change; everything else is byte-stable.
#[test]
fn case_a_exact_clone() {
    let server = server("a");
    let intent = server.parse_intent("夜间公园里 1女 被匿名男强暴");
    assert_eq!(intent.scene_family.as_deref(), Some("park"));
    let sel = server.match_templates(&intent, Some(7)).unwrap().expect("selection");
    assert_eq!(sel.primary.scene_family, "park");
    assert!(!sel.needs_scene_adaptation);

    let state = server.clone_project(sel.primary.template_id.as_str(), Some("A".into()), 1).unwrap();
    let pid = state.project_id;
    let base = server.load_project_snapshot(&pid).unwrap();

    let meta = server.db.get_template_metadata(sel.primary.template_id.as_str()).unwrap();
    let mappings = meta
        .character_anchor_variants
        .iter()
        .map(|v| (v.clone(), "hoshino ai".to_string()))
        .collect();
    let proposal = identity_patch(1, sel.primary.template_id.as_str(), mappings);

    let (report, app) = server.validate_patch(&pid, &proposal).unwrap();
    assert!(report.passed, "gates: {report:#?}");
    assert!(report.preservation_ratio >= 0.95);

    // only identity text changed: prompts differ only in mapped tokens
    for (b, n) in base.panels().iter().zip(app.draft.get("panels").unwrap().as_array().unwrap()) {
        let bp = b["prompt"].as_str().unwrap().replace("nakano miku", "X");
        let np = n["prompt"].as_str().unwrap().replace("hoshino ai", "X");
        assert_eq!(bp, np, "non-identity prompt text must be identical");
    }
    // global negative untouched, panel count inherited
    assert_eq!(app.draft["globalNegativePrompt"], base.raw["globalNegativePrompt"]);
    assert_eq!(app.draft["panels"].as_array().unwrap().len(), base.panels().len());
}

/// Case B — Scene Clone: structure kept, location swapped. Scene blocks
/// change; camera/order/panel count stay stable.
#[test]
fn case_b_scene_clone() {
    let server = server("b");
    let state = server.clone_project("T010", Some("B".into()), 2).unwrap();
    let pid = state.project_id;
    let base = server.load_project_snapshot(&pid).unwrap();

    let base_text = serde_json::to_string(&base.raw).unwrap().to_lowercase();
    let candidates = [
        ("park", "office"),
        ("bush", "storage racks"),
        ("cardboard", "office chair"),
        ("tree", "window"),
    ];
    let mappings: Vec<(String, String)> = candidates
        .into_iter()
        .filter(|(old, _)| base_text.contains(old))
        .map(|(o, n)| (o.to_string(), n.to_string()))
        .collect();
    assert!(mappings.len() >= 3);
    let proposal = PatchProposal {
        base_project_version: 1,
        primary_template_id: TemplateId::new("T010"),
        intent_hash: "b".into(),
        intent: PatchIntent::SceneAdapt,
        touched_panels: vec![],
        expected_preservation_ratio: 0.80,
        rationale: vec![],
        user_requested_resize: false,
        operations: vec![PatchOperation {
            common: PatchOperationCommon {
                operation_id: "op-scene".into(),
                panel_index: None,
                panel_id: None,
                anchor: None,
                expected_old: None,
                expected_old_hash: None,
                expected_project_version: 1,
            },
            kind: OperationKind::ReplaceSceneToken { replacements: mappings.into_iter().map(|(o, n)| TokenReplacement { old_token: o, new_token: n }).collect() },
        }],
    };
    let (report, app) = server.validate_patch(&pid, &proposal).unwrap();
    assert!(report.passed, "gates: {report:#?}");
    assert_eq!(app.draft["panels"].as_array().unwrap().len(), base.panels().len());
    // camera schedule inherited
    let text = serde_json::to_string(&app.draft).unwrap().to_lowercase();
    assert!(text.contains("pov"));
    // scene tokens replaced (spot check: no whole-token `park` remains in prompts)
    for p in app.draft["panels"].as_array().unwrap() {
        let prompt = p["prompt"].as_str().unwrap_or("");
        let lower = prompt.to_lowercase();
        assert!(
            !(lower.contains(" park,") || lower.contains(" park ") || lower.starts_with("park")),
            "scene leak in panel prompt: {lower}"
        );
    }
}

/// Case C — No exact match: exotic scene. Nearest template wins, flagged for
/// scene adaptation; never a fabricated perfect match.
#[test]
fn case_c_no_exact_match() {
    let server = server("c");
    let intent = server.parse_intent("火山口里 1女 被匿名男强暴");
    let sel = server.match_templates(&intent, Some(3)).unwrap().expect("selection");
    assert!(sel.needs_scene_adaptation);
    assert!(sel.primary.score < 0.55);
}

/// Case D — Wrong legacy index: importer rescans and the rebuilt stats (not
/// the legacy counts) take effect.
#[test]
fn case_d_importer_fixes_wrong_index() {
    let server = server("d");
    let all = server.template_metadata().unwrap();
    assert_eq!(all.len(), 30);

    let mut mismatch_fixed = 0;
    for m in &all {
        if let Some(legacy) = &m.legacy {
            if !legacy.mismatches.is_empty() {
                mismatch_fixed += 1;
                // rebuilt stats must be internally consistent
                let female = m.female_lead_count.unwrap_or(0);
                let male = m.male_lead_count.unwrap_or(0);
                assert_eq!(m.total_role_count, female + male, "{}", m.template_id);
                // and must match a fresh (or text-verified) scan of the anchors
                assert_eq!(m.character_anchors.len() as u32, female, "{}", m.template_id);
                // every multi-role template documents its roles somehow
                assert!(female > 0 || m.male_lead_count.unwrap_or(0) > 0, "{}", m.template_id);
            }
        }
    }
    // after text-verified anchor merging most legacy inconsistencies resolve;
    // the remainder are genuine data problems that MUST be recorded.
    assert!(mismatch_fixed >= 1, "expected genuine legacy inconsistencies to be recorded, got {mismatch_fixed}");

    // max_simultaneous_slots computed from real panels, not index text
    let t010 = all.iter().find(|m| m.template_id == "T010").unwrap();
    assert_eq!(t010.max_simultaneous_slots, 2);
}

/// Case E — Agent overreach: wholesale prompt rewrite must fail
/// Scope/Anti-Rewrite gates and be rejected.
#[test]
fn case_e_agent_overreach_blocked() {
    let server = server("e");
    let state = server.clone_project("T010", Some("E".into()), 3).unwrap();
    let pid = state.project_id;
    let base = server.load_project_snapshot(&pid).unwrap();

    // rewrite 30 panels wholesale with "nicer" prompts
    let mut ops = Vec::new();
    for i in 1..=30u32 {
        let old = base.panels()[(i - 1) as usize]["prompt"].as_str().unwrap().to_string();
        ops.push(PatchOperation {
            common: PatchOperationCommon {
                operation_id: format!("op-rewrite-{i}"),
                panel_index: Some(i),
                panel_id: None,
                anchor: None,
                expected_old: Some(old),
                expected_old_hash: None,
                expected_project_version: 1,
            },
            kind: OperationKind::PatchPromptBlock {
                target: TextTarget::PanelPrompt,
                new_text: "masterpiece, best quality, beautiful cinematic scene, dramatic lighting".into(),
            },
        });
    }
    let proposal = PatchProposal {
        base_project_version: 1,
        primary_template_id: TemplateId::new("T010"),
        intent_hash: "e".into(),
        intent: PatchIntent::UserDelta,
        touched_panels: (1..=30).collect(),
        expected_preservation_ratio: 0.90,
        rationale: vec![],
        user_requested_resize: false,
        operations: ops,
    };
    let (patch_id, report) = server.propose_patch(&pid, &proposal, None).unwrap();
    assert!(!report.passed);
    assert!(!report.anti_rewrite.passed, "anti-rewrite must catch wholesale rewrite");
    // project state must land in PatchRejected
    let row = server.db.get_project(&pid).unwrap();
    assert_eq!(row.status, storyboard_domain::ProjectStatus::PatchRejected);
    // and the rejected patch can never commit
    let err = server.commit_patch(&pid, patch_id).unwrap_err();
    assert!(err.to_string().contains("only `approved`"));
}

/// Case F — Stale patch: wrong base version → STALE_PATCH; changed anchor →
/// PRECONDITION_FAILED / AnchorNotFound. Never fuzzy-matched.
#[test]
fn case_f_stale_patch_rejected() {
    let server = server("f");
    let state = server.clone_project("T010", Some("F".into()), 4).unwrap();
    let pid = state.project_id;

    // (1) wrong base version
    let mut p = identity_patch(9, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    p.base_project_version = 9;
    let err = match server.validate_patch(&pid, &p) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("stale base version must be rejected"),
    };
    assert!(err.contains("STALE_PATCH"), "{err}");

    // (2) commit v2, then re-use a v1-based proposal → stale
    let p1 = identity_patch(1, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    let (patch_id, report) = server.propose_patch(&pid, &p1, None).unwrap();
    assert!(report.passed);
    server.resolve_approval(&pid, patch_id, true).unwrap();
    server.commit_patch(&pid, patch_id).unwrap();

    let p2 = identity_patch(1, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    let err = match server.validate_patch(&pid, &p2) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("proposal against old version must be rejected"),
    };
    assert!(err.contains("STALE_PATCH"), "{err}");

    // (3) expected_old no longer present
    let p3 = PatchProposal {
        base_project_version: 2,
        primary_template_id: TemplateId::new("T010"),
        intent_hash: "f3".into(),
        intent: PatchIntent::UserDelta,
        touched_panels: vec![1],
        expected_preservation_ratio: 0.9,
        rationale: vec![],
        user_requested_resize: false,
        operations: vec![PatchOperation {
            common: PatchOperationCommon {
                operation_id: "op-block".into(),
                panel_index: Some(1),
                panel_id: None,
                anchor: Some("already replaced".into()),
                expected_old: Some("this block was already replaced".into()),
                expected_old_hash: None,
                expected_project_version: 2,
            },
            kind: OperationKind::PatchPromptBlock { target: TextTarget::PanelPrompt, new_text: "x".into() },
        }],
    };
    let err = match server.validate_patch(&pid, &p3) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("missing anchor must be rejected"),
    };
    assert!(err.contains("ANCHOR_NOT_FOUND"), "{err}");
}

/// Case G — Rollback: after a commit, restoring the parent snapshot returns
/// the project byte-for-byte to v1 content.
#[test]
fn case_g_rollback() {
    let server = server("g");
    let state = server.clone_project("T010", Some("G".into()), 5).unwrap();
    let pid = state.project_id;
    let v1_bytes = server.workspace.read_project_version(&pid, 1).unwrap();

    let p = identity_patch(1, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    let (patch_id, report) = server.propose_patch(&pid, &p, None).unwrap();
    assert!(report.passed);
    server.resolve_approval(&pid, patch_id, true).unwrap();
    let outcome = server.commit_patch(&pid, patch_id).unwrap();
    assert_eq!(outcome.new_version, 2);

    // v2 actually differs
    let v2_bytes = server.workspace.read_project_version(&pid, 2).unwrap();
    assert_ne!(v1_bytes, v2_bytes);

    // diff file exists and is structured
    let diff_bytes = std::fs::read_to_string(&outcome.diff_path).unwrap();
    let d: diff::ProjectDiff = serde_json::from_str(&diff_bytes).unwrap();
    assert!(d.summary.panels_modified > 0);
    assert!(d.summary.preservation_ratio > 0.9);

    let v3 = server.rollback(&pid, 1).unwrap();
    assert_eq!(v3, 3);
    let v3_bytes = server.workspace.read_project_version(&pid, 3).unwrap();
    assert_eq!(v1_bytes, v3_bytes, "rollback must restore the parent snapshot byte-for-byte");
}

/// Case H — Commit boundary: the production agent has no commit tool; only
/// the Application Controller commits approved patches.
#[test]
fn case_h_commit_boundary() {
    let server = server("h");

    // (1) commit_storyboard_patch is not registered on the production profile
    let reg = storyboard_tools::ToolRegistry::for_profile(AgentProfile::StoryboardProduction);
    assert!(!reg.tool_names().contains(&"commit_storyboard_patch"));
    let err = reg.dispatch("commit_storyboard_patch", &json!({}), None, &server as &dyn ToolBackend).unwrap_err();
    assert!(matches!(err, storyboard_tools::ToolError::UnknownTool(_)));

    // (2) commit requires an approved patch — the controller guards it
    let state = server.clone_project("T010", Some("H".into()), 6).unwrap();
    let pid = state.project_id;
    let p = identity_patch(1, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    let (patch_id, _) = server.propose_patch(&pid, &p, None).unwrap();
    let err = server.commit_patch(&pid, patch_id).unwrap_err();
    assert!(err.to_string().contains("only `approved` patches"));

    // (3) after approval + commit the agent still has no write path — but the
    // controller succeeds
    server.resolve_approval(&pid, patch_id, true).unwrap();
    let outcome = server.commit_patch(&pid, patch_id).unwrap();
    assert_eq!(outcome.new_version, 2);
}

/// Agent e2e with a scripted (mock) provider: search → propose → validate →
/// NeedsApproval; manifest complete; events observable.
#[test]
fn agent_e2e_mock_provider_full_loop() {
    let server = server("agent");
    let state = server.clone_project("T010", Some("Agent".into()), 7).unwrap();
    let pid = state.project_id;

    let proposal = identity_patch(1, "T010", vec![("nakano miku".into(), "hoshino ai".into())]);
    let proposal_json = serde_json::to_string(&proposal).unwrap();

    // script 1: search; script 2: propose; script 3: validate
    let mk = |name: &str, args: String| TurnResponse {
        message: model_providers::ChatMessage {
            role: model_providers::Role::Assistant,
            content: String::new(),
            tool_calls: vec![model_providers::ToolCall { id: format!("c-{name}"), name: name.into(), arguments_json: args }],
            tool_call_id: None,
        },
        finish_reason: "tool_calls".into(),
        usage: None,
    };
    let provider = MockProvider::new(vec![
        mk("search_templates", json!({"query": {"text": "公园 夜 强暴"}}).to_string()),
        mk("propose_storyboard_patch", json!({"project_id": pid.to_string(), "proposal": serde_json::from_str::<serde_json::Value>(&proposal_json).unwrap()}).to_string()),
        mk("validate_storyboard_patch", json!({"project_id": pid.to_string(), "proposal": serde_json::from_str::<serde_json::Value>(&proposal_json).unwrap()}).to_string()),
    ]);

    let bus = std::sync::Arc::new(agent_protocol::EventBus::new());
    let rx = bus.subscribe();
    let runtime = AgentRuntime::new(
        RuntimeConfig {
            profile: AgentProfile::StoryboardProduction,
            approval: ApprovalPolicy { mode: ApprovalMode::AlwaysPrompt },
            ..Default::default()
        },
        Arc::new(provider),
        bus,
    );

    // F07: the app server observes the run — manifest at turn start, every
    // event streamed into agent_events.
    let out = runtime.run_turn(
        "thread-1",
        Some(&pid.to_string()),
        "把角色换成星野爱",
        &server,
        Some(&server as &dyn agent_runtime::RunObserver),
    );
    assert!(matches!(out.status, agent_runtime::TurnStatus::NeedsApproval { .. }));
    // manifest completeness (F07 / Run Manifest 完整率 100%)
    assert!(!out.manifest.run_id.is_empty());
    assert_eq!(out.manifest.provider_id, "mock");
    assert!(!out.manifest.core_contract_hash.is_empty());
    assert!(!out.manifest.tool_registry_version.is_empty());
    assert!(out.manifest.base_project_version.is_some());
    // manifest persisted automatically at turn start (not post-hoc)
    assert!(server.workspace.root.join("runs").join(&out.run_id).join("manifest.json").is_file());
    assert!(server.db.has_agent_run(&out.run_id).unwrap());

    // events were emitted through the bus AND persisted with monotonic seq
    let mut kinds = Vec::new();
    while let Ok(e) = rx.try_recv() {
        kinds.push(e.type_name());
    }
    assert!(kinds.contains(&"turn.started"));
    assert!(kinds.contains(&"validator.completed"));
    assert!(kinds.contains(&"approval.requested"));
    let persisted = server.db.list_agent_events("thread-1").unwrap();
    assert!(persisted.len() >= 3);
    assert!(persisted.iter().any(|(_, t)| t == "validator.completed"));
    for (i, (seq, _)) in persisted.iter().enumerate() {
        assert_eq!(*seq, (i + 1) as u64, "per-thread seq must be 1..=n contiguous");
    }

    // commit via controller after (mock) user approval
    let patch = server.db.latest_patch(&pid).unwrap();
    server.resolve_approval(&pid, patch.id, true).unwrap();
    let outcome = server.commit_patch(&pid, patch.id).unwrap();
    assert_eq!(outcome.new_version, 2);
    // §21 chain ends in Versioned
    let row = server.db.get_project(&pid).unwrap();
    assert_eq!(row.status, storyboard_domain::ProjectStatus::Versioned);
}

/// §21 commit chain: approval → CommitRequested → Committed → Versioned.
#[test]
fn status_machine_commit_chain() {
    use storyboard_domain::ProjectStatus as S;
    let chain = [
        (S::Draft, S::Matched),
        (S::Matched, S::Cloned),
        (S::Cloned, S::PatchProposed),
        (S::PatchProposed, S::Validating),
        (S::Validating, S::AwaitingApproval),
        (S::AwaitingApproval, S::CommitRequested),
        (S::CommitRequested, S::Committed),
        (S::Committed, S::Versioned),
    ];
    let mut cur = S::Draft;
    for (from, to) in chain {
        assert_eq!(cur, from);
        cur = cur.transition(to).unwrap();
    }
    assert_eq!(cur, S::Versioned);
    // illegal jumps are rejected
    assert!(S::AwaitingApproval.transition(S::Versioned).is_err());
    assert!(S::Cloned.transition(S::Committed).is_err());
}
