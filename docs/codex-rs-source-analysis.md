# codex-rs 源码分析

本文分析的是当前仓库里的 `codex-rs/` Rust workspace，也就是这个项目的主实现，而不是 `claude/` 分支里的还原 TypeScript 源码。

## 结论摘要

- `codex-rs` 是一个很大的 Rust workspace，顶层约有 74 个一级子目录，包含大量功能拆分 crate。
- 它不是“一个 TUI crate 加几个工具 crate”这么简单，而是一个由 CLI、TUI、核心代理运行时、协议层、app-server、执行与沙箱、登录鉴权、MCP、插件/技能、状态持久化和模型提供方适配共同组成的平台。
- `codex-cli` 是用户最常见的入口，但它本质上更像一个多路分发壳层，真正的会话运行时在 `codex-core`，交互体验在 `codex-tui`，远程/IDE 集成入口在 `codex-app-server`。
- 从代码组织看，Rust 版本比 `claude` 还原源码更强调边界清晰、协议显式、权限和沙箱治理前置，以及多入口共享同一核心线程运行时。

## 1. Workspace 结构

`codex-rs/Cargo.toml` 显示这是一个大 workspace，成员覆盖了：

- 用户入口：`cli`、`tui`
- 核心运行时：`core`
- 协议与类型：`protocol`、`app-server-protocol`
- 服务端：`app-server`、`app-server-client`
- 执行与沙箱：`exec`、`exec-server`、`sandboxing`、`execpolicy`、`linux-sandbox`、`windows-sandbox-rs`
- 集成层：`mcp-server`、`codex-mcp`、`rmcp-client`、`connectors`
- 用户与状态：`login`、`state`、`feedback`
- 能力扩展：`plugin`、`skills`、`core-skills`、`tools`
- 提供方与模型：`chatgpt`、`ollama`、`lmstudio`、`models-manager`、`model-provider-info`
- 以及大量 utils crate

这说明它的设计方向不是“把所有逻辑堆进一个大二进制”，而是以 workspace 作为架构边界，把能力按职责拆开。

## 2. 运行入口

### `codex-cli`

`codex-rs/cli/src/main.rs` 是主用户入口。它有两个关键信号：

- 顶层用 `clap` 统一解析交互式和非交互式模式。
- 它不是只启动 TUI，还暴露了一整组子命令。

主要子命令包括：

- `exec` / `review`：非交互式执行路径
- `login` / `logout`
- `mcp` / `mcp-server`
- `app-server`
- `sandbox`
- `resume` / `fork`
- `cloud`
- `apply`

因此 `codex-cli` 的定位更像“统一控制平面”。它负责把不同运行模式路由到合适的 crate，而不是自己承载主要业务逻辑。

### `codex-tui`

`codex-rs/tui/src/main.rs` 很薄，核心工作是调用 `codex_tui::run_main(...)`。这说明：

- TUI 不是独立产品逻辑，而是核心运行时之上的一个交互前端。
- CLI 层和 TUI 层通过明确的参数结构连接，而不是互相渗透。

## 3. `codex-core` 是真正的核心运行时

`codex-rs/core/src/lib.rs` 暴露了这个 crate 的真实定位：它不是单一算法库，而是整个代理线程运行时的中枢。

从模块出口可以看出 `codex-core` 负责：

- 会话/线程：`codex_thread`、`thread_manager`
- 代理执行：`codex`、`codex_delegate`、`agent`
- 配置与加载：`config`、`config_loader`
- 工具执行：`tools`、`function_tool`、`mcp_tool_call`
- 权限与安全：`exec_policy`、`sandboxing`、`seatbelt`、`landlock`、`windows_sandbox`
- 状态与持久化：`rollout`、`message_history`、`state_db_bridge`
- 记忆、技能、插件、project doc、上下文管理

这一层决定了整个系统的产品边界。

## 4. 核心抽象是 Thread，而不是单次请求

`codex-rs/core/src/codex_thread.rs` 和 `thread_manager.rs` 非常关键。

### `CodexThread`

`CodexThread` 是一条会话线程的双向消息管道抽象。它暴露的能力包括：

- `submit(op)`：提交操作
- `next_event()`：拉取事件
- `shutdown_and_wait()`
- `steer_input(...)`
- 查询 token usage、config snapshot、rollout path、state db

这说明核心抽象不是“给模型发一个 prompt 再拿回字符串”，而是“维护一个长期存在的会话线程，用户和代理通过 Submission/Event 队列交互”。

### `ThreadManager`

`ThreadManager` 管理多个线程及其共享服务，包括：

