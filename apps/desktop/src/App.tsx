import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import './App.css';
import { api } from './api';
import type {
  Candidate, CommitOutcome, PatchDetailResult, ProjectRow, ProviderSummary, QueryIntent,
  Selection, TemplateMetadata, ValidationReport, VersionRow,
} from './types';

type Page = 'library' | 'new' | 'projects' | 'agent' | 'settings';

export default function App() {
  const [page, setPage] = useState<Page>('library');
  const [ws, setWs] = useState<{ root: string; templates: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.workspaceInfo().then(setWs).catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">Storyboard<span>Studio</span></div>
        {(['library', 'new', 'projects', 'agent', 'settings'] as Page[]).map((p) => (
          <button
            key={p}
            className={`nav-btn ${page === p ? 'active' : ''}`}
            aria-current={page === p ? 'page' : undefined}
            onClick={() => setPage(p)}
          >
            {{ library: '模板库', new: '新建项目', projects: '项目', agent: 'Agent', settings: '设置' }[p]}
          </button>
        ))}
        <div className="spacer" />
        <div className="ws-info">
          {ws ? (
            <>
              workspace: <code>{ws.templates}</code> 套模板
              <br />
              <code style={{ fontSize: 9 }}>{ws.root}</code>
            </>
          ) : (
            'loading…'
          )}
        </div>
      </aside>
      <main className="main">
        {error && <div className="panel bad-text" role="alert">{error}</div>}
        {page === 'library' && <LibraryPage />}
        {page === 'new' && <NewProjectPage />}
        {page === 'projects' && <ProjectsPage />}
        {page === 'agent' && <AgentPage />}
        {page === 'settings' && <SettingsPage />}
      </main>
    </div>
  );
}

// ---------------- Library ----------------

