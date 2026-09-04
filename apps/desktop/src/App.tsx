import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import './App.css';
import { api } from './api';
import type {
  Candidate, CommitOutcome, ProjectRow, QueryIntent, Selection, TemplateMetadata, ValidationReport,
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
          <button key={p} className={`nav-btn ${page === p ? 'active' : ''}`} onClick={() => setPage(p)}>
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
        {error && <div className="panel bad-text">{error}</div>}
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
  useEffect(() => {
    api.listTemplates().then(setTemplates).catch(() => setTemplates([]));
  }, []);
  const families = Array.from(new Set(templates.map((t) => t.scene_family).filter(Boolean))).sort();
  const shown = family ? templates.filter((t) => t.scene_family === family) : templates;

  return (
    <>
      <h1>模板库</h1>
      <div className="sub">
        只读原始模板(immutable originals,sha256 内容寻址)。角色统计已由 Importer 全卷重扫,旧索引计数仅存档。
      </div>
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

  const runMatch = () => {
    setBusy(true);
    setCreated(null);
    api
      .matchTemplates(text, null)
      .then((s) => {
        setSelection(s);
        setIntent(null);
        api.parseIntent(text).then(setIntent).catch(() => {});
      })
      .catch(() => setSelection(null))
      .finally(() => setBusy(false));
  };

  const doClone = (templateId: string) => {
    setBusy(true);
    api
      .cloneProject(templateId, title || null, 42)
      .then(setCreated)
      .catch(() => setCreated(null))
      .finally(() => setBusy(false));
  };

  return (
    <>
      <h1>新建项目</h1>
      <div className="sub">程序先做确定性 Top-K 过滤与评分;AI 只在候选内做语义解释。默认 ONE REQUEST = ONE PRIMARY TEMPLATE。</div>
      <div className="panel">
        <div className="row">
          <input
            type="text"
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
              <input type="text" placeholder="新项目标题(默认继承模板)" value={title} onChange={(e) => setTitle(e.target.value)} />
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
  const [versions, setVersions] = useState<Record<string, number[]>>({});
  const refresh = () => api.listProjects().then(setProjects).catch(() => setProjects([]));
  useEffect(() => {
    refresh();
  }, []);

  const loadVersions = (pid: string) => {
    api.projectVersions(pid).then((v) => setVersions((m) => ({ ...m, [pid]: v }))).catch(() => {});
  };
  const doRollback = (pid: string, v: number) => {
    api.rollback(pid, v).then(refresh).catch(() => {});
  };
  const doExport = (pid: string) => {
    api.exportProject(pid).catch(() => {});
  };

  return (
    <>
      <h1>项目</h1>
      <div className="sub">每次 Commit 产生不可变版本快照;回滚以新版本恢复父快照内容(版本历史永不覆盖)。</div>
      {projects.length === 0 && <div className="empty">还没有项目 —— 到「新建项目」从模板克隆一个。</div>}
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
            <button className="ghost" onClick={() => doExport(p.id)}>
              导出 JSON
            </button>
          </div>
          {(versions[p.id] ?? []).length > 0 && (
            <table className="versions" style={{ marginTop: 10 }}>
              <thead>
                <tr>
                  <th>版本</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {versions[p.id].map((v) => (
                  <tr key={v}>
                    <td className="mono">v{v}</td>
                    <td>
                      {v < p.current_version ? (
                        <button className="ghost" onClick={() => doRollback(p.id, v)}>
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
  const [log, setLog] = useState<{ kind: string; text: string }[]>([]);
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [patchId, setPatchId] = useState<number | null>(null);
  const [outcome, setOutcome] = useState<CommitOutcome | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api.listProjects().then((ps) => {
      setProjects(ps);
      if (ps.length > 0) setPid(ps[0].id);
    }).catch(() => {});
  }, []);

  const add = (kind: string, text: string) => setLog((l) => [...l, { kind, text }]);

  // background turn telemetry: per-event stream + final result
  useEffect(() => {
    const un1 = listen<{ type: string }>('sbx://agent-event', (e) => {
      const t = e.payload.type;
      if (t === 'agent.run.manifest.created') add('hl', 'run manifest persisted (F07)');
      else if (t === 'tool.started' || t === 'tool.completed') add('hl', t);
      else if (t === 'validator.completed') add('ok', 'validator completed');
      else if (t === 'approval.requested') add('hl', 'approval requested');
    });
    const un2 = listen<{ status: string; run_id: string; patch_id?: number; report?: ValidationReport; error?: string }>(
      'sbx://agent-turn-result',
      (e) => {
        setBusy(false);
        const r = e.payload;
        if (r.error) {
          add('err', r.error);
          return;
        }
        add('ok', `turn ${r.status} · run ${r.run_id.slice(0, 14)}…`);
        if (r.report) setReport(r.report);
        if (r.patch_id != null) {
          setPatchId(r.patch_id);
          add('ok', `patch ${r.patch_id} proposed & validated`);
        }
      },
    );
    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
    };
  }, []);

  const runAgent = () => {
    setBusy(true);
    setLog([]);
    setReport(null);
    setPatchId(null);
    setOutcome(null);
    add('info', `turn dispatched on background thread · project ${pid.slice(0, 8)}…`);
    api
      .agentSwapIdentity(pid, extractAnchor(instruction))
      .catch((e) => {
        add('err', String(e));
        setBusy(false);
      });
  };

  const approveAndCommit = () => {
    if (patchId == null) return;
    setBusy(true);
    api
      .approvePatch(pid, patchId)
      .then(() => api.commitPatch(pid, patchId))
      .then((o) => {
        setOutcome(o);
        add('ok', `committed v${o.new_version} (parent v${o.parent_version}) · preservation ${o.preservation_ratio.toFixed(3)}`);
      })
      .catch((e) => add('err', String(e)))
      .finally(() => setBusy(false));
  };

  const reject = () => {
    if (patchId == null) return;
    api.rejectPatch(pid, patchId)
      .then(() => add('info', 'patch rejected'))
      .catch((e) => add('err', String(e)));
  };

  return (
    <>
      <h1>Agent</h1>
      <div className="sub">
        Agent 只读检索并提出 Semantic Patch;七道确定性 Gate 全部 PASS 后,由你在本页批准,Application Controller 才会提交(commit 工具不存在于 Agent 工具表)。
      </div>
      <div className="panel">
        <div className="row">
          <select
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
          <input type="text" value={instruction} onChange={(e) => setInstruction(e.target.value)} />
          <button className="primary" onClick={runAgent} disabled={busy || !pid}>
            执行
          </button>
        </div>
      </div>

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

      {report && (
        <div className="panel" style={{ marginTop: 14 }}>
          <h3>
            Validation — {report.passed ? <span className="ok-text">ALL PASS</span> : <span className="bad-text">FAILED</span>}
            {report.passed && ` · 保留率 ${(report.preservation_ratio * 100).toFixed(1)}%`}
          </h3>
          {reportGates(report).map((g) => (
            <div key={g.gate} className="gate">
              <span className={g.passed ? 'pass' : 'fail'}>{g.passed ? 'PASS' : 'FAIL'}</span>
              {g.gate}
              {g.failures.length > 0 && <span className="details">{g.failures.join(' · ')}</span>}
            </div>
          ))}
          {report.passed && patchId != null && !outcome && (
            <div className="row" style={{ marginTop: 14 }}>
              <button className="primary" onClick={approveAndCommit} disabled={busy}>
                批准并提交
              </button>
              <button className="danger" onClick={reject} disabled={busy}>
                拒绝
              </button>
            </div>
          )}
        </div>
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

function reportGates(r: ValidationReport) {
  return [r.schema, r.scope, r.anti_rewrite, r.identity_leak, r.scene_leak, r.reference_integrity, r.json_parse];
}

function extractAnchor(instruction: string): string {
  // "把角色换成 X" / "换成 X" / "X" — take the tail after 换成, else last token
  const m = instruction.match(/换成\s*(.+)$/);
  if (m) return m[1].trim();
  return instruction.trim().split(/\s+/).pop() ?? 'hoshino ai';
}

// ---------------- Settings ----------------

function SettingsPage() {
  return (
    <>
      <h1>设置</h1>
      <div className="sub">Provider / Matcher 权重 / 审批策略 / Prompt Preset(本版为只读展示,配置写入 SQLite settings 表)。</div>
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
          <h3>Gate 阈值</h3>
          <dl className="kv">
            <dt>身份替换保留率</dt><dd>≥ 0.90</dd>
            <dt>场景替换保留率</dt><dd>≥ 0.80</dd>
            <dt>Top-K</dt><dd>3(dominance 0.15)</dd>
          </dl>
        </div>
        <div className="panel">
          <h3>Agent Profile</h3>
          <dl className="kv">
            <dt>Production 工具</dt>
            <dd>search/read×4 + propose/preview/validate</dd>
            <dt>无 shell/write</dt>
            <dd className="ok-text">commit 不在工具表(F02)</dd>
          </dl>
        </div>
        <div className="panel">
          <h3>Prompt Presets</h3>
          <div className="muted" style={{ lineHeight: 1.8 }}>
            prompts/v1:CORE_CONTRACT · INTENT_PARSER · TEMPLATE_MATCH · CHARACTER_REPLACE ·
            PATCH_GENERATOR · FAILURE_RECOVERY(每次 Run 记录 Manifest + 契约哈希)
          </div>
        </div>
      </div>
    </>
  );
}