- `AuthManager`
- `ModelsManager`
- `EnvironmentManager`
- `SkillsManager`
- `PluginsManager`
- `McpManager`
- `SkillsWatcher`

这意味着线程不是孤立对象，而是运行在一个具备认证、模型、环境、技能、插件和 MCP 资源的共享宿主里。

## 5. 协议层是第一等公民

`codex-rs/protocol/src/protocol.rs` 直接说明 Codex 采用 Submission Queue / Event Queue 模式。

协议层定义了：

- `Submission`
- `Op`
- `Event`
- `UserInput`
- 审批、权限、计划、动态工具、MCP、模型消息等结构

几个重要结论：

- Rust 实现把“代理与客户端之间的交互”建模成显式协议，而不是仅靠函数调用或 UI 状态同步。
- 这个协议同时服务于 TUI、本地客户端、app-server、远程连接和测试。
- `Op::UserTurn` 之类的设计说明每一轮 turn 都携带 cwd、审批策略、sandbox policy 等完整上下文，不依赖隐式全局状态。

这类协议化设计是整个系统可扩展到 IDE、桌面端、远程客户端的基础。

## 6. 工具系统在 Rust 里更偏“注册表 + 执行管线”

`codex-rs/core/src/tools/registry.rs` 展示了 Rust 实现的工具模型。

这里的关键点是：

- 工具统一抽象为 `ToolHandler`
- 注册表按 `(namespace, name)` 管理 handler
- 执行时区分 `Function` 与 `Mcp` 两种工具类型
- 工具调用前后会经过 hook、sandbox tag、mutating 判断、审计和结果包装

换句话说，Rust 版本的工具系统更强调：

- 静态类型边界
- 统一 dispatch
- hook 化扩展点
- 工具生命周期治理

这比单纯“把工具当函数列表传给模型”更偏运行时平台。

## 7. TUI 是厚前端，不是薄壳

`codex-rs/tui/src/lib.rs` 的模块列表非常长，说明 TUI 不是简单聊天框。

它覆盖了：

- 应用主状态：`app.rs`、`app_event.rs`
- 底部输入和审批 UI：`bottom_pane/`
- 历史渲染、markdown、diff、pager overlay
- slash command、resume picker、theme picker、model migration
- app-server session 适配
- streaming 渲染与帧率控制
- 通知、标题、配色、外部编辑器、语音

这说明 Rust TUI 的角色不是“渲染核心状态的被动视图”，而是一个完整终端应用层，负责把事件流、审批流、消息流和工具输出组织成可交互界面。

## 8. `app-server` 让核心运行时变成外部可消费服务

`codex-rs/app-server/src/lib.rs` 和 `message_processor.rs` 说明 app-server 的职责不是简单转发 HTTP。

它负责：

- 管理 transport：stdio / websocket
- 处理 JSON-RPC 请求与通知
- 创建 `ThreadManager`
- 把客户端请求转换为线程与工具运行时操作
- 提供 config、FS、watch、external agent config、模型等 API
- 管理连接状态、出站消息路由和优雅关闭

这说明 app-server 的定位是“把 `codex-core` 线程运行时包装成可供外部客户端消费的控制平面”。

因此，如果说：

- `codex-tui` 是本地交互前端
- `codex-core` 是代理运行时

那么：

- `codex-app-server` 就是远程/IDE/桌面集成用的服务宿主

## 9. `app-server-protocol` 和 `protocol` 分工不同

这两个协议 crate 容易混淆，但定位不一样。

### `codex-protocol`

面向“Codex 线程运行时”的内部/半内部协议，定义会话事件、turn、审批、消息、sandbox、动态工具等核心概念。

### `codex-app-server-protocol`

面向 app-server 的 JSON-RPC API 面，包含：

- `common`
- `thread_history`
- `v1`
- `v2`
- schema 和 TS 导出

它更像“客户端集成 API 合约”，不是底层线程事件模型本身。

这套双层协议设计说明项目明确区分了：

- 运行时内部事件语义
- 面向外部客户端的 RPC API 语义

这种分层是合理的，否则 app-server 很容易和 core 强耦合。

## 10. 配置系统非常重，而且是产品核心之一

`codex-rs/core/src/config/mod.rs` 很大，导入也非常密集。它承担的不只是“读 config.toml”：

- 多层 config 合并
- CLI overrides
- 权限、网络、沙箱、shell 环境策略推导
- 模型提供方与 service tier 选择
- profile、managed features、agent roles、MCP server 配置
- project doc、memory、plugin、skills、OTEL、web search 等整合

从这层代码能看出的关键判断是：

