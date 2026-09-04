//! `sbx` — Storyboard Studio CLI. Headless access to the same Application
//! Controller the desktop UI talks to (import / match / clone / patch /
//! validate / commit / rollback / export + an end-to-end demo).

use clap::{Parser, Subcommand};
use serde_json::json;
use app_server::AppServer;
use storyboard_domain::{OperationKind, PatchIntent, PatchOperation, PatchOperationCommon, PatchProposal, ProjectId, TemplateId, TokenReplacement};
use storyboard_importer::skill::SkillBundle;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sbx", version, about = "NovelAI Storyboard Studio — local-first template-driven workbench")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialize a workspace and import the frozen skill bundle.
    Init {
        workspace: PathBuf,
        #[arg(long, default_value = "fixtures/current-skill")]
        skill: PathBuf,
    },
    /// List imported templates.
    ListTemplates { workspace: PathBuf },
    /// Parse + match templates for a natural-language query.
    Match { workspace: PathBuf, query: String, #[arg(long)] seed: Option<u64> },
    /// Deep-clone a template into a new project (v1).
    Clone { workspace: PathBuf, template_id: String, #[arg(long)] title: Option<String>, #[arg(long, default_value_t = 42)] seed: u64 },
    /// List projects.
    ListProjects { workspace: PathBuf },
    /// End-to-end deterministic demo: clone -> identity patch -> validate -> approve -> commit -> export -> rollback.
    Demo {
        workspace: PathBuf,
        #[arg(long, default_value = "fixtures/current-skill")]
        skill: PathBuf,
    },
    /// Export a project's current version.
    Export { workspace: PathBuf, project_id: String, out: PathBuf },
    /// Roll a project back to an older version snapshot.
    Rollback { workspace: PathBuf, project_id: String, to_version: u64 },
}

fn open_skill(path: &PathBuf) -> SkillBundle {
    if path.is_dir() {
        SkillBundle::open_dir(path).expect("open skill dir")
    } else {
        SkillBundle::open_zip(path).expect("open .skill archive")
    }
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { workspace, skill } => {
            let bundle = open_skill(&skill);
            let server = AppServer::init(&workspace, &bundle).expect("init");
            println!("workspace initialized at {}", server.workspace.root.display());
        }
        Cmd::ListTemplates { workspace } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            for m in server.template_metadata().unwrap() {
                println!(
                    "{}  {:<40} family={:<12} panels={:<3} roles={} confidence={:.2} warnings={}",
                    m.template_id,
                    truncate(&m.title, 40),
                    m.scene_family,
                    m.panel_count,
                    m.total_role_count,
                    m.metadata_confidence,
                    m.warnings.len()
                );
            }
        }
        Cmd::Match { workspace, query, seed } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            let intent = server.parse_intent(&query);
            println!("QueryIntent: {}", serde_json::to_string_pretty(&intent).unwrap());
            match server.match_templates(&intent, seed).unwrap() {
                Some(sel) => {
                    println!("\nPrimary: {} ({}) score={:.3} mode={:?} scene_adapt={}", sel.primary.template_id, sel.primary.title, sel.primary.score, sel.mode, sel.needs_scene_adaptation);
                    println!("breakdown: {}", serde_json::to_string(&sel.primary.breakdown).unwrap());
                    for c in &sel.candidates {
                        println!("  candidate: {} score={:.3}", c.template_id, c.score);
                    }
                }
                None => println!("no templates"),
            }
        }
        Cmd::Clone { workspace, template_id, title, seed } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            let state = server.clone_project(&template_id, title, seed).expect("clone");
            println!("project {} (v1) cloned from {} — title: {}", state.project_id, state.source_template_id, state.title);
        }
        Cmd::ListProjects { workspace } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            for p in server.db.list_projects().unwrap() {
                println!("{}  v{:<3} {:?}  {}", p.id, p.current_version, p.status, p.title);
            }
        }
        Cmd::Demo { workspace, skill } => {
            let bundle = open_skill(&skill);
            let server = AppServer::init(&workspace, &bundle).expect("init");
            demo(&server);
        }
        Cmd::Export { workspace, project_id, out } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            let pid: ProjectId = project_id.parse().expect("uuid");
            let path = server.export_json(&pid, &out).expect("export");
            println!("exported to {}", path.display());
        }
        Cmd::Rollback { workspace, project_id, to_version } => {
            let server = AppServer::open(&workspace).expect("open workspace");
            let pid: ProjectId = project_id.parse().expect("uuid");
            let v = server.rollback(&pid, to_version).expect("rollback");
            println!("rolled back; new current version v{v}");
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Deterministic pipeline used by the demo and golden tests:
/// clone T010, swap identity, validate, approve, commit, export, rollback.
fn demo(server: &AppServer) {
    // 1. match
    let intent = server.parse_intent("夜间公园里 1女 被匿名男强暴");
    let selection = server.match_templates(&intent, Some(7)).unwrap().expect("match");
    println!("【Template Match】 primary = {} ({}) score = {:.3}", selection.primary.template_id, selection.primary.title, selection.primary.score);

    // 2. clone
    let state = server.clone_project(selection.primary.template_id.as_str(), Some("夜の公園で犯される新人アイドル".into()), 1234).expect("clone");
    let pid = state.project_id;
    println!("cloned project {pid} ({} panels)", server.load_project_snapshot(&pid).unwrap().panel_count());

    // 3. identity replacement proposal (what the agent would emit).
    // C10: only map tokens that actually exist in the project (verified via
    // metadata anchors + a text scan), never guessed ones.
    let meta = server.db.get_template_metadata(selection.primary.template_id.as_str()).unwrap();
    let base_snap = server.load_project_snapshot(&pid).unwrap();
    let base_text = serde_json::to_string(&base_snap.raw).unwrap().to_lowercase();
    let mut replacements = Vec::new();
    for variant in &meta.character_anchor_variants {
        replacements.push(TokenReplacement { old_token: variant.clone(), new_token: "hoshino ai".into() });
    }
    for anchor in &meta.character_anchors {
        if !base_text.contains(&anchor.to_lowercase()) {
            continue;
        }
        if !replacements.iter().any(|r| r.old_token == *anchor) {
            replacements.push(TokenReplacement { old_token: anchor.clone(), new_token: "hoshino ai".into() });
        }
    }
    for (old, new) in [("headphone around neck", "star hairpin")] {
        if base_text.contains(old) {
            replacements.push(TokenReplacement { old_token: old.into(), new_token: new.into() });
        }
    }
    println!("identity mappings: {:?}", replacements.iter().map(|r| r.old_token.as_str()).collect::<Vec<_>>());
    let proposal = PatchProposal {
        base_project_version: 1,
        primary_template_id: TemplateId::new(selection.primary.template_id.as_str()),
        intent_hash: "demo".into(),
        intent: PatchIntent::CharacterReplace,
        touched_panels: vec![],
        expected_preservation_ratio: 0.90,
        rationale: vec!["user asked to swap the character".into()],
        user_requested_resize: false,
        operations: vec![PatchOperation {
            common: PatchOperationCommon {
                operation_id: "op-identity-1".into(),
                panel_index: None,
                panel_id: None,
                anchor: None,
                expected_old: None,
                expected_old_hash: None,
                expected_project_version: 1,
            },
            kind: OperationKind::ReplaceCharacterIdentity { replacements, slots: None },
        }],
    };

    // 4. propose + validate
    let (patch_id, report) = server.propose_patch(&pid, &proposal, Some("run_demo")).expect("propose");
    println!("patch {patch_id} proposed; passed = {}", report.passed);
    for g in report.gates() {
        println!("  {:<20} {} {}", g.gate, if g.passed { "PASS" } else { "FAIL" }, g.failures.join("; "));
    }

    // 5. approve + commit (Application Controller only)
    server.resolve_approval(&pid, patch_id, true).unwrap();
    let outcome = server.commit_patch(&pid, patch_id).expect("commit");
    println!("committed v{} (parent v{}), preservation = {:.3}", outcome.new_version, outcome.parent_version, outcome.preservation_ratio);

    // 6. export
    let out = server.workspace.root.join("exports").join("demo-export.json");
    let exported = server.export_json(&pid, &out).unwrap();
    println!("exported: {}", exported.display());

    // 7. rollback to v1 (F04: full parent snapshot)
    let v3 = server.rollback(&pid, 1).unwrap();
    println!("rolled back to v1 content as v{v3}");

    println!("\naudit tail:");
    for e in server.db.list_audit(6).unwrap() {
        println!("  {}", serde_json::to_string(&e).unwrap().chars().take(160).collect::<String>());
    }
    let _ = json!({});
}
