# NovelAI Storyboard Studio

本地优先的模板驱动 Storyboard JSON 工作台。基于 v2.1 Architecture Freeze 开发计划书实现。

**核心原则**:`Template first. Clone before generate.` · `AI proposes. Program validates. Commit only after PASS + approval.`

## 这是什么

把既有 Reference Template Cloner Skill(30 套只读作者模板 + 检索/克隆/最小修改规则)产品化为桌面应用:

- 30 套模板按 **sha256 内容寻址**导入为 immutable originals,永不被改写
- **确定性 Matcher**:场景族硬过滤 → 六维加权 Top-K(带 score breakdown)→ dominance/加权随机选择;AI 只允许在 Top-K 内做语义解释
- **Clone Engine**:纯确定性 deep clone(新 UUID/seed,原文逐字保留,可复现)
- **Semantic Patch**:Agent 不碰 JSON,只能提交 typed `PatchProposal`(8 种操作,每个改动带 `expected_project_version` / `expected_old` 前置条件)
- **八道确定性 Gate**:Schema / Scope / Anti-Rewrite / Identity Leak / Scene Leak / Reference Integrity / JSON Parse / **Clothing Chain**(服装状态链:同格换装、只减不回穿、负权防回穿同步换词)
- **CharacterReplacementPlan / SceneMappingPlan**:`ReplaceCharacterIdentity` 携带 `appearance_replacements`(发色/瞳色/固有服装/固有道具);场景替换下每个旧场景 token 必须 **映射或显式保留**(`kept_tokens`),否则 Scene Leak Gate 阻断
- **Application Controller 独占提交**:临时文件 → 重新解析 → Schema 校验 → 原子 rename + 目录 fsync → **单事务 SQLite 终结**(版本行 + 项目状态 + patch 状态 + 审计);崩溃后孤儿快照在启动时**前滚收养**;`commit_storyboard_patch` **不在** Production Agent 工具表中
- **版本与回滚**:每次 Commit 产生不可变快照 + 结构化 diff;回滚 = 以新版本恢复父快照并记录回滚 diff(F04)
- **Agent Runtime 2.0**(Codex-derived 设计):长寿命 Thread + Op 队列(`UserTurn`/`Steer`/`Cancel`/`Shutdown`)、模型调用独立任务(**排队不取消**)、turn 级 patch 状态机(propose → 按 patch_id validate → 审批同一行)、CancellationToken、token 级流式、上下文预算(消息数 + 字符双上限,tool 配对保真)、持久 rollout + 重启恢复、per-thread 单调 seq
- **Provider 层**:OpenAI-compatible wire DTO(tool_calls 嵌套 `function.{name,arguments}`、tool_call_id、跨 chunk UTF-8 安全的 SSE 解析)+ OS 钥匙串密钥(Windows 凭据管理器 / macOS Keychain / Linux Secret Service;SQLite 只存引用)+ 连通性测试;Mock 仅显式演示模式

## 仓库结构

```
├─ apps/desktop/            # React + TS UI(Vite)+ Tauri 2 壳(src-tauri)
├─ crates/
│  ├─ storyboard-domain     # 模板/项目/Patch/Schema/服装链类型(30 套实测冻结 schema)
│  ├─ storyboard-importer   # Phase 0:skill 提取、全卷重扫角色统计(P0-03)、metadata+置信度
│  ├─ storyboard-storage    # SQLite(rusqlite bundled,事务/条件状态迁移)+ workspace + 原子写(含目录 fsync)
│  ├─ storyboard-matcher    # QueryIntent 解析(规则版)、scene_aliases、Top-K、加权随机
│  ├─ storyboard-clone      # Deep Clone + 保证校验器
│  ├─ storyboard-patch      # Patch 引擎(前置条件/STALE_PATCH)、token 边界替换、Diff
│  ├─ storyboard-validator  # 八道 Gate
│  ├─ agent-protocol        # typed 事件 + EventBus(§17.1;wire 名与 type_name 契约测试锁定)
│  ├─ storyboard-tools      # 内部 Typed Tool Registry(§15,生产档无 commit;validate 支持 by patch_id)
│  ├─ model-providers       # Provider trait + OpenAI-compatible wire DTO + Mock
│  ├─ agent-runtime         # Thread/Turn/Manifest/Approval/预算/rollout
│  └─ app-server            # Application Controller + Provider 配置/钥匙串 + `sbx` CLI
├─ prompts/v1/              # CORE_CONTRACT 等 6 个预设(§7)
├─ migrations/              # SQLite 迁移
├─ fixtures/current-skill/  # 冻结的 .skill 迁移基线(只读,随安装包打包为 resource)
└─ docs/                    # v2.1 开发计划书全文 + codex 集成调研
```

