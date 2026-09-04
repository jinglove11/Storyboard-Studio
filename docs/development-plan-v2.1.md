NovelAI Storyboard Studio
桌面端完整开发计划书（Codex-derived Agent Runtime 版）
版本：v2.1 Architecture Freeze
日期：2026-09-03
定位：本地优先的模板驱动 Storyboard JSON 工作台
核心原则
Template first. Clone before generate.
AI proposes. Program validates. Commit only after PASS + approval.
# 1. 执行摘要
本项目拟将现有 Reference Template Cloner Skill 产品化为桌面应用。现有 Skill 已具有 30 套只读原始模板、模板索引、模板选择规则、Deep Clone、Character Replacement、Scene Mapping、最小修改预算以及 Anti-Rewrite / Identity Leak / Scene Leak 三道 Gate。桌面端的工作不是重新设计这些业务逻辑，而是把它们从“提示词约束”迁移为可测试、可版本化、可审计的程序模块。
Agent 基座采用 Codex-derived Rust Runtime 思路：重点借鉴/复用 Codex 的 thread/turn 生命周期、协议层、app-server 结构、ModelProvider 抽象、approval/sandbox、通用 apply-patch 和事件流，但不 Fork 整个 Codex 产品。v1.0 先采用内部 Typed Tool Registry；MCP 适配器延后到 v1.1。Storyboard 领域写操作继续由自研 Semantic Patch 与 Validator 控制，Production Agent 不拥有任意 shell/write_file，也不拥有 commit_storyboard_patch 权限。
## 1.1 v2.1 Architecture Freeze 决策
本版只锁定架构边界，不新增产品功能。以下 7 项是进入正式开发前必须执行的冻结决定。
# 2. 当前资产与迁移基线
## 2.1 当前 Skill 已具备能力
30 套只读 template_001.json ~ template_030.json；实际模板为唯一事实源。
template-index.json：scene_aliases、模板元数据、场景/人物/节奏/镜头等索引。
template-selection.md：同场景族过滤、Top-K、加权随机、Primary Template、A-D 模式。
template-mutation.md：Deep Clone、KEEP/REPLACE/PATCH/DELETE、Character Replacement、Scene Mapping、Change Budget。
三道 Gate：Anti-Rewrite、Identity Leak、Scene Leak。
schemaVersion 2 项目 JSON 兼容约束；模板格数默认继承。
## 2.2 Phase 0 必须先修的数据问题
当前 template-index 不能直接作为桌面端数据库导入真源。实测 30 个索引条目中有 28 个出现 character_count 与 female_character_count + male_character_count 不一致。桌面端 Importer 必须重新扫描整套模板并生成角色统计，不能把某一格 customCharacters 槽位数当作整套人物数。
# 3. 产品范围
## 3.1 MVP 必做
模板导入与索引
模板库浏览/筛选
规则 Top-K 匹配与 score breakdown
Primary Template 选择
Deep Clone
角色替换
场景替换
Semantic Patch
Diff 预览
四类 Validator（Schema/Scope/Leak/Rewrite）
项目版本与回滚
JSON 导入/导出
多模型 Provider 设置
Agent 对话与 Tool Call 日志
## 3.2 MVP 明确不做
云账号与多人协作
云同步
在线素材市场
复杂图片资产管理
NovelAI API 批量生图
跨设备状态同步
向量数据库（30~100 套阶段先不需要）
自动跨模板拼接
Agent 任意 shell 写项目文件
# 4. 总体技术架构
推荐技术栈：Tauri 2 + React + TypeScript 负责桌面壳与 UI；Rust 负责 Storyboard Core、Codex-derived Agent Runtime、文件/数据库/Validator；SQLite 负责模板、角色、项目、版本与审计索引。
# 5. Codex 源码借鉴/复用策略
截至 2026-09-03，OpenAI Codex 仓库采用 Apache-2.0。codex-rs 已拆分 core、protocol、app-server、app-server-client、model-provider、model-provider-info、apply-patch、sandboxing、skills、MCP 等多个 crate；其 app-server-client 也已提供 in-process runtime 的启动、请求/事件传输和生命周期管理。计划采用“选择性借鉴/抽取”而非完整 Fork。
OpenAI Codex 官方源码仓库：https://github.com/openai/codex
## 5.1 建议重点借鉴
## 5.2 不直接照搬
Codex TUI / CLI 产品界面
OpenAI 登录流程
Cloud Tasks/Feedback/Analytics 等与本产品无关模块
面向代码仓库的任意 shell 修改作为默认能力
通用文本 apply_patch 直接修改 Storyboard JSON
# 6. Agent Runtime 设计
## 6.1 两个 Agent Profile
## 6.2 Agent Loop
1. Start/Resume Thread
2. Parse user intent
3. Call read-only tools (max N)
4. Select Primary Template
5. Produce structured PatchProposal
6. Run deterministic validators
7. If FAIL: return structured errors to agent, max 2 retries
8. If PASS: generate Diff + approval decision
9. Application Controller verifies approval + preconditions
10. commit_storyboard_patch atomically
11. Create project version + audit event
重点：Agent 不直接“输出一份完整新 JSON 覆盖原项目”。完整 JSON 只由 Clone Engine 或 Patch Engine 在已知基线之上确定性产生。
## 6.3 Agent Run Manifest
每次 Agent Turn/Run 都必须固化运行环境，避免出现“同一个模型昨天能用、今天结果不同却无法定位原因”的不可审计状态。Manifest 与 Thread/Event 分离保存，作为可复现实验与问题追踪的最小证据。
AgentRunManifest {
  run_id: String,
  provider_id: String,
  model: String,
  prompt_preset_version: String,
  core_contract_hash: String,
  tool_registry_version: String,
  primary_template_revision: String,
  base_project_version: String,
  sampling: Json,
  created_at: Timestamp
}
任何 PatchProposal、Validator Report 与 Project Version 都应能反查对应 run_id。API Key 等秘密信息不得进入 Manifest。
# 7. Prompt / Skill 预设体系
不采用单一超长 system prompt。将稳定约束、任务指令、错误修复拆开版本化，每次 Turn 根据任务动态组合。
## 7.1 CORE_CONTRACT 建议硬约束
C01 Primary Template is the source of truth.
C02 One request = one Primary Template by default.
C03 Clone before patch; patch before generation.
C04 Agent must not directly overwrite project JSON.
C05 Every write must be represented as a typed patch operation.
C06 Untouched panels must remain byte/semantic stable where possible.
C07 Validator failures may only be fixed inside the reported scope.
C08 Template originals are read-only.
C09 JSON schema compatibility is mandatory.
C10 Tool results override model guesses.
C11 Production Agent must never receive commit_storyboard_patch.
C12 Every PatchOperation must carry explicit preconditions when mutating existing content.
# 8. 端到端执行流
## 8.1 规则匹配与 AI 的职责边界
模板匹配不应该让 LLM 从几百个模板中自由搜索。程序先使用 scene_family、exact_scene、character count、time、pace、keywords 等字段做确定性过滤和评分，得到 Top-K；AI 仅在 Top-K 内处理语义模糊、别名或用户自然语言解释。
QueryIntent
  ↓
