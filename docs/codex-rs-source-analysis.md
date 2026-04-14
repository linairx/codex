# codex-rs 源码分析

本文分析当前仓库里的 `codex-rs/` Rust workspace，也就是这个项目的主实现，而不是
`claude/` 分支里的 TypeScript 还原源码。

这份文档现在只承担三个职责：

- 给出 `codex-rs` 的架构总览
- 说明当前已经落地到什么程度
- 把读者送到当前仍然有效的设计稿和路线图

它不再承担：

- 某一轮 PR 的拆包说明
- checklist / template / draft 之间的跳转流程
- 某一轮 SQLite / observer / remote-bridge 收口的过程记录

这些过程性文档已经移到：

- `docs/archive/persistent-runtime/`

## 结论摘要

- `codex-rs` 是一个多入口、线程化、协议化的代理运行时平台。
- 它的主干不是单一 CLI 或单一 TUI，而是 `core + protocol + app-server`。
- 当前仓库已经明显具备 resident thread、thread mode/status、SQLite repaired-summary、
  observer/fs-watch、subagent、plan delta 等基础设施。
- 但 `claude` 里对应的高层产品模式并没有全部落地。真正仍未完成的重点，主要是：
  - 正式的持久助手模式
  - 完整远程 bridge / 远程审批
  - 更完整的多 agent 编排体验
  - 长时规划模式产品化
  - 后台任务与异步结果回流体验

## 1. Workspace 结构

`codex-rs/Cargo.toml` 显示这是一个大型 workspace，顶层成员覆盖：

- 用户入口：`cli`、`tui`
- 核心运行时：`core`
- 协议与类型：`protocol`、`app-server-protocol`
- 服务端：`app-server`、`app-server-client`
- 执行与沙箱：`exec`、`exec-server`、`sandboxing`、`execpolicy`、`linux-sandbox`、`windows-sandbox-rs`
- 集成层：`mcp-server`、`codex-mcp`、`rmcp-client`、`connectors`
- 用户与状态：`login`、`state`、`feedback`
- 扩展能力：`plugin`、`skills`、`core-skills`、`tools`
- 模型与提供方：`chatgpt`、`ollama`、`lmstudio`、`models-manager`、`model-provider-info`

这说明它的方向不是“单二进制堆所有逻辑”，而是通过 workspace 维持边界清晰的运行时平台。

## 2. 核心入口

### `codex-cli`

`codex-rs/cli/src/main.rs` 是主入口，但更像控制平面壳层，而不是主要业务实现。

它负责把不同运行模式路由到合适 crate，例如：

- `exec` / `review`
- `login` / `logout`
- `mcp` / `mcp-server`
- `app-server`
- `sandbox`
- `resume` / `fork`

### `codex-tui`

`codex-rs/tui/src/main.rs` 很薄，主要调用 `codex_tui::run_main(...)`。

这说明 TUI 是运行时之上的交互前端，而不是与 core 并列的一套会话系统。

### `codex-app-server`

`codex-rs/app-server/` 把线程运行时包装成对外可消费的控制平面。

这也是为什么很多后续产品模式最终都要落回 app-server 协议层，而不是只做 TUI 层拼装。

## 3. 真正的核心是 Thread Runtime

`codex-rs/core/src/codex_thread.rs` 与 `thread_manager.rs` 说明，系统核心抽象不是单次请求，
而是长期存在的线程：

- `submit(op)`
- `next_event()`
- `shutdown_and_wait()`
- `steer_input(...)`

线程运行在共享宿主中，宿主持有认证、模型、环境、插件、技能、MCP 等共享服务。

因此，`codex-rs` 的很多“产品能力”其实不是从零发明，而是要把已有线程运行时正式产品化。

## 4. 协议层是主干

`codex-rs/protocol/src/protocol.rs` 和 `codex-rs/app-server-protocol/` 共同说明两件事：

- 运行时内部事件语义是显式协议
- 面向客户端的 RPC 语义也是显式协议

这让它天然适合继续长出：

- 持久线程模式
- 远端消费
- 恢复与 reconnect
- 计划流与多 agent

## 5. 当前已经明显落地的能力

下面这些不该再被写成“纯前瞻”。

### resident / thread mode / thread status 基底

协议层已经有：

- `ThreadMode::{Interactive, ResidentAssistant}`
- `ThreadActiveFlag::{WaitingOnApproval, WaitingOnUserInput, BackgroundTerminalRunning, WorkspaceChanged}`

这说明“长期线程”和“当前运行状态”已经有正式 API 形态，而不是只在 TUI 内部脑补。

### SQLite repaired-summary 收口