function LibraryPage() {
  const [templates, setTemplates] = useState<TemplateMetadata[]>([]);
  const [family, setFamily] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [importPath, setImportPath] = useState('');
  const [importNote, setImportNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = () =>
    api.listTemplates().then(setTemplates).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  const doImport = () => {
    if (!importPath.trim()) return;
    setBusy(true);
    setImportNote(null);
    api
      .importSkill(importPath.trim())
      .then((s) => {
        setImportNote(`导入完成:${s.templates_imported} 套(${s.duplicates} 重复,${s.total_warnings} 警告)`);
        return refresh();
      })
      .catch((e) => setErr(`导入失败:${String(e)}`))
      .finally(() => setBusy(false));
  };

  const families = Array.from(new Set(templates.map((t) => t.scene_family).filter(Boolean))).sort();
  const shown = family ? templates.filter((t) => t.scene_family === family) : templates;

  return (
    <>
      <h1>模板库</h1>
      <div className="sub">
        只读原始模板(immutable originals,sha256 内容寻址)。角色统计已由 Importer 全卷重扫,旧索引计数仅存档。
      </div>
      {err && <div className="panel bad-text" role="alert">{err}</div>}
      {templates.length === 0 && !err && (
        <div className="panel">
          <h3>模板库为空</h3>
          <div className="muted">输入 skill 目录(含 references/template-index.json)或 .skill zip 路径导入:</div>
          <div className="row" style={{ marginTop: 8 }}>
            <input
              type="text"
              aria-label="skill 目录路径"
              style={{ flex: 1 }}
              placeholder="/path/to/novelai-author-storyboard.skill 或目录"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
            />
            <button className="primary" onClick={doImport} disabled={busy}>导入</button>
          </div>
        </div>
      )}
      {importNote && <div className="panel ok-text">{importNote}</div>}
      {templates.length > 0 && (
        <>
          <div className="chips" style={{ marginBottom: 14 }}>
            <button className={`chip ${family === '' ? 'hl' : ''}`} onClick={() => setFamily('')}>
              全部 ({templates.length})
            </button>
            {families.map((f) => (
              <button key={f} className={`chip ${family === f ? 'hl' : ''}`} onClick={() => setFamily(f)}>
                {f} ({templates.filter((t) => t.scene_family === f).length})
              </button>
            ))}
          </div>
          <div className="row" style={{ marginBottom: 12 }}>
            <input
              type="text"
              aria-label="追加导入 skill 路径"
              style={{ flex: 1 }}
              placeholder="追加导入:skill 目录或 .skill zip 路径…"
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
            />
            <button className="ghost" onClick={doImport} disabled={busy || !importPath.trim()}>导入</button>
          </div>
          <div className="cards">
            {shown.map((t) => (
              <div key={t.template_id} className="card">
                <div className="tid">{t.template_id}</div>
                <div className="title" title={t.title}>{t.title}</div>
                <div className="chips">
                  <span className="chip hl">{t.scene_family || '—'}</span>
                  <span className="chip">{t.panel_count} 格</span>
                  <span className="chip">{t.total_role_count} 角色</span>
                  <span className="chip">slots≤{t.max_simultaneous_slots}</span>
                  <span className="chip">{t.pace}</span>
                  <span className="chip">置信 {t.metadata_confidence.toFixed(2)}</span>
                </div>
                <div className="chips" style={{ marginTop: 6 }}>
                  {t.character_anchors.slice(0, 3).map((a) => (
                    <span key={a} className="chip">{a}</span>
                  ))}
                  {t.camera_profile.slice(0, 3).map((c) => (
                    <span key={c} className="chip">{c}</span>
                  ))}
                </div>
                {t.warnings.length > 0 && (
                  <div className="warn-line">⚠ {t.warnings.length} 条导入警告(元数据待人工复核)</div>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </>
  );
}

// ---------------- New Project (Match + Clone) ----------------

function NewProjectPage() {
  const [text, setText] = useState('夜间公园里 1女 被匿名男强暴');
  const [intent, setIntent] = useState<QueryIntent | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [busy, setBusy] = useState(false);
  const [title, setTitle] = useState('');
  const [created, setCreated] = useState<ProjectRow | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const runMatch = () => {
    setBusy(true);
    setCreated(null);
    setErr(null);
    api
      .matchTemplates(text, null)
      .then((s) => {
        setSelection(s);
        setIntent(null);
        api.parseIntent(text).then(setIntent).catch(() => {});
      })
      .catch((e) => {
        setSelection(null);
        setErr(String(e));
      })
      .finally(() => setBusy(false));
  };

  const doClone = (templateId: string) => {
    setBusy(true);
    setErr(null);
    api
      .cloneProject(templateId, title || null, 42)
      .then(setCreated)
      .catch((e) => {
        setCreated(null);
        setErr(String(e));
      })
      .finally(() => setBusy(false));
  };

  return (
    <>
      <h1>新建项目</h1>
      <div className="sub">程序先做确定性 Top-K 过滤与评分;AI 只在候选内做语义解释。默认 ONE REQUEST = ONE PRIMARY TEMPLATE。</div>
      {err && <div className="panel bad-text" role="alert">{err}</div>}
      <div className="panel">
        <div className="row">
          <input
            type="text"
            aria-label="需求描述"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="例:夜间公园里 1女 被匿名男强暴,80格"
            onKeyDown={(e) => e.key === 'Enter' && runMatch()}
          />
          <button className="primary" onClick={runMatch} disabled={busy}>
            匹配
          </button>
        </div>
        {intent && (
          <div className="breakdown" style={{ marginTop: 10 }}>
            <b>QueryIntent</b> {JSON.stringify(intent)}
          </div>
        )}
      </div>

      {selection && (
        <>
          <div className="panel">
            <h3>Primary Template — {selection.primary.template_id}(score {selection.primary.score.toFixed(3)},mode {selection.mode})</h3>
            <ScoreBreakdownView c={selection.primary} />
            {selection.needs_scene_adaptation && (
              <div className="warn-line">⚠ 相似度不足 0.55:将按 Nearest Template + Scene Adaptation 处理,不会假装完美匹配。</div>
            )}
            <div className="row" style={{ marginTop: 12 }}>
              <input type="text" aria-label="新项目标题" placeholder="新项目标题(默认继承模板)" value={title} onChange={(e) => setTitle(e.target.value)} />
              <button className="primary" disabled={busy} onClick={() => doClone(selection.primary.template_id)}>
                Deep Clone → v1
              </button>
            </div>
          </div>
          {selection.candidates.map((c) => (
            <div key={c.template_id} className="panel">
              <h3>候选 {c.template_id}(score {c.score.toFixed(3)})</h3>
              <ScoreBreakdownView c={c} />
              <button className="ghost" style={{ marginTop: 8 }} onClick={() => doClone(c.template_id)}>
                改用此模板克隆
              </button>
            </div>
          ))}
        </>
      )}

      {created && (
        <div className="panel">
          <h3>已创建项目</h3>
          <dl className="kv">
            <dt>Project ID</dt>
            <dd>{created.id}</dd>
            <dt>标题</dt>
            <dd>{created.title}</dd>
            <dt>版本</dt>
            <dd>v{created.current_version}</dd>
          </dl>
          <div className="muted" style={{ marginTop: 8 }}>到「Agent」页发起修改,或在「项目」页查看版本。</div>
        </div>
      )}
    </>
  );
}

function ScoreBreakdownView({ c }: { c: Candidate }) {
  const b = c.breakdown;
  const parts: [string, number, string][] = [
    ['场景', b.scene, '#7c6cf0'],
    ['结构', b.structure, '#4ecdc4'],
    ['人物', b.characters, '#e8b64c'],
    ['时间', b.time, '#e8636c'],
    ['节奏', b.pace, '#6fa8ff'],
    ['镜头/道具', b.camera_props, '#43d17c'],
  ];
  const total = parts.reduce((s, [, v]) => s + Math.max(v, 0), 0) || 1;
  return (
    <>
      <div className="score-bar">
        {parts.map(([name, v, color]) => (
          <div key={name} style={{ width: `${(Math.max(v, 0) / total) * 100}%`, background: color }} title={`${name} ${v}`} />
        ))}
      </div>
      <div className="breakdown">
        {parts.map(([name, v]) => (
          <span key={name} style={{ marginRight: 12 }}>
            {name} <b>{v.toFixed(0)}</b>
          </span>
        ))}
        <br />
        {c.title}
      </div>
    </>
  );
}

// ---------------- Projects ----------------

function ProjectsPage() {
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [versions, setVersions] = useState<Record<string, VersionRow[]>>({});
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [exportPaths, setExportPaths] = useState<Record<string, string>>({});

  const refresh = () => api.listProjects().then(setProjects).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  const loadVersions = (pid: string) => {
    api.projectVersions(pid)
      .then((v) => setVersions((m) => ({ ...m, [pid]: v })))
      .catch((e) => setErr(String(e)));
  };

  const doRollback = (pid: string, v: number, current: number) => {
    if (!window.confirm(`回滚到 v${v}?将以新版本 v${current + 1} 恢复该快照(历史不会被覆盖)。`)) return;
    setErr(null);
    api
      .rollback(pid, v)
      .then((nv) => {
        setNote(`已回滚:v${v} 的内容恢复为新版本 v${nv}`);
        refresh();
        loadVersions(pid);
      })
      .catch((e) => setErr(String(e)));
  };

  const doExport = (pid: string) => {
    setErr(null);
    const custom = (exportPaths[pid] ?? '').trim();
    api
      .exportProject(pid, custom || null)
      .then((p) => setNote(`导出成功:${p}`))
      .catch((e) => setErr(String(e)));
  };

  return (
    <>
      <h1>项目</h1>
      <div className="sub">每次 Commit 产生不可变版本快照;回滚以新版本恢复父快照内容(版本历史永不覆盖)。</div>
      {err && <div className="panel bad-text" role="alert">{err}</div>}
      {note && <div className="panel ok-text">{note}</div>}
      {projects.length === 0 && !err && <div className="empty">还没有项目 —— 到「新建项目」从模板克隆一个。</div>}
      {projects.map((p) => (
        <div key={p.id} className="panel">
          <div className="row">
            <div style={{ flex: 1 }}>
              <b>{p.title}</b> <span className="muted">from {p.source_template_id}</span>
            </div>
            <span className="chip hl">v{p.current_version}</span>
            <span className="chip">{p.status}</span>
          </div>
          <div className="row" style={{ marginTop: 10 }}>
            <button className="ghost" onClick={() => loadVersions(p.id)}>
              版本历史
            </button>
            <input
              type="text"
              aria-label={`导出路径 ${p.title}`}
              style={{ flex: 1 }}
              placeholder="导出路径(留空 = workspace 默认 exports/)"
              value={exportPaths[p.id] ?? ''}
              onChange={(e) => setExportPaths((m) => ({ ...m, [p.id]: e.target.value }))}
            />
            <button className="ghost" onClick={() => doExport(p.id)}>
              导出 JSON
            </button>
          </div>
          {(versions[p.id] ?? []).length > 0 && (
            <table className="versions" style={{ marginTop: 10 }}>
              <thead>
                <tr>
                  <th>版本</th>
                  <th>父版本</th>
                  <th>时间</th>
                  <th>Diff</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {versions[p.id].map((v) => (
                  <tr key={v.version}>
                    <td className="mono">v{v.version}</td>
                    <td className="mono">{v.parent_version != null ? `v${v.parent_version}` : '—'}</td>
                    <td className="muted">{new Date(v.created_at).toLocaleString()}</td>
                    <td>{v.has_diff ? '✓' : '—'}</td>
                    <td>
                      {v.version < p.current_version ? (
                        <button className="ghost" onClick={() => doRollback(p.id, v.version, p.current_version)}>
                          回滚到此版本
                        </button>
                      ) : (
                        <span className="muted">当前</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          <div className="muted mono" style={{ marginTop: 8, fontSize: 10 }}>{p.id}</div>
        </div>
      ))}
    </>
  );
}

// ---------------- Agent ----------------

function AgentPage() {
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [pid, setPid] = useState('');
  const [instruction, setInstruction] = useState('把角色换成 hoshino ai');
  const [steerText, setSteerText] = useState('');
  const [log, setLog] = useState<{ kind: string; text: string }[]>([]);
  const [streamText, setStreamText] = useState('');
  const [detail, setDetail] = useState<PatchDetailResult | null>(null);
  const [patchId, setPatchId] = useState<number | null>(null);
  const [outcome, setOutcome] = useState<CommitOutcome | null>(null);
  const [busy, setBusy] = useState(false);
  const [providerReady, setProviderReady] = useState<{ configured: boolean; active: string | null } | null>(null);
  const [quickAnchor, setQuickAnchor] = useState('');
  // per-project thread: projects never share conversation context
  const threadId = pid ? `ui-${pid}` : 'ui-none';

  useEffect(() => {
    api.listProjects().then((ps) => {
      setProjects(ps);
      if (ps.length > 0) setPid(ps[0].id);
    }).catch((e) => add('err', String(e)));
    api.agentProviderStatus().then(setProviderReady).catch(() => {});
  }, []);

  const add = (kind: string, text: string) => setLog((l) => [...l.slice(-200), { kind, text }]);

  useEffect(() => {
    // token-level stream + lifecycle telemetry (§17 bridge).
    // Wire contract: event `type` is the dotted name (message.delta, …) —
    // pinned by the Rust wire_names_match_type_names test.
    const un1 = listen<{ type: string; thread_id?: string; text?: string; tool?: string; error?: string }>(
      'sbx://agent-event',
      (e) => {
        const p = e.payload;
        if (p.thread_id && p.thread_id !== threadId) return; // other projects' threads
        switch (p.type) {
          case 'message.delta':
            if (p.text) setStreamText((t) => (t + p.text).slice(-4000));
            break;
          case 'agent.run.manifest.created':
            add('hl', 'run manifest persisted (F07)');
            break;
          case 'tool.started':
            add('hl', 'tool → ' + String(p.tool ?? ''));
            break;
          case 'tool.completed':
            add('ok', `tool ✓ ${String(p.tool ?? '')}`);
            break;
          case 'validator.completed':
            add('ok', 'validator completed');
            break;
          case 'approval.requested':
            add('hl', 'approval requested');
            break;
          case 'turn.cancelled':
            add('err', 'turn cancelled');
            break;
          case 'turn.failed':
            add('err', 'turn failed: ' + String(p.error ?? ''));
            break;
          case 'patch.commit.failed':
            add('err', String(p.error ?? 'commit/persistence issue'));
            break;
          case 'thread.idle':
            add('ok', 'thread idle — pulling result');
            api
              .agentThreadResult(threadId, pid)
              .then((r) => {
                const res = r.result;
                if (res?.kind === 'needs_approval' && res.detail) {
                  setDetail(res.detail);
                  if (res.patch_id != null) setPatchId(res.patch_id);
                  add('ok', `patch ${res.patch_id} proposed & validated (risk=${res.risk ?? '?'}${res.auto_approved ? ', auto-approved' : ''})`);
                } else if (res?.kind === 'failed') {
                  add('err', 'turn failed: ' + String(res.error ?? ''));
                } else if (res?.kind === 'completed') {
                  add('info', 'completed: ' + String(res.reply ?? '').slice(0, 120));
                }
                setBusy(false);
              })
              .catch((e) => {
                add('err', String(e));
                setBusy(false);
              });
            break;
        }
      },
    );
    return () => {
      un1.then((f) => f());
    };
  }, [threadId, pid]);

  const runAgent = () => {
    if (!pid) return;
    if (providerReady && !providerReady.configured) {
      add('err', '尚未配置 Provider —— 到「设置」添加并激活(或启用 mock 演示模式)');
      return;
    }
    setBusy(true);
    setLog([]);
    setStreamText('');
    setDetail(null);
    setPatchId(null);
    setOutcome(null);
    add('info', `turn dispatched · thread ${threadId}`);
    api.agentStart(threadId, pid, instruction).catch((e) => {
      add('err', String(e));
      setBusy(false);
    });
  };

  const doSteer = () => {
    if (!steerText.trim()) return;
    add('hl', `⤷ steer: ${steerText}`);
    api.agentSteer(threadId, steerText).catch((e) => add('err', String(e)));
    setSteerText('');
  };

  const doCancel = () => {
    add('err', 'cancel requested');
    api.agentCancel(threadId).catch((e) => add('err', String(e)));
  };

  const approveAndCommit = () => {
    if (patchId == null) return;
    setBusy(true);
    api
      .approvePatch(pid, patchId)
      .then(() => api.commitPatch(pid, patchId))
      .then((o) => {
        setOutcome(o);
        add('ok', `committed v${o.new_version} (parent v${o.parent_version}) · preservation ${(o.preservation_ratio * 100).toFixed(1)}%`);
      })
      .catch((e) => add('err', String(e)))
      .finally(() => setBusy(false));
  };

  const reject = () => {
    if (patchId == null) return;
    api.rejectPatch(pid, patchId).then(() => add('info', 'patch rejected')).catch((e) => add('err', String(e)));
  };

  const doQuickSwap = () => {
    if (!pid || !quickAnchor.trim()) return;
    setBusy(true);
    add('info', `quick identity swap → ${quickAnchor.trim()}`);
    api
      .quickIdentitySwap(pid, quickAnchor.trim())
      .then((r) => {
        setPatchId(r.patch_id);
        return api.patchDetail(pid, r.patch_id);
      })
      .then((d) => setDetail(d))
      .catch((e) => add('err', String(e)))
      .finally(() => setBusy(false));
  };

  const report = detail?.validation ?? null;

  return (
    <>
      <h1>Agent</h1>
      <div className="sub">
        Codex 形态的生命周期:Op 队列驱动、可取消、可插话(steer)、token 级流式、rollout 持久化、重启恢复会话。
        Agent 只提出 Semantic Patch;Gate 全 PASS 后由你批准 <b>同一条 patch</b>,Application Controller 提交。
      </div>
      {providerReady && !providerReady.configured && (
        <div className="panel warn-line" role="alert">
          ⚠ 未配置模型 Provider:Agent 无法调用真实模型。到「设置」添加 Provider(base URL / 模型 / API key,密钥存 OS 钥匙串)并激活,
          或激活 mock 演示模式。下方的「确定性换角色」不依赖 Provider,仍可用。
        </div>
      )}
      <div className="panel">
        <div className="row">
          <select
            aria-label="选择项目"
            value={pid}
            onChange={(e) => setPid(e.target.value)}
            style={{ background: 'var(--bg-3)', color: 'var(--text)', border: '1px solid var(--line)', borderRadius: 8, padding: '8px 10px' }}
          >
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.title} (v{p.current_version})
              </option>
            ))}
          </select>
          <input type="text" aria-label="Agent 指令" value={instruction} onChange={(e) => setInstruction(e.target.value)} />
          <button className="primary" onClick={runAgent} disabled={busy || !pid}>
            执行
          </button>
          <button className="danger" onClick={doCancel} disabled={!busy}>
            停止
          </button>
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          <input
            type="text"
            aria-label="插话内容"
            placeholder="turn 进行中插话(steer):中途补充/修改指令…"
            value={steerText}
            onChange={(e) => setSteerText(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && doSteer()}
          />
          <button className="ghost" onClick={doSteer} disabled={!busy}>
            插话
          </button>
        </div>
        <div className="row" style={{ marginTop: 8 }}>
          <input
            type="text"
            aria-label="快速换角色锚"
            placeholder="确定性换角色(不经模型):新角色锚,如 hoshino ai"
            value={quickAnchor}
            onChange={(e) => setQuickAnchor(e.target.value)}
          />
          <button className="ghost" onClick={doQuickSwap} disabled={busy || !pid || !quickAnchor.trim()}>
            快速换角色
          </button>
        </div>
      </div>

      {streamText && (
        <div className="panel">
          <h3>流式输出</h3>
          <div className="agent-log" style={{ maxHeight: 120 }} aria-live="polite">{streamText}</div>
        </div>
      )}

      {log.length > 0 && (
        <div className="agent-log">
          {log.map((l, i) => (
            <div key={i}>
              <span className="t">[{String(i + 1).padStart(2, '0')}]</span>{' '}
              <span className={l.kind === 'err' ? 'err' : l.kind === 'ok' ? 'ok' : 'hl'}>{l.text}</span>
            </div>
          ))}
        </div>
      )}

      {detail && (
        <PatchDetailView detail={detail} report={report} patchId={patchId} outcome={outcome} busy={busy}
          onApprove={approveAndCommit} onReject={reject} />
      )}

      {outcome && (
        <div className="panel">
          <h3>提交完成</h3>
          <dl className="kv">
            <dt>新版本</dt>
            <dd>v{outcome.new_version}(父版本 v{outcome.parent_version})</dd>
            <dt>保留率</dt>
            <dd>{(outcome.preservation_ratio * 100).toFixed(1)}%</dd>
            <dt>Diff</dt>
            <dd>{outcome.diff_path}</dd>
          </dl>
        </div>
      )}
    </>
  );
}

/** The approval surface: gates + the concrete diff the user is approving. */
function PatchDetailView({
  detail, report, patchId, outcome, busy, onApprove, onReject,
}: {
  detail: PatchDetailResult;
  report: ValidationReport | null;
  patchId: number | null;
  outcome: CommitOutcome | null;
  busy: boolean;
  onApprove: () => void;
  onReject: () => void;
}) {
  return (
    <div className="panel" style={{ marginTop: 14 }}>
      <h3>
        Patch #{detail.patch_id} — 基于 v{detail.base_version}({detail.status})
        {report && (
          <>
            {' '}· {report.passed ? <span className="ok-text">ALL PASS</span> : <span className="bad-text">FAILED</span>}
            {report.passed && ` · 保留率 ${(report.preservation_ratio * 100).toFixed(1)}%`}
          </>
        )}
      </h3>

      {report && (
        <div aria-live="polite">
          {reportGates(report).map((g) => (
            <div key={g.gate} className="gate">
              <span className={g.passed ? 'pass' : 'fail'}>{g.passed ? 'PASS' : 'FAIL'}</span>
              {g.gate}
              {g.failures.length > 0 && <span className="details">{g.failures.join(' · ')}</span>}
            </div>
          ))}
        </div>
      )}

      <div style={{ marginTop: 12 }}>
        <b>修改范围(Diff 预览)</b>
        <div className="breakdown" style={{ marginTop: 6 }}>
          操作:{detail.preview.applied.join('; ') || '—'}
          <br />
          触达面板:{detail.preview.touched_panels.length} 格
          {detail.preview.touched_panels.length > 0 &&
            `(P${detail.preview.touched_panels.slice(0, 12).join(', P')}${detail.preview.touched_panels.length > 12 ? ', …' : ''})`}
          <br />
          {detail.proposal.rationale.length > 0 && <>理由:{detail.proposal.rationale.join('; ')}</>}
        </div>
      </div>

      {report?.passed && patchId != null && !outcome && (
        <div className="row" style={{ marginTop: 14 }}>
          <button className="primary" onClick={onApprove} disabled={busy}>
            批准并提交
          </button>
          <button className="danger" onClick={onReject} disabled={busy}>
            拒绝
          </button>
        </div>
      )}
    </div>
  );
}

function reportGates(r: ValidationReport) {
  const gates = [r.schema, r.scope, r.anti_rewrite, r.identity_leak, r.scene_leak, r.reference_integrity, r.json_parse];
  if (r.clothing_chain) gates.push(r.clothing_chain);
  return gates;
}

// ---------------- Settings ----------------

function SettingsPage() {
  const [providers, setProviders] = useState<ProviderSummary[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [id, setId] = useState('');
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [testing, setTesting] = useState(false);

  const refresh = () =>
    api.providersList().then((r) => {
      setProviders(r.providers);
      setActive(r.active);
    }).catch((e) => setErr(String(e)));
  useEffect(() => {
    refresh();
  }, []);

  const flash = (msg: string) => {
    setNote(msg);
    setErr(null);
  };

  const save = () => {
    if (!id.trim() || !baseUrl.trim() || !model.trim()) {
      setErr('id / base URL / 模型 均必填');
      return;
    }
    api.providerSave(id.trim(), name.trim() || id.trim(), baseUrl.trim(), model.trim())
      .then(() => {
        flash(`已保存 ${id.trim()}`);
        refresh();
      })
      .catch((e) => setErr(String(e)));
  };

  const setKey = (pid: string) => {
    if (!apiKey.trim()) {
      setErr('API key 为空');
      return;
    }
    api.providerSetApiKey(pid, apiKey.trim())
      .then(() => {
        flash(`API key 已写入系统钥匙串 (${pid})`);
        setApiKey('');
        refresh();
      })
      .catch((e) => setErr(`钥匙串写入失败:${String(e)}`));
  };

  const test = (pid: string) => {
    setTesting(true);
    api.providerTest(pid)
      .then((r) => (r.ok ? flash(`连通 ✓ ${String(r.reply ?? '')}`) : setErr(`连通失败:${String(r.error ?? '')}`)))
      .catch((e) => setErr(String(e)))
      .finally(() => setTesting(false));
  };

  const activate = (pid: string) => {
    api.providerActivate(pid)
      .then(() => {
        flash(`已激活 ${pid}`);
        refresh();
      })
      .catch((e) => setErr(String(e)));
  };

  const remove = (pid: string) => {
    if (!window.confirm(`删除 provider ${pid}?(钥匙串中的 key 一并清除)`)) return;
    api.providerDelete(pid).then(() => {
      flash(`已删除 ${pid}`);
      refresh();
    }).catch((e) => setErr(String(e)));
  };

  return (
    <>
      <h1>设置</h1>
      <div className="sub">
        Provider 配置存 SQLite;<b>API key 只进 OS 钥匙串</b>(Windows 凭据管理器 / macOS Keychain / Linux Secret Service),数据库仅存引用。
      </div>
      {err && <div className="panel bad-text" role="alert">{err}</div>}
      {note && <div className="panel ok-text">{note}</div>}

      <div className="panel">
        <h3>模型 Provider(OpenAI 兼容 /chat/completions)</h3>
        <div className="two-col">
          <div>
            <div className="row"><input type="text" aria-label="provider id" placeholder="id(如 glm / openai / local)" value={id} onChange={(e) => setId(e.target.value)} /></div>
            <div className="row" style={{ marginTop: 6 }}><input type="text" aria-label="显示名" placeholder="显示名" value={name} onChange={(e) => setName(e.target.value)} /></div>
            <div className="row" style={{ marginTop: 6 }}><input type="text" aria-label="base URL" placeholder="https://api.example.com/v1" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} /></div>
            <div className="row" style={{ marginTop: 6 }}><input type="text" aria-label="模型名" placeholder="模型名(如 gpt-4o / glm-4.7)" value={model} onChange={(e) => setModel(e.target.value)} /></div>
            <div className="row" style={{ marginTop: 10 }}>
              <button className="primary" onClick={save}>保存 Provider</button>
            </div>
          </div>
          <div>
            <div className="row">
              <input type="password" aria-label="API key" placeholder="API key(写入系统钥匙串,不落库)" value={apiKey} onChange={(e) => setApiKey(e.target.value)} />
            </div>
            <div className="muted" style={{ marginTop: 6 }}>保存 provider 后,为其写入 key → 测试连通 → 激活。</div>
            <div className="row" style={{ marginTop: 10 }}>
              <button className="ghost" onClick={() => activate('mock')}>启用 mock 演示模式</button>
            </div>
          </div>
        </div>
      </div>

      {providers.length > 0 && (
        <div className="panel">
          <h3>已配置(当前激活:{active ?? '无'})</h3>
          <table className="versions">
            <thead>
              <tr><th>ID</th><th>模型</th><th>Key</th><th>状态</th><th>操作</th></tr>
            </thead>
            <tbody>
              {providers.map((p) => (
                <tr key={p.id}>
                  <td className="mono">{p.id}</td>
                  <td>{p.model}</td>
                  <td>{p.has_api_key ? '✓ 已存' : '—'}</td>
                  <td>{p.active ? <span className="ok-text">激活</span> : <span className="muted">未激活</span>}</td>
                  <td>
                    <button className="chip" onClick={() => setKey(p.id)}>写 key</button>{' '}
                    <button className="chip" disabled={testing} onClick={() => test(p.id)}>测试</button>{' '}
                    <button className="chip" onClick={() => activate(p.id)} disabled={!p.has_api_key}>激活</button>{' '}
                    <button className="chip" onClick={() => remove(p.id)}>删除</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className="two-col">
        <div className="panel">
          <h3>默认 Matcher 权重(plan Table 8)</h3>
          <dl className="kv">
            <dt>场景/地点</dt><dd>35</dd>
            <dt>结构/主题</dt><dd>20</dd>
            <dt>人物数量</dt><dd>15</dd>
            <dt>时间/环境</dt><dd>10</dd>
            <dt>Pace/格数</dt><dd>10</dd>
            <dt>镜头/道具</dt><dd>10</dd>
          </dl>
        </div>
        <div className="panel">
          <h3>Gate 阈值 / Agent Profile</h3>
          <dl className="kv">
            <dt>身份替换保留率</dt><dd>≥ 0.90</dd>
            <dt>场景替换保留率</dt><dd>≥ 0.80</dd>
            <dt>Production 工具</dt><dd>search/read×4 + propose/preview/validate</dd>
            <dt>无 shell/write</dt><dd className="ok-text">commit 不在工具表(F02)</dd>
          </dl>
        </div>
      </div>
    </>
  );
}