SQL / Rule filter
  ↓
Top-K (3~10)
  ↓
optional LLM rerank
  ↓
PrimaryTemplateSelection
# 9. Template Matcher 详细设计
## 9.1 QueryIntent
struct QueryIntent {
  scene_family: Option<String>,
  exact_scene: Option<String>,
  time: Option<String>,
  character_count: Option<u32>,
  character_roles: Vec<String>,
  narrative_tags: Vec<String>,
  pace_hint: Option<String>,
  desired_panel_count: Option<u32>,
  props: Vec<String>,
  camera_hints: Vec<String>,
  seed: Option<u64>,
}
## 9.2 第一版评分
权重必须配置化并可在设置页调整。模板数小于约 100 时，规则评分足够；达到几百套后再增加 FTS5/Embedding 混合检索。
## 9.3 Top-K 与随机策略
Top-K default = 3
if top1.score - top2.score >= 0.15:
    choose top1
else:
    weighted_random(top_k, weight = score)
桌面端应把每个候选的 score breakdown 可视化，避免“AI 说这个模板更像但用户无法判断”。
# 10. Template Library 与 Importer
## 10.1 导入流程
Drop JSON / Select Folder
→ Parse + Schema fingerprint
→ Content hash / duplicate detection
→ Full-panel scan
→ Auto metadata extraction
→ Confidence scoring
→ User confirmation
→ Copy to immutable original store
→ Insert SQLite metadata
## 10.2 原始模板不可变
模板文件按 content hash 存储；导入后标记 immutable。
项目克隆使用模板快照 ID，不写回 originals。
模板更新以新 revision 导入，不能原地覆盖旧 revision。
任何修复只更新 metadata/index，不修改作者原 JSON。
## 10.3 自动索引字段
# 11. Clone Engine
Clone Engine 完全确定性，不依赖 LLM。它负责读取 immutable template、deep copy、生成项目 UUID/Panel UUID、复制 paramsOverride/imageSize/customCharacters 结构、建立 source_template_revision，并创建初始 v1。
clone_template(template_revision_id) -> ProjectDraft

Guarantees:
- schema fields preserved
- panel ordering preserved
- original prompt text preserved
- template file untouched
- project version v1 created atomically
# 12. Storyboard Semantic Patch
这是项目与 Codex 通用 apply_patch 的最大区别。Storyboard JSON 不允许 Agent 直接用文本 diff 修改；Agent 必须产生领域级操作，程序再转换为具体 JSON 修改。为避免命名与权限歧义，Storyboard 领域接口统一使用 propose_storyboard_patch / preview_storyboard_patch / validate_storyboard_patch / commit_storyboard_patch；Codex apply_patch 仅保留给 Developer Mode 的代码与普通文件。
## 12.1 PatchProposal Schema
PatchProposal {
  base_project_version: String,
  primary_template_id: String,
  intent_hash: String,
  operations: Vec<PatchOperation>,
  touched_panels: Vec<u32>,
  expected_preservation_ratio: f32,
  rationale: Vec<String>
}

