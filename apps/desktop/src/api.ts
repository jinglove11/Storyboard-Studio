import { invoke } from '@tauri-apps/api/core';
import type {
  CommitOutcome, ProjectRow, QueryIntent, Selection, TemplateMetadata, ValidationReport,
} from './types';

// ---- commands exposed by src-tauri (thin bridge to AppServer) ----

export const api = {
  workspaceInfo: () => invoke<{ root: string; templates: number }>('workspace_info'),
  listTemplates: () => invoke<TemplateMetadata[]>('list_templates'),
  parseIntent: (text: string) => invoke<QueryIntent>('parse_intent', { text }),
  matchTemplates: (text: string, seed?: number | null) =>
    invoke<Selection>('match_templates', { text, seed: seed ?? null }),
  cloneProject: (templateId: string, title: string | null, seed?: number) =>
    invoke<ProjectRow>('clone_project', { templateId, title, seed: seed ?? 42 }),
  listProjects: () => invoke<ProjectRow[]>('list_projects'),
  projectVersions: (projectId: string) => invoke<number[]>('project_versions', { projectId }),
  buildIdentityPatch: (projectId: string, newAnchor: string) =>
    invoke<{ patch_id: number; report: ValidationReport }>('build_identity_patch', {
      projectId, newAnchor,
    }),
  approvePatch: (projectId: string, patchId: number) =>
    invoke<void>('approve_patch', { projectId, patchId }),
  rejectPatch: (projectId: string, patchId: number) =>
    invoke<void>('reject_patch', { projectId, patchId }),
  commitPatch: (projectId: string, patchId: number) =>
    invoke<CommitOutcome>('commit_patch', { projectId, patchId }),
  rollback: (projectId: string, toVersion: number) =>
    invoke<number>('rollback', { projectId, toVersion }),
  exportProject: (projectId: string) => invoke<string>('export_project', { projectId }),
  // agent turn runs on a background thread; results arrive via events
  agentSwapIdentity: (projectId: string, newAnchor: string) =>
    invoke<{ started: boolean }>('agent_swap_identity', { projectId, newAnchor }),
};
