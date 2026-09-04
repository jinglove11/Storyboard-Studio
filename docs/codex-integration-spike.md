# Codex Integration Spike 报告(Phase 0.5)

日期:2026-09-04 · 执行:对上游真实源码的盘点 + 编译实测 + 设计对照审计

## 上游锁定

- 仓库:`git@github.com:openai/codex.git`(Apache-2.0)
- **锁定 commit:`f3f6922519fa38487c8250c2b8a670a39a2cf9ff`**(2026-09-04,Narrow async user message guidance #42677)
- 工具链要求:Rust **1.95.0** + edition 2024(本仓当前 1.93,edition 2021)

## 实测结论

`codex-rs` 为 121 个 crate 的超大 workspace。计划书 Table 5 关注的组件实测情况:

| Codex 组件 | LOC | 实测情况 | 决策 |
|---|---|---|---|
| `codex-core`(thread/turn 生命周期) | 215k | 拖数十个内部 crate(agent-graph-store/cloud-tasks/login/execpolicy…),TUI/CLI/云功能深度耦合 | **仅借设计** |
| `codex-protocol`(事件/请求类型) | 28k | 传递依赖 execpolicy、network-proxy、http-client、gix-url 等代码代理专用件 | **仅借设计** |
| `codex-app-server-protocol` | 34k | 依赖 codex-protocol 全链 + rollout/secrets/history | **仅借设计** |
| `codex-app-server-client`(in-process 客户端) | 3k | 依赖 codex-app-server → codex-core 全量 | **仅借设计** |
| `codex-model-provider-info`(provider 元数据/registry) | 1.3k | 结构最干净(`ModelProviderInfo`/`WireApi`/`built_in_model_providers`/`create_oss_provider_with_base_url`),但依赖链死于上游 fork patch(见下) | **抽取结构定义**(自研 `StoryboardModelProvider` + OpenAI-compatible provider 已覆盖同等语义) |
| `codex-apply-patch` | 5k | 依赖 tree-sitter-bash;且 F01 要求领域写入不得走通用文本 patch | **不引入** |
| MCP(`rmcp-client`/`ext/mcp`) | — | v1.0 主链路不需要(F06) | **v1.1 再评估** |

### 关键阻塞:上游 fork patch

codex 的 `[patch.crates-io]` 指向 **openai-oss-forks** 的三个分叉:

```toml
crossterm        = { git = ".../openai-oss-forks/crossterm",        rev = "45fecb95..." }
tokio-tungstenite= { git = ".../openai-oss-forks/tokio-tungstenite",rev = "0e5b2d73..." }
tungstenite      = { git = ".../openai-oss-forks/tungstenite-rs",   rev = "4fffad30..." }
```

实测:不复制这三条 patch,`codex-api` 的版本解析直接失败(`tokio-tungstenite/proxy` 特性在 crates.io 官方版不存在)。复制则有连带风险——`crossterm` patch 会替换整个 workspace 的 crossterm,与 Tauri 的依赖冲突。加上 Rust 1.95 工具链要求,直接依赖在当前不可行。spike 编译验证留在 `crates/codex-spike/`(指向本地锁定克隆,需重新指向 git+rev 后方可构建)。

## 设计对照审计(自研 runtime vs Codex 真实实现)

对照 `CodexThread`(`submit(Op)` / `start_or_steer_turn` / `suspend_turn_and_shutdown` / rollout 截断)后确认的自研差距与处置:

| 发现 | 处置 |
|---|---|
| Tauri 全局单锁,长 turn 冻 UI | ✅ 已修:Arc<AppServer> 无锁 + 后台线程 + app.emit 事件流 |
| Provider 无超时/重试,可挂死 | ✅ 已修:120s 硬超时 + 指数退避(429/5xx/传输错误) |
| Run Manifest/agent_events 未持久化(F07 半落地) | ✅ 已修:RunObserver——manifest 开跑即落盘,事件逐条入库(seq 单调) |
| §21 状态链跳过 CommitRequested | ✅ 已修:approval→CommitRequested→Committed→Versioned |
| trait 同步签名,堵死流式输出 | 🟡 v1.1:async 化或加回调式流接口(破坏性变更,越晚越贵) |
| 无 context 预算/压缩(§29) | 🟡 v1.1:对话历史截断 + 只读必要 panel |
| turn 不可取消/不可 steer(Codex 支持) | 🟠 v1.1+:CancellationFlag + steer 队列 |

## 最终决策(对齐计划书 Table 5)

- **复用策略 = 借设计 + 自研**,与计划书 §5"选择性借鉴/抽取而非完整 Fork"一致;Spike 证实"直接依赖"路线当前不可行且收益低
- 上游锁定 commit `f3f6922` 仅作设计参照,不进依赖树;若 v1.1 引入 MCP(`rmcp`)或 provider-info 结构,以 git+rev 依赖并同步复制 patch 段,同时在本文件追加记录
- 归属声明见 `NOTICE`