PatchOperationCommon {
  operation_id: String,
  panel_id: Option<String>,
  anchor: Option<String>,
  expected_old: Option<String>,
  expected_old_hash: Option<String>,
  expected_project_version: String
}
## 12.2 第一版 Operation
MVP 不提供 ArbitraryJsonReplace；若未来增加，也必须进入高级模式并要求人工审批。
## 12.3 Patch 前置条件与陈旧写入保护
所有会修改既有内容的 PatchOperation 必须带前置条件。Patch Engine 在内存副本上执行前，先验证 expected_project_version；对定位到既有文本/块的操作还必须验证 expected_old 或 expected_old_hash。任何不一致均返回 STALE_PATCH / PRECONDITION_FAILED，不允许“尽量匹配”后继续写入。
if current_project_version != expected_project_version:
    reject(STALE_PATCH)

if hash(current_anchor_text) != expected_old_hash:
    reject(PRECONDITION_FAILED)

apply_to_in_memory_draft_only()
## 12.4 Commit 边界
Production Agent 的工具表中不存在 commit_storyboard_patch。Agent 只能 propose / preview / validate。Gate 全部 PASS 后，由 Approval Policy 得到授权，再由 Application Controller 调用 commit_storyboard_patch。Commit 采用临时文件写入、重新解析、Schema 校验、原子 rename，并创建新的 Project Version；旧版本永不原地覆盖。
# 13. Validator 与 Gate
## 13.1 保留率
保留率必须按“目标块/非目标块”区分，而不是简单全文字符相似度。角色替换模式重点检查非身份块；场景替换模式允许场景块变化，但镜头、Panel 顺序、与场景无关的内容保持稳定。阈值配置化，现有 Skill 的 90%/80% 可作为初始基线而非永恒常量。
# 14. Model Provider 层
Provider 层以 Codex model-provider/model-provider-info 的抽象为主要参考。当前 Codex 已支持内置与用户定义 provider 信息，包含 base_url、认证、headers、query params、retry、wire API 等配置。桌面端应保持 Provider 与业务逻辑解耦。
trait StoryboardModelProvider {
  fn id(&self) -> &str;
  fn capabilities(&self) -> ProviderCapabilities;
  async fn start_turn(&self, req: TurnRequest) -> Result<Stream<TurnEvent>>;
}
不要把模型名写死在业务代码；Matcher、Modifier、Reviewer 可分别选择不同模型。
# 15. Storyboard Typed Tool Registry（v1.0）
v1.0 只实现内部 Typed Tool Registry，不依赖 MCP。Production Agent 的注册表只能包含只读工具与 propose/preview/validate；commit_storyboard_patch、rollback_version、export_json 属于 Application Controller / 用户动作。v1.1 再增加 MCP Server/Client Adapter，并继续复用同一 Tool Schema 与 Permission Policy。
## 15.1 MCP 延后策略
MCP 不参与 v1.0 的主链路与退出条件。v1.0 的目标是先把 Tool Schema、权限边界、Patch/Gate/Commit 链路做稳定；v1.1 再用 Adapter 把同一内部 Tool Registry 暴露给 MCP Server/Client。这样可以避免在核心领域模型尚未稳定时，同时承担外部协议、连接生命周期和第三方权限管理复杂度。
# 16. Approval / Sandbox / 权限模型
参考 Codex 当前 approval/sandbox 的分层思想，但针对 Storyboard 进一步收紧。生产模式默认 read-only + semantic-patch。Codex 协议目前支持 approvalPolicy、sandboxPolicy 等 turn 级覆盖，因此本项目也采用“默认策略 + 单 Turn 覆盖”的配置模型。
# 17. App Server / 前后端协议
优先采用“单进程 in-process Rust runtime + typed channel”的架构，Tauri command 只负责桥接 React 与 Rust。Codex app-server-client 已证明 in-process 客户端可以复用 app-server 生命周期并通过 typed channels 传输请求/事件，因此本项目也应避免为桌面版额外启动本地 HTTP 服务。
React
  ⇅ Tauri IPC
Rust UI Facade
  ⇅ typed request/event channels
Storyboard App Server
  ├─ Thread Manager
  ├─ Agent Runtime
  ├─ Domain Core
  └─ Storage
## 17.1 事件类型
thread.started / thread.resumed
turn.started / turn.completed / turn.failed
tool.started / tool.completed
template.match.updated
patch.proposed
validator.completed
approval.requested / approval.resolved
patch.commit.requested / patch.commit.completed / patch.commit.failed
project.version.created
agent.run.manifest.created
export.completed
# 18. SQLite 数据模型
API Key 不明文存 SQLite；通过系统 Keychain/Secret Store 保存，数据库只记录 secret reference。
# 19. 本地文件结构
workspace/
├─ templates/
│  └─ originals/<sha256>.json
├─ projects/<project_id>/
│  ├─ versions/v0001/project.json
│  ├─ versions/v0002/project.json
│  ├─ diffs/v0001-v0002.json
│  └─ exports/
├─ prompts/
│  └─ <preset_version>/...
├─ runs/<run_id>/manifest.json
├─ database/storyboard.db
└─ logs/
# 20. UI / UX 规划
## 20.1 Agent 面板交互
User: “把角色换成 X，场景改成夜间公园”

