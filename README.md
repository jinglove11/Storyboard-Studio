# NovelAI Storyboard Studio

本地优先的模板驱动 Storyboard JSON 工作台。基于 v2.1 Architecture Freeze 开发计划书实现。

**核心原则**:`Template first. Clone before generate.` · `AI proposes. Program validates. Commit only after PASS + approval.`

## 这是什么

把既有 Reference Template Cloner Skill(30 套只读作者模板 + 检索/克隆/最小修改规则)产品化为桌面应用:

- 30 套模板按 **sha256 内容寻址**导入为 immutable originals,永不被改写
- **确定性 Matcher**:场景族硬过滤 → 六维加权 Top-K(带 score breakdown)→ dominance/加权随机选择;AI 只允许在 Top-K 内做语义解释
- **Clone Engine**:纯确定性 deep clone(新 UUID/seed,原文逐字保留,可复现)
- **Semantic Patch**:Agent 不碰 JSON,只能提交 typed `PatchProposal`(8 种操作,每个改动带 `expected_project_version` / `expected_old` 前置条件)
- **七道确定性 Gate**:Schema / Scope / Anti-Rewrite / Identity Leak / Scene Leak / Reference Integrity / JSON Parse
- **Application Controller 独占提交**:临时文件 → 重新解析 → Schema 校验 → 原子 rename → 新版本 + diff + 审计事件;`commit_storyboard_patch` **不在** Production Agent 工具表中
- **版本与回滚**:每次 Commit 产生不可变快照;回滚 = 以新版本恢复父快照(F04)
- **Agent Runtime**(Codex-derived 设计):Thread/Turn、工具循环、Run Manifest(provider/model/契约哈希/工具表版本/采样参数)、审批策略、事件流
- **Provider 层**:OpenAI-compatible HTTP(GLM/LM Studio/Ollama 等)+ 脚本化 Mock(离线测试)

## 仓库结构

```
├─ apps/desktop/            # React + TS UI(Vite)+ Tauri 2 壳(src-tauri)
├─ crates/
│  ├─ storyboard-domain     # 模板/项目/Patch/Schema 类型(30 套实测冻结 schema)
│  ├─ storyboard-importer   # Phase 0:skill 提取、全卷重扫角色统计(P0-03)、metadata+置信度
│  ├─ storyboard-storage    # SQLite(rusqlite bundled)+ workspace 文件布局 + 原子写
│  ├─ storyboard-matcher    # QueryIntent 解析(规则版)、scene_aliases、Top-K、加权随机
│  ├─ storyboard-clone      # Deep Clone + 保证校验器
│  ├─ storyboard-patch      # Patch 引擎(前置条件/STALE_PATCH)、token 边界替换、Diff
│  ├─ storyboard-validator  # 七道 Gate
│  ├─ agent-protocol        # typed 事件 + EventBus(§17.1)
│  ├─ storyboard-tools      # 内部 Typed Tool Registry(§15,生产档无 commit)
│  ├─ model-providers       # Provider trait + OpenAI-compatible + Mock
│  ├─ agent-runtime         # Thread/Turn/Manifest/Approval + prompts 组装
│  └─ app-server            # Application Controller + `sbx` CLI
├─ prompts/v1/              # CORE_CONTRACT 等 6 个预设(§7)
├─ migrations/              # SQLite 迁移
├─ fixtures/current-skill/  # 冻结的 .skill 迁移基线(只读)
└─ docs/                    # v2.1 开发计划书全文
```

## 快速开始

```bash
# 前置:Rust 1.75+、Node 20+、(Linux 桌面构建需 webkit2gtk-4.1-dev / gtk3-dev / libsoup-3.0-dev)

# 1) 核心 + 测试(44 个,含真实 30 套模板的 Golden Cases A–H)
cargo test --workspace

# 2) CLI 端到端 demo(匹配→克隆→换角色→验证→提交→导出→回滚)
cargo run -p app-server --bin sbx -- demo ./ws-demo

# 3) 桌面应用
cd apps/desktop
npm install
npm run tauri dev      # 开发
npm run tauri build    # 安装包
```

CLI 其他命令:`init` / `list-templates` / `match` / `clone` / `list-projects` / `export` / `rollback`。

## 安全边界(v2.1 冻结决定)

| 冻结项 | 实现 |
|---|---|
| F01 领域写接口与通用 apply_patch 分离 | `propose/preview/validate/commit_storyboard_patch` 独立于文本 patch |
| F02 commit 不注册给 Agent | `ToolRegistry::for_profile(Production)` 不含 commit;仅 `AppServer::commit_patch` |
| F03 前置条件 | `expected_project_version` + `expected_old(_hash)`;不一致 → `STALE_PATCH`/`PRECONDITION_FAILED`,绝不模糊匹配 |
| F04 回滚 = 父快照 | `AppServer::rollback` 以新版本恢复旧内容,历史不可变 |
| F06 MCP 延后 v1.1 | v1.0 仅内部 Typed Tool Registry |
| F07 Run Manifest | 每次 Turn 固化 provider/model/契约哈希/工具表版本/基线版本/采样参数 |

## 测试

- 单元测试:token 边界替换、锚提取、Clone 保证、前置条件、Schema、Gate、意图解析、加权随机确定性…
- `crates/app-server/tests/golden.rs`:**Golden Cases A–H**(真实 30 套 fixtures)+ Mock Provider 的 Agent 端到端全循环

## License / NOTICE

本项目代码 Apache-2.0。Agent Runtime 借鉴 OpenAI Codex(Apache-2.0)的架构设计(thread/turn、协议分层、provider 抽象、approval 思想),未直接复制其源码;后续引入上游 crate 时在 NOTICE 中记录归属与锁定 commit。