服务端读取面已经明显不只是“有 SQLite 就直接信”，而是开始做 repaired-summary 收敛：

- `thread/read`
- `thread/list`
- `thread/resume`
- `thread/metadata/update`
- `thread/unarchive`
- `thread/rollback`
- `getConversationSummary`

也就是说，SQLite 这一层已经不是空设计，而是已经进入实现与修边界阶段。

### observer / fs-watch 基础

协议层已经有：

- `fs/watch`
- `fs/unwatch`
- `fs/changed`

这不是完整 observer 产品，但已经足够说明“事件源”不是空白。

### subagent / multi-agent 基底

仓库里已经有：

- `spawn_agent`
- subagent metadata
- subagent notification
- TUI 对 loaded subagent 的回填

所以“多 agent”并不是零基础，而是缺完整产品组织层。

### planning 基底

协议与服务端已经有：

- `TurnPlanUpdatedNotification`
- `PlanDeltaNotification`

这说明 planning 流已存在，但还不等于长时规划模式已经产品化。

### experimental feature 装配

协议和 TUI 已经有：

- `ExperimentalFeatureList*`
- `ExperimentalFeatureEnablementSet*`
- 实验功能 UI 入口

因此“实验功能组织能力”是部分落地，不是完全缺失。

## 6. 仍然明显没完成的任务

这里说的“没完成”，是指还没有达到 `source-analysis` 想表达的那种高层产品模式。

### 1. 正式持久助手模式

目前已有的是：

- `resident: true`
- `residentAssistant`
- reconnect / status / watcher / SQLite 的一批基底

但缺的仍是一个真正面向产品的正式模式：

- 长期驻留线程的清晰生命周期
- 无前台窗口时继续运转的完整语义
- 统一后台状态模型
- 客户端稳定的 reconnect 心智

### 2. 完整远程 bridge / 远程审批

目前已有的是：

- app-server
- typed client
- 最小远端消费闭环的基础

但缺的仍是：

- 正式远端控制面
- 远程审批闭环
- 本地状态到远端的完整桥接体验

### 3. 更完整的多 agent 编排体验

目前已有的是：

- `spawn_agent`
- subagent threads
- 部分 TUI backfill

但缺的仍是：

- coordinator / worker 的显式产品模型
- 多 agent 任务面板
- 子代理状态可视化
- 结果汇总与协调体验

### 4. 长时规划模式产品化

目前已有的是 planning 事件流，不是完整 planning 产品模式。

仍缺：

- 独立 planning mode
- 规划与执行的显式切换点
- 长时计划的审阅、修改、批准语义

### 5. 后台任务与异步结果产品面

目前已有的是 `backgroundTerminalRunning` 之类状态位。

仍缺：

- 后台任务面板
- 统一通知中心
- 稍后恢复查看结果的统一用户路径

## 7. 当前更可信的优先级

按现有代码状态，比起继续扩基础设施，当前更合理的顺序是：

1. 把持久助手模式正式产品化
2. 基于这层稳定线程语义继续做 remote bridge / remote approvals
3. 再做更完整的 multi-agent orchestration 体验
4. 再做长时规划模式产品化
5. 最后再把后台任务与异步结果体验做完整

更短地说：

- 先把线程长期存在这件事做成正式产品模式
- 再让远端和多 agent 建立在这层之上

## 8. 当前仍然有效的文档入口

如果现在要继续做真正的开发判断，更适合读的是下面这些长期文档：

- `docs/codex-rs-roadmap.md`
- `docs/persistent-runtime-implementation-plan.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/remote-bridge-consumption.md`
- `docs/sqlite-state-convergence.md`

如果只是想看历史拆包、PR 模板、checklist、drafts，请去：

- `docs/archive/persistent-runtime/`

## 9. 这份文档的当前角色

现在这份文档应被理解成：

- 总体架构入口
- 完成度判断入口
- 路线图入口

不应再把它当成：

- 活的任务堆栈
- 某一轮 PR 的工作流说明
- checklist / draft / template 导航页

## 附：分析依据

- `codex-rs/Cargo.toml`
- `codex-rs/cli/src/main.rs`
- `codex-rs/tui/src/main.rs`
- `codex-rs/tui/src/lib.rs`
- `codex-rs/core/src/lib.rs`
- `codex-rs/core/src/codex_thread.rs`
- `codex-rs/core/src/thread_manager.rs`
- `codex-rs/protocol/src/protocol.rs`
- `codex-rs/app-server/src/lib.rs`
- `codex-rs/app-server/src/message_processor.rs`
- `codex-rs/app-server-protocol/src/protocol/v2.rs`