Agent Activity:
✓ search_templates
✓ read_template T010
✓ read_project
→ Patch proposed: 47 operations

Validation:
Schema       PASS
Scope        PASS
Identity     PASS
Scene Leak   PASS
Rewrite      91.6% preserved

[View Diff]  [Commit]  [Reject]
# 21. 项目状态机
Draft
  → Matched
  → Cloned
  → PatchProposed
  → Validating
      ├─ FAIL → PatchRejected / Retry
      └─ PASS → AwaitingApproval / AutoApproved
  → CommitRequested
  → Committed
  → Versioned
  → Exported
状态变化必须由 Rust Core 驱动，React 只渲染状态，不自行推断。
# 22. 安全、隐私与可靠性
本地模板与项目默认不上传；只有发送给模型的必要上下文离开本机。
提供“发送上下文预览”：用户可看到本次会发送哪些 Panel/元数据。
Provider API Key 使用 OS Keychain；日志默认不记录密钥。
Production Agent 无任意 shell/write_file，也不注册 commit_storyboard_patch。
所有 Commit 原子化：写临时文件 → parse/validate → rename。
项目每次写入前保存 parent version，可一键 rollback。
模板 originals 只读并做 sha256 校验。
外部 MCP server 默认关闭；启用时逐 Server 显示权限。
# 23. 性能目标
# 24. 测试策略
## 24.1 单元测试
Query normalization
Top-K scoring
weighted random deterministic seed
character stats extraction
Deep Clone immutability
Patch operation application
Patch precondition / stale-write rejection
Schema validation
Leak scanners
preservation ratio
snapshot version/rollback
Production tool registry excludes commit
Agent Run Manifest serialization
## 24.2 Golden Cases
## 24.3 Agent 评测
模板选择准确率（人工 gold set）
Patch 越界率
Validator 首次通过率
平均重试次数
非目标文本保留率
Identity/Scene Leak 漏检率
不同 Provider 输出一致性
Run Manifest 完整率（目标 100%）
同一基线/同一配置下的可复现实验可追踪率
# 25. 开发阶段与里程碑
按 1 名主开发 + AI 辅助估算，建议 10 周完成可长期使用的 v1.0。Phase 0.5 Codex Integration Spike 必须在正式 Agent Runtime 开发前完成，用 2~3 个工作日验证“哪些 Codex crate 可稳定嵌入、哪些只借设计”。若只做最小 MVP，可压缩至 6~7 周，但不建议跳过 Phase 0/0.5、Patch/Validator 和版本系统。
# 26. 详细开发任务清单
# 27. v1.0 验收标准
# 28. v1.1 / v2.0 扩展路线
## 28.1 模板规模扩大
100+：SQLite FTS5、模板场景族统计面板。
300+：BM25 + embedding hybrid retrieval；先规则 hard filter 再向量 rerank。
1000+：离线 embedding cache、批量 metadata pipeline、模板质量评分。
## 28.2 Agent 能力扩展
MCP Server/Client Adapter：把 v1.0 内部 Typed Tool Registry 暴露为标准 MCP 能力
多 Agent Reviewer（Modifier 与 Reviewer 分离上下文）
Auto Review 仅审批低风险 Patch，不审批 shell/外部写操作
可安装 Storyboard Skill Pack / Prompt Pack
MCP 插件：NovelAI、图像生成、素材库、角色数据库
Developer Mode 引入通用 Codex apply_patch / shell，保持 Production 隔离
## 28.3 NovelAI 集成
JSON 一键导入/导出仍作为稳定主链路
后续再接 NovelAI API（若官方接口/权限满足）
生成候选图回填 Panel；记录 seed/model/params
避免 API 逻辑侵入 Template/Agent Core
# 29. 主要风险与对策
# 30. 从当前 Skill 到桌面端的迁移方案
冻结当前 .skill 作为 Migration Fixture，不再直接把 SKILL.md 当运行时逻辑。
解包 30 套 template JSON 到 immutable originals，并计算 sha256。
把 template-selection.md 的归一化、权重、Top-K 规则转成 Rust Matcher 配置与测试。
把 template-mutation.md 的 Character/Scene/Change Budget 转成 Semantic Patch Operation + Validator。
把 SKILL.md 的硬约束提炼成 CORE_CONTRACT.md，不把历史说明全部塞进 system prompt。
把 style-guide/tag-vocab/narrative-templates 保留为 fallback/reference assets，而非主要生成器。
重建 template metadata；修正旧 character_count 问题。
用原 30 套跑 Golden Cases，确保桌面端行为与当前 Template Clone Mode 一致。
# 31. 推荐代码仓库结构
storyboard-studio/
├─ apps/desktop/                 # React/Tauri UI
├─ crates/
│  ├─ storyboard-domain/         # Template/Project/Patch types
│  ├─ storyboard-storage/        # SQLite/files
│  ├─ storyboard-importer/       # template scan/index
│  ├─ storyboard-matcher/        # Top-K
│  ├─ storyboard-clone/          # deterministic clone
│  ├─ storyboard-patch/          # semantic patch
│  ├─ storyboard-validator/      # gates
│  ├─ agent-runtime/             # Codex-derived lifecycle
│  ├─ agent-protocol/            # typed events/requests
│  ├─ model-providers/           # provider adapters
│  ├─ storyboard-tools/          # v1.0 internal typed tool registry
│  └─ app-server/                # in-process facade
├─ prompts/
├─ migrations/
├─ fixtures/
│  └─ current-skill/
└─ tests/golden/
# 32. 工程约束
领域逻辑不写进 React；React 不直接操作 JSON 文件。
Agent 不拥有项目文件句柄；所有领域写入由 Application Controller 调用 commit_storyboard_patch。
模板 originals 永远不可写。
任何 Provider 不能绕过 Tool Router/Permission Policy。
每个 PatchOperation 必须有 unit test 与明确的 precondition；MVP rollback 统一依赖 parent snapshot，不强制实现 inverse operation。
所有协议结构使用 serde + JSON Schema/TypeScript 类型生成，避免前后端手写重复类型。
对 Codex 上游代码的复用集中在 codex_adapter/agent-runtime，不让业务 crate 直接依赖大量上游内部类型。
对上游 Codex 固定 commit/tag，并维护 NOTICE/Apache-2.0 归属信息。
# 33. 外部技术依据（截至 2026-09-03）
以下只用于确认 Codex 复用方案的当前事实；业务规则仍以本项目现有 Skill 与桌面端设计为准。
OpenAI Codex 官方源码仓库 — https://github.com/openai/codex
OpenAI Codex License — Apache-2.0
codex-rs workspace / crate structure
codex-protocol README
codex app-server-client README
model-provider-info registry
model-provider crate
app-server protocol schemas
# 34. 最终技术决策
Desktop      = Tauri 2 + React
Core         = Rust
Storage      = SQLite + immutable JSON originals
Matcher      = deterministic rule Top-K first
Agent        = Codex-derived Rust Runtime
Protocol     = typed in-process events + Tauri IPC
Tools        = internal typed Storyboard Tool Registry (v1.0)
MCP          = adapter / plugin protocol (v1.1)
Patch        = semantic typed patch + preconditions (not arbitrary JSON rewrite)
Commit       = Application Controller only; never exposed to Production Agent
Validation   = deterministic gates before every commit
Provider     = Codex-inspired provider abstraction
Permissions  = Production restricted / Developer approved
Versioning   = snapshot + diff + rollback
Expansion    = FTS/Embedding only after template scale requires it
一句话：借 Codex 的 Agent 基础设施，不把 Codex 的“任意代码仓库修改权”照搬到 Storyboard 生产模式；Agent 只提出 Semantic Patch，Application Controller 才能在前置条件、Validator 与 Approval 全部通过后 Commit；模板检索、克隆、Patch、Validator 与版本系统始终由桌面端自己的 Storyboard Core 掌控。

