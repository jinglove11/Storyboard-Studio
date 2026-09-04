// Shared types mirroring the Rust side (serde-serialized).

export interface TemplateMetadata {
  template_id: string;
  revision_id: string;
  title: string;
  source_name: string;
  sha256: string;
  scene_family: string;
  exact_scene: string | null;
  scene_tags: string[];
  location_tags: string[];
  time_tags: string[];
  environment_tags: string[];
  total_role_count: number;
  female_lead_count: number | null;
  male_lead_count: number | null;
  max_simultaneous_slots: number;
  character_anchors: string[];
  character_anchor_variants: string[];
  male_identity: string | null;
  male_panel_ratio: number | null;
  panel_count: number;
  narrative_type: string | null;
  pace: string;
  camera_profile: string[];
  important_props: string[];
  keywords: string[];
  metadata_confidence: number;
  warnings: string[];
}

export interface ScoreBreakdown {
  scene: number;
  structure: number;
  characters: number;
  time: number;
  pace: number;
  camera_props: number;
  reasons: string[];
}

export interface Candidate {
  template_id: string;
  title: string;
  score: number;
  breakdown: ScoreBreakdown;
  scene_family: string;
  panel_count: number;
  total_role_count: number;
}

export interface Selection {
  primary: Candidate;
  candidates: Candidate[];
  mode: 'Deterministic' | 'WeightedRandom' | string;
  needs_scene_adaptation: boolean;
}

export interface ProjectRow {
  id: string;
  title: string;
  source_template_id: string;
  current_version: number;
  status: string;
  created_at: string;
  updated_at: string;
}

export interface GateResult {
  gate: string;
  passed: boolean;
  failures: string[];
  warnings: string[];
}

export interface ValidationReport {
  passed: boolean;
  schema: GateResult;
  scope: GateResult;
  anti_rewrite: GateResult;
  identity_leak: GateResult;
  scene_leak: GateResult;
  reference_integrity: GateResult;
  json_parse: GateResult;
  preservation_ratio: number;
}

export interface QueryIntent {
  scene_family?: string | null;
  exact_scene?: string | null;
  time?: string | null;
  character_count?: number | null;
  character_roles: string[];
  narrative_tags: string[];
  pace_hint?: string | null;
  desired_panel_count?: number | null;
  props: string[];
  camera_hints: string[];
  seed?: number | null;
  keywords: string[];
}

export interface CommitOutcome {
  project_id: string;
  new_version: number;
  parent_version: number;
  diff_path: string;
  preservation_ratio: number;
}

export interface AppEvent {
  type: string;
  [k: string]: unknown;
}