## 快速开始

```bash
# 前置:Rust 1.93(rust-toolchain.toml 已锁定)、Node 22+、
#      (Linux 桌面构建需 webkit2gtk-4.1-dev / gtk3-dev / libsoup-3.0-dev / javascriptcoregtk-4.1-dev)

# 1) 核心 + 测试(83 个,含真实 30 套模板的 Golden Cases + 崩溃恢复/审批边界回归)
cargo test --workspace

# 2) CLI 端到端 demo(匹配→克隆→换角色→验证→提交→导出→回滚)
cargo run -p app-server --bin sbx -- demo ./ws-demo

# 3) 桌面应用
cd apps/desktop
npm install
npm run tauri:dev      # 开发
npm run tauri:build    # 安装包
```

CLI 其他命令:`init` / `list-templates` / `match` / `clone` / `list-projects` / `export` / `rollback`。

### 桌面端首个 Provider

1. 「设置」页:填 provider id / base URL / 模型名 → 保存
2. 输入 API key → 写入(进 OS 钥匙串,不落 SQLite)→ 测试连通 → 激活
3. 「Agent」页选择项目执行;未配置 Provider 时页面明确拒绝执行(也提供独立的"确定性换角色"快捷操作,不经模型)

## 安全边界(v2.1 冻结决定)

| 冻结项 | 实现 |
|---|---|
| F01 领域写接口与通用 apply_patch 分离 | `propose/preview/validate/commit_storyboard_patch` 独立于文本 patch |
| F02 commit 不注册给 Agent | `ToolRegistry::for_profile(Production)` 不含 commit;仅 `AppServer::commit_patch`;审批走条件状态迁移(`validated→approved`,校验 project 归属) |
| F03 前置条件 | `expected_project_version` + `expected_old(_hash)`;不一致 → `STALE_PATCH`/`PRECONDITION_FAILED`,绝不模糊匹配 |
| F04 回滚 = 父快照 | `AppServer::rollback` 以新版本恢复旧内容 + 回滚 diff,历史不可变 |
| F06 MCP 延后 v1.1 | v1.0 仅内部 Typed Tool Registry |
| F07 Run Manifest | 每次 Turn 固化 provider/model/契约哈希/工具表版本/基线版本/采样参数;thread 行先于事件落库(FK 顺序) |
| 密钥安全 | API key 仅存 OS 钥匙串;SQLite `providers.config_json` 只含引用;审计不含密钥 |

## 数据可靠性

- commit = 原子文件写(temp → fsync → rename → 目录 fsync)+ **单事务 SQLite 终结**;任何一步失败不产生半提交状态
- 崩溃恢复:启动时扫描磁盘版本目录,孤儿快照(已写文件未落库)**前滚收养**;重试路径识别同字节版本
- 消息持久化:per-thread 数据库分配单调 seq(重启不覆盖);rollout.jsonl 与数据库同序追加
- 持久化失败不再静默:`persistence_warnings` 队列 + 事件总线可见

## 测试

- 单元测试:token 边界替换、锚提取、Clone 保证、前置条件、Schema 嵌套类型、Gate、意图解析、加权随机确定性、SSE 跨 chunk UTF-8、wire DTO、上下文预算字符上限与 tool 配对、排队不取消、并发 spawn、服装链阶段模式……
- `crates/app-server/tests/golden.rs`:**Golden Cases A–H**(真实 30 套 fixtures)+ Mock Provider 的 Agent 端到端全循环 + 审批越权阻断 + AutoLowRisk 落库 + 孤儿快照收养 + 跨重启单调 seq

## CI

`.github/workflows/ci.yml`:rustfmt / clippy `-D warnings` / `cargo test --locked` / 前端 lint+tsc+build / Tauri Linux 构建。

## License / NOTICE

本项目代码 Apache-2.0(见 [LICENSE](LICENSE))。Agent Runtime 借鉴 OpenAI Codex(Apache-2.0)的架构设计(thread/turn、协议分层、provider 抽象、approval 思想),未直接复制其源码;上游锁定 commit 与决策记录见 [NOTICE](NOTICE) 与 [docs/codex-integration-spike.md](docs/codex-integration-spike.md)。

`fixtures/current-skill/` 下的 30 套作者模板为迁移基线资产,其原作者权利与分发授权需在使用/分发前单独确认(见 NOTICE)。