[TABLE 1]
目标 | 第一版标准
模板驱动 | 任何生成先选择一个 Primary Template；默认不从零创作
可扩展 | 30 套可扩展至 100/500/1000 套，新增模板不修改核心代码
可控 Agent | AI 只读上下文并提出 Patch；程序验证后应用
多模型 | OpenAI / Claude / GLM / OpenAI-compatible / Local 通过 Provider 层切换
本地优先 | 模板、角色库、项目、Diff、索引均默认存本地
可回退 | 每次 Commit 创建 Project Version，可查看 Diff 与回滚
可审计 | 所有 Tool Call / Patch / Gate / Approval 均有事件记录

[TABLE 2]
编号 | 冻结决定
F01 | Storyboard 写接口与 Codex apply_patch 分离：领域写入统一使用 propose/preview/validate/commit_storyboard_patch。
F02 | commit_storyboard_patch 不注册给 Production Agent；只有 Application Controller 能在 Gate + Approval 后调用。
F03 | 所有修改既有内容的 PatchOperation 必须携带 expected_project_version，并按需携带 expected_old / expected_old_hash。
F04 | MVP 回滚使用完整 parent snapshot；不强制每个 Operation 实现 inverse。
F05 | 增加 Phase 0.5 Codex Integration Spike，在正式开发前验证 crate 真实复用边界并锁定上游 commit。
F06 | MCP 延后到 v1.1；v1.0 只实现内部 Typed Tool Registry。
F07 | 每次 Agent Run 保存 Run Manifest：Provider/Model/Prompt/Contract/Tool Registry/Template Revision/Base Version/采样参数。

[TABLE 3]
字段 | 建议新定义
total_role_count | 整套模板中语义上存在的主要角色总数
female_lead_count / male_lead_count | 整套主要角色按性别/角色类型统计；未知则 unknown
max_simultaneous_slots | 任一 Panel 中同时出现的最大 character slot 数
character_anchors | 整套实际身份锚集合
confidence | 自动索引字段的置信度，低置信度要求人工确认

