import { invoke } from '@tauri-apps/api/core';
import type {
  CommitOutcome, ProjectRow, QueryIntent, Selection, TemplateMetadata, ValidationReport,
  VersionRow, ProviderSummary, PatchDetailResult,
} from './types';

// ---- commands exposed by src-tauri (thin bridge to AppServer) ----

export const api = {
  workspaceInfo: () => invoke<{ root: string; templates: number }>('workspace_info'),
  listTemplates: () => invoke<TemplateMetadata[]>('list_templates'),
  importSkill: (path: string) =>
    invoke<{ templates_imported: number; duplicates: number; total_warnings: number }>('import_skill', { path }),
  parseIntent: (text: string) => invoke<QueryIntent>('parse_intent', { text }),
  matchTemplates: (text: string, seed?: number | null) =>
    invoke<Selection>('match_templates', { text, seed: seed ?? null }),
  cloneProject: (templateId: string, title: string | null, seed?: number) =>
    invoke<ProjectRow>('clone_project', { templateId, title, seed: seed ?? 42 }),
  listProjects: () => invoke<ProjectRow[]>('list_projects'),
  projectVersions: (projectId: string) => invoke<VersionRow[]>('project_versions', { projectId }),
  patchDetail: (projectId: string, patchId: number) =>
    invoke<PatchDetailResult>('patch_detail', { projectId, patchId }),
  quickIdentitySwap: (projectId: string, newAnchor: string) =>
    invoke<{ patch_id: number; report: ValidationReport }>('quick_identity_swap', {
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
  exportProject: (projectId: string, outPath?: string | null) =>
    invoke<string>('export_project', { projectId, outPath: outPath ?? null }),
  persistenceWarnings: () => invoke<string[]>('persistence_warnings'),
  // providers (Settings)
  providersList: () => invoke<{ providers: ProviderSummary[]; active: string | null }>('providers_list'),
  providerSave: (id: string, name: string, baseUrl: string, model: string) =>
    invoke<void>('provider_save', { id, name, baseUrl, model }),
  providerDelete: (id: string) => invoke<void>('provider_delete', { id }),
  providerSetApiKey: (id: string, key: string) =>
    invoke<void>('provider_set_api_key', { id, key }),
  providerTest: (id: string) => invoke<{ ok: boolean; error?: string; reply?: string }>('provider_test', { id }),
  providerActivate: (id: string) => invoke<void>('provider_activate', { id }),
  agentProviderStatus: () => invoke<{ configured: boolean; active: string | null }>('agent_provider_status'),
  // lifecycle 2.0: thread Op queue (start / steer / cancel / result)
  agentStart: (threadId: string, projectId: string, text: string) =>
    invoke<{ started: boolean; thread_id: string; resumed: boolean }>('agent_start', { threadId, projectId, text }),
  agentSteer: (threadId: string, text: string) =>
    invoke<void>('agent_steer', { threadId, text }),
  agentCancel: (threadId: string) =>
    invoke<void>('agent_cancel', { threadId }),
  agentThreadResult: (threadId: string, projectId: string) =>
    invoke<{ lifecycle: string; result: PatchDetailResult['turn'] }>(
      'agent_thread_result', { threadId, projectId },
    ),
};