- Codex 的行为高度配置化。
- 配置不是边缘功能，而是连接权限、模型、环境、插件和产品模式的中心枢纽。
- “运行一个 turn”之前，系统已经先把大量策略在配置层收敛好了。

## 11. 安全模型是体系化的，不是工具级补丁

从 crate 和模块命名看，安全治理贯穿整个 workspace：

- `execpolicy`
- `sandboxing`
- `linux-sandbox`
- `windows-sandbox-rs`
- `shell-escalation`
- `process-hardening`
- `network-proxy`
- `permissions` 和相关协议类型

再结合 `cli` 的 `sandbox` 子命令、`config` 中的 sandbox policy、`tools/registry.rs` 中的 mutating 判断，可以得出一个明确结论：

这个仓库的默认工程假设不是“模型能安全执行命令”，而是“命令执行必须先经过策略、权限、沙箱和审批系统的共同约束”。

这也是 Rust 实现与普通 agent 项目拉开差距的地方。

## 12. crate 分层大致可以这样理解

为了阅读方便，可以把 `codex-rs` 粗分成五层：

### 1. 入口层

- `cli`
- `tui`
- `app-server`

### 2. 运行时层

- `core`
- `exec`
- `exec-server`
- `state`

### 3. 协议层

- `protocol`
- `app-server-protocol`
- `codex-api`

### 4. 集成与扩展层

- `codex-mcp`
- `mcp-server`
- `plugin`
- `skills`
- `connectors`

### 5. 平台与基础设施层

- `login`
- `models-manager`
- `sandboxing`
- `execpolicy`
- `network-proxy`
- 各种 utils crate

这个分层不是代码里硬编码的正式架构图，但从依赖和入口职责看，这是比较贴近实际的理解方式。

## 13. 与 `claude` 还原源码相比的显著差异

如果把它和前面分析过的 `claude` TypeScript 还原源码对比，几个差异很明显：

- Rust 版更强调 crate 边界，TS 版更强调单仓下模块化。
- Rust 版协议更显式，事件和操作模型更正式。
- Rust 版把 app-server 作为正式入口，而不是附属桥接能力。
- Rust 版安全和沙箱治理更加前置，也更系统化。
- Rust 版 TUI 虽然很厚，但核心会话抽象明显在 `core`，而不是散落在 UI 组件中。

## 14. 推荐阅读顺序

如果后续要继续深入读 `codex-rs`，建议按这个顺序：

1. `codex-rs/Cargo.toml`
2. `codex-rs/cli/src/main.rs`
3. `codex-rs/tui/src/lib.rs`
4. `codex-rs/core/src/lib.rs`
5. `codex-rs/protocol/src/protocol.rs`
6. `codex-rs/core/src/codex_thread.rs`
7. `codex-rs/core/src/thread_manager.rs`
8. `codex-rs/core/src/tools/registry.rs`
9. `codex-rs/core/src/config/mod.rs`
10. `codex-rs/app-server/src/lib.rs`
11. `codex-rs/app-server/src/message_processor.rs`
12. 再按具体兴趣进入 `exec`、`sandboxing`、`mcp`、`plugin`、`skills`、`state`

这个顺序能先建立“多入口共享一个运行时”的大图，再钻进细节。

## 15. 总体判断

`codex-rs` 最准确的定位不是单一 CLI，也不是单一 TUI，而是：

- 一个多入口的代理运行时平台
- 一个以线程和事件协议为核心抽象的会话系统
- 一个把权限、审批、沙箱和执行策略内建进主流程的工程化代理框架
- 一个能通过 app-server、MCP、插件、技能和模型提供方适配向外扩展的宿主

如果只从 `codex` 命令本身看这个仓库，会低估它的复杂度。真正的核心是 `core + protocol + app-server` 这条主干，`cli` 和 `tui` 只是最常见的入口表现形式。

## 附：本分析的依据

- `codex-rs/Cargo.toml`
- `codex-rs/cli/src/main.rs`
- `codex-rs/cli/src/lib.rs`
- `codex-rs/tui/src/main.rs`
- `codex-rs/tui/src/lib.rs`
- `codex-rs/tui/src/cli.rs`
- `codex-rs/core/src/lib.rs`
- `codex-rs/core/src/codex_thread.rs`
- `codex-rs/core/src/thread_manager.rs`
- `codex-rs/core/src/tools/registry.rs`
- `codex-rs/core/src/config/mod.rs`
- `codex-rs/protocol/src/lib.rs`
- `codex-rs/protocol/src/protocol.rs`
- `codex-rs/app-server/src/lib.rs`
- `codex-rs/app-server/src/message_processor.rs`