[TABLE 4]
层 | 技术/职责
UI | React + TypeScript；模板库、项目页、Diff、Agent、设置
Desktop Shell | Tauri 2；窗口、文件选择、通知、IPC、密钥存储桥接
Storyboard Core | Rust；Importer/Matcher/Clone/Patch/Validator/Export
Agent Runtime | Codex-derived Rust；Thread/Turn/Tool/Approval/Event/Provider
Storage | SQLite + 原始 JSON 文件；内容哈希与版本快照
Extension | v1.0 内部 Typed Tool Registry；v1.1 增加 MCP-compatible Adapter

[TABLE 5]
Codex 组件 | 本项目用途 | 策略
core | Thread/Turn、Agent 生命周期、Context、Tool 调度 | 参考设计；必要时抽取小模块
protocol / app-server-protocol | 前后端事件/请求类型 | 强参考；自建 storyboard 协议扩展
app-server / app-server-client | in-process runtime 与事件流 | 优先参考，减少自写生命周期代码
model-provider / model-provider-info | Provider 抽象、base_url、auth、retry | 优先复用思想/接口
apply-patch | 通用文本文件 patch | 仅用于开发者模式/配置文件
MCP | 工具扩展与外部能力 | v1.1 扩展协议；不进入 v1.0 核心链路
approval / sandbox | 权限、审批、工具边界 | 强参考；生产模式进一步收紧
skills / prompts | 预设指令、能力包 | 借鉴为 Storyboard Prompt Preset/Skill Pack

[TABLE 6]
Profile | 默认权限 | 适用场景
Storyboard Production | 只读模板/项目；search/read/propose_patch/validate；无任意 shell/write_file | 普通生成、换角色、换场景、局部修改
Developer | workspace-write + Codex apply_patch + 可选 shell；必须 approval；MCP 自 v1.1 可选 | 开发软件、批处理模板、调试插件

[TABLE 7]
文件 | 职责
CORE_CONTRACT.md | 永久硬约束：模板优先、单 Primary、禁止无关重写、Patch-only
INTENT_PARSER.md | 把自然语言转成 QueryIntent
TEMPLATE_MATCH.md | 只在程序 Top-K 候选中语义重排/解释
CHARACTER_REPLACE.md | 角色身份替换范围
SCENE_ADAPT.md | 场景映射范围
USER_DELTA.md | 用户明确新增/删除/调整事项
PATCH_GENERATOR.md | 只输出 PatchProposal Schema
REVIEWER.md | 基于 Diff + Validator Report 输出 pass/fail
FAILURE_RECOVERY.md | 只修 validator 报错项，禁止扩大 scope

[TABLE 8]
维度 | 默认权重 | 说明
场景/地点 | 35 | 先同场景族硬过滤；exact_scene 再加分
结构/主题 | 20 | 以非敏感的叙事结构标签、阶段结构、交互类型匹配
人物数量 | 15 | 使用修复后的 total_role_count/max_simultaneous_slots
时间/环境 | 10 | day/night/sunset/weather 等
Pace/格数 | 10 | pace + panel_count 距离
镜头/道具 | 10 | camera_profile / important_props / composition

[TABLE 9]
类别 | 字段示例
标识 | template_id, revision, title, source_name, sha256
场景 | scene_family, exact_scene, time, environment_tags
人物 | total_role_count, lead counts, character_anchors, max_simultaneous_slots
结构 | panel_count, pace, stage_count, opening_type, ending_type
镜头 | camera_profile, composition_profile, aspect_ratio_profile
道具 | important_props, keywords
质量 | metadata_confidence, warnings, reviewed_at

[TABLE 10]
Operation | 作用
ReplaceCharacterIdentity | 替换身份锚、固有外貌、角色专属服装/道具
ReplaceSceneToken | 按场景映射替换地点/环境/道具
PatchPromptBlock | 在明确 panel + anchor 上做最小文本块修改
UpdateTitle | 更新项目/Panel title
RegenerateIds | 生成新的项目/Panel UUID
RegenerateSeeds | 按策略更新 seed
ResizeStoryboard | 用户明确指定格数时才允许，独立高风险操作
DeleteConflictingBlock | 删除与用户明确要求冲突的块

[TABLE 11]
Gate | 检查内容 | 失败处理
Schema Gate | schemaVersion、顶层字段、Panel 字段、类型、必填结构 | 拒绝 Commit
Scope Gate | Operation 是否超出用户/任务允许范围 | 拒绝并回传越界项
Anti-Rewrite | 原文保留率、非目标块变化、镜头/顺序/权重结构漂移 | 回滚 Patch
Identity Leak | 旧角色锚、外貌、专属物残留 | 生成结构化命中列表
Scene Leak | 旧场景地名/环境/场景专属物残留 | 生成结构化命中列表
Patch Preconditions | base version、expected_old / expected_old_hash 是否仍与当前状态一致 | 返回 STALE_PATCH / PRECONDITION_FAILED
Reference Integrity | source template revision/hash 是否一致 | 阻止陈旧基线 Patch
JSON Parse | 最终导出可解析 | 阻止保存/导出

[TABLE 12]
Provider | MVP 方式
OpenAI | 原生 Provider
Claude | 独立 adapter；若使用不同 wire protocol，在 adapter 内转换
GLM | 优先 OpenAI-compatible；用户配置 base_url/api_key/model
OpenAI-compatible | 通用自定义 Provider
Ollama / LM Studio | 本地 Provider，可沿 Codex OSS provider 思路

[TABLE 13]
Tool | 权限 | 说明
search_templates | 只读 | 查询模板元数据与 Top-K
read_template_summary | 只读 | 读取候选模板概要
read_template_panels | 只读 | 按范围读取完整 panel
read_project | 只读 | 读取当前项目状态
read_diff_context | 只读 | 读取基线与当前差异
propose_storyboard_patch | 提案 | 只提交 PatchProposal，不落盘
validate_storyboard_patch | 只读计算 | 执行所有 Gate
preview_storyboard_patch | 只读计算 | 在内存副本应用并返回预览 Diff
commit_storyboard_patch | App-only 写入 | 仅 Application Controller 可调用；不注册给 Production Agent
rollback_version | 写入 | 用户明确动作
export_json | 写出 | 导出到用户选择路径

[TABLE 14]
动作 | Production | Developer
读取模板/项目 | 自动 | 自动
提出 Patch | 自动 | 自动
批准低风险身份/场景 Patch | 可配置 auto / prompt | 自动或 prompt
删除 Panel / Resize | 必须 prompt | 必须 prompt
任意文件 write | 禁止 | workspace 范围 + prompt
Shell | 禁止 | 默认 prompt
外部 Tool/MCP 写操作（v1.1） | 按 Tool 单独审批 | 按配置

[TABLE 15]
表 | 核心字段
templates | id, title, current_revision_id, created_at
template_revisions | id, template_id, file_path, sha256, schema_fingerprint, imported_at
template_metadata | revision_id, scene_family, exact_scene, time, panel_count, pace, role counts, confidence
template_tags | revision_id, kind, value, weight
characters | id, name, identity_json, default_outfit_json
projects | id, title, source_template_revision_id, current_version_id
project_versions | id, project_id, parent_id, snapshot_path, diff_path, created_at
patches | id, project_id, base_version_id, proposal_json, validation_json, status
agent_threads | id, project_id, provider_id, model, created_at
agent_events | thread_id, seq, type, payload_json, created_at
agent_runs | id, thread_id, provider_id, model, prompt_preset_version, core_contract_hash, tool_registry_version, template_revision_id, base_project_version_id, sampling_json, manifest_json, created_at
providers | id, type, config_json_without_secret
settings | key, value_json

[TABLE 16]
页面 | 主要内容
Library | 模板卡片、场景族、格数、人物数、导入、校验状态
New Project | 输入主题/场景/角色/时间/格数；显示 Top-K
Match | score breakdown、模板预览、Primary Template 选择
Storyboard | Panel 网格、Prompt、角色槽、状态、画幅
Diff | 模板 vs 当前版本；按身份/场景/其他分类变化
Agent | 对话、Tool Calls、Patch Proposal、Validator、Approval
Characters | 角色库与身份锚管理
Settings | Provider、模型、Matcher 权重、Approval、Prompt Preset
Diagnostics | 数据库、模板索引、Gate 阈值、日志

[TABLE 17]
场景 | 目标
30~100 套模板规则检索 | < 100 ms（不含 LLM）
500~1000 套 SQLite/FTS 检索 | < 300 ms 目标
80~100 Panel Deep Clone | < 100 ms
Diff + Schema Gate | < 300 ms
完整 Leak/Rewrite Validator | < 1 s 目标（纯本地）
UI 打开 100 Panel 项目 | 首屏 < 1 s，Panel 虚拟列表

[TABLE 18]
Case | 输入 | 预期
A Exact Clone | 已有场景 + 新角色 | 选同场景模板；只身份块变化
B Scene Clone | 结构相似但地点变化 | 场景块变化；镜头/顺序保持
C No exact match | 陌生但可映射场景 | 选最近模板；标记低 fidelity
D Wrong index | 旧 index 人数错误 | Importer 重算并阻止错误 metadata 生效
E Agent overreach | Agent 提议改写大量无关 Panel | Scope/Anti-Rewrite FAIL
F Stale patch | 基线版本或 expected_old/hash 已变 | Patch Preconditions FAIL；不得模糊匹配继续写
G Rollback | Commit 后用户撤销 | 恢复 parent snapshot
H Commit boundary | Production Agent 尝试调用 commit_storyboard_patch | Tool 不存在/Permission Denied；只能由 Application Controller 提交

[TABLE 19]
阶段 | 周期 | 主要交付物 | 退出条件
Phase 0 基线/数据修复 | 第1周前半 | 解析现有 .skill；30 模板迁移；重建 metadata；修复人物统计 | 30 模板可重复导入；metadata 审核通过
Phase 0.5 Codex Integration Spike | 第1周后半 | 最小 Rust/Tauri Agent Thread；1 个自定义 Storyboard Tool；事件流；OpenAI-compatible Provider 验证 | 形成 Codex 复用决策表：直接依赖 / 抽取 / 仅借设计，并锁定上游 commit
Phase 1 桌面壳/存储 | 第2周 | Tauri+React、SQLite、workspace、Library UI | 可导入/浏览/查看模板
Phase 2 Matcher/Clone | 第3周 | QueryIntent、Top-K、score breakdown、Deep Clone、v1 project | 三类匹配 Golden Cases 通过
Phase 3 Patch/Validator | 第4周 | Semantic Patch、Diff、Schema/Scope/Leak/Rewrite Gate、版本/回滚 | 无 AI 也能完成确定性替换与审计
Phase 4 Codex-derived Runtime | 第5-6周 | Thread/Turn、Tool Router、Event、Approval、Prompt Preset | Agent 可只读并产生 PatchProposal
Phase 5 Provider | 第7周 | OpenAI + OpenAI-compatible + Claude/GLM adapter；ProviderCapabilities 与 fallback | 至少 3 Provider 可切换；MCP 不阻塞 v1.0
Phase 6 Agent UX | 第8周 | Agent panel、Tool log、Validation、Approval、retry | 完整端到端 Agent 流程可用
Phase 7 稳定化 | 第9周 | 压力测试、错误恢复、Secret、日志、安装包 | 核心回归稳定
Phase 8 v1.0 | 第10周 | 文档、迁移工具、发布构建 | Golden Cases + release checklist 全通过

[TABLE 20]
ID | 任务
P0-01 | 编写 .skill extractor，导出 template-library 与规则文件
P0-02 | 设计 TemplateRevision 与 metadata schema
P0-03 | 重写角色统计：整套扫描，不依赖单格槽位
P0-04 | 为 30 套模板生成新 metadata 并人工复核
P05-01 | Codex Integration Spike：跑通最小 in-process Thread/Turn
P05-02 | 注册 1 个自定义 Storyboard read-only tool 并验证 typed tool call
P05-03 | 验证 event stream → Tauri UI 桥接
P05-04 | 验证至少 1 个 OpenAI-compatible Provider，并输出复用边界决策记录
P1-01 | 初始化 Tauri 2 + React + Rust workspace
P1-02 | SQLite migrations + repository layer
P1-03 | Template Library UI + import dialog
P2-01 | QueryIntent parser（纯规则第一版）
P2-02 | scene_alias normalization
P2-03 | Top-K scorer + breakdown
P2-04 | Deep Clone + project v1
P3-01 | PatchProposal / PatchOperation Rust types
P3-02 | Patch Engine + precondition checks + atomic commit
P3-03 | Diff Engine
P3-04 | Schema/Scope/Rewrite/Leak Validators
P3-05 | Project snapshot version / rollback（MVP 不要求 inverse operation）
P4-01 | Thread/Turn manager
P4-02 | Codex-style tool registry + typed tool calls
P4-03 | Prompt Preset loader/versioning
P4-04 | Approval policy
P4-05 | Agent event stream → Tauri UI
P5-01 | ModelProvider abstraction
P5-02 | OpenAI provider
P5-03 | OpenAI-compatible provider（GLM 等）
P5-04 | Claude adapter
P6-01 | Agent chat + tool timeline
P6-02 | Patch preview / approve / reject
P6-03 | Validator error retry loop
P7-01 | Keychain + secrets
P7-02 | Crash recovery / partial write recovery
P7-03 | Installer + auto backup

[TABLE 21]
类别 | 必须满足
模板 | 30 套全部导入、哈希、只读、metadata 可审计
索引 | 角色数量修复；场景/格数/人物等关键字段有 confidence
Matcher | Top-K 可解释；同场景族优先；随机可复现
Clone | 原模板不被修改；字段/Panel 顺序完整继承
Patch | Agent 不能直接覆盖 project.json；写操作均为 typed operation；每个修改操作具备 precondition
Validator | Schema/Scope/Rewrite/Identity/Scene/Reference Integrity 全部可阻断
Version | 每次 Commit 产生版本；可回滚
Provider | OpenAI + 至少一个 OpenAI-compatible + Claude/GLM 中至少一个可用
Agent | 只读检索→Patch→Gate→Approval→Application Controller Commit 全链路可观测；Run Manifest 完整
安全 | API Key 不明文；Production 无 shell/write_file
导出 | 导出的 JSON 可被现有软件读取，schema 不新增非法字段

[TABLE 22]
风险 | 后果 | 对策
Codex 上游变化快 | 直接依赖内部 crate 可能破坏升级 | 优先“借设计+抽取稳定接口”；建立 codex_adapter 层，锁定 commit
模板 metadata 错误 | 匹配器选错模板 | Importer confidence + 人工复核 + metadata version
LLM 越界修改 | 作者原模板保真下降 | typed patch + Scope + Anti-Rewrite + approval
Prompt 太长/上下文膨胀 | 成本与漂移上升 | 只读必要 panels；摘要与原文分层；context budget
多 Provider 能力差异 | 工具调用/结构化输出不一致 | ProviderCapabilities + fallback path
JSON Schema 变更 | 导出不兼容 | schema fingerprint + migration layer
MCP/外部工具风险（v1.1） | 数据泄漏/写入风险 | v1.0 不依赖 MCP；v1.1 默认关闭，每 server 权限与 approval
模板数量变大 | 线性扫描变慢 | SQLite/FTS/embedding 分阶段升级
