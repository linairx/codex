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

## 16. 从 `claude` 到 `codex-rs` 的移植路线图

结合 `docs/claude-source-analysis.md` 和本文，可以更明确地判断：`codex-rs` 最缺的不是底层执行能力，而是若干已经在 `claude` 里验证过的“产品模式”和“体验闭环”。

换句话说，值得移植的重点不是花哨功能本身，而是那些能显著提升 `codex-rs` 完整性的运行模式。

### 第一优先级

#### 1. 远程桥接与远程审批

`claude` 的 `Bridge` 体系最值得参考。

原因不是因为它“能在网页上遥控本地 CLI”很炫，而是因为 `codex-rs` 已经具备更适合做这件事的底座：

- `codex-app-server`
- `codex-app-server-protocol`
- `codex-core` 的线程模型
- 显式的审批、权限和事件协议

所以最值得移植的是：

- 本地会话和远端客户端之间的桥接协议
- 远程审批回传
- 会话状态同步
- 后台任务和通知从本地回流到远端

这类能力能直接把 `codex-rs` 从“本地终端工具”升级为“本地运行时 + 远端控制面”的双端产品。

#### 2. 持久助手模式

`claude` 的 `KAIROS`/assistant 类能力也很值得迁移。

`codex-rs` 已经有：

- `ThreadManager`
- rollout 和状态持久化
- app-server
- 线程级事件流
- 后台执行和审批系统

缺的是一个正式的“长期驻留助手模式”，例如：

- 会话不中断地长期存在
- 可恢复、可轮询、可异步接收新任务
- 可在无前台 TUI 的情况下继续运转
- 有明确的后台状态和通知模型

这类能力对 `codex-rs` 的价值远高于增加若干零散命令。

#### 3. 更完整的多 Agent 编排体验

`claude` 里的 coordinator/worker 分工，是 `codex-rs` 值得借鉴的第三个高优先级方向。

Rust 版已经有比 `claude` 更好的基础：

- 线程抽象明确
- 协议层强
- app-server 可作为外部控制平面
- 安全和审批模型完善

因此最值得补的不是“再加一个 spawn agent 接口”，而是：

- coordinator 和 worker 的显式角色模型
- 多 agent 任务面板
- 子代理状态可视化
- 代理之间的消息与结果汇总体验

这会显著提升 `codex-rs` 在复杂任务下的组织能力。

### 第二优先级

#### 4. 长时规划模式

`claude` 的 `Ultraplan` 代表的是“把复杂问题单独切到长时规划模式”。

对 `codex-rs` 来说，值得移植的不是 Anthropic 内部那套云端实现，而是这种产品形态本身：

- 规划与执行分离
- 长时任务独立线程化
- 计划结果可审阅、可修改、可批准
- 执行前有明确切换点

因为 `codex-rs` 本身已经是线程化、协议化系统，这类模式比在普通 CLI 里更容易做成正式能力。

#### 5. 后台任务与异步结果回流体验

`claude` 在“命令丢后台继续跑、之后再回来确认/查看结果”这件事上，产品意识更强。

`codex-rs` 虽然在运行时和协议上已经有大量基础设施，但用户层面仍然可以更进一步：

- 明确区分前台 turn 和后台任务
- 后台任务完成后主动通知
- 支持远端或稍后恢复查看结果
- 对长任务、定时任务、持续监控任务提供统一展示

这类能力和 `codex-rs` 的线程、状态和 app-server 架构天然兼容。

### 第三优先级

#### 6. 更丰富的实验功能装配方式

`claude` 通过 feature gate 和用户类型把大量能力做成“可门控模式”。

`codex-rs` 不应该照搬那种复杂度，但可以借鉴其中有价值的部分：

- 把实验功能组织成明确模式，而不是零散隐藏参数
- 让 TUI、CLI、app-server 三个入口共享同一能力开关
- 让协议和配置层清楚表达实验能力是否启用

对 Rust 版来说，重点应该是少而精的模式化，而不是复制一大套零散隐藏命令。

## 17. 不建议直接移植的部分

并不是 `claude` 里出现的功能都值得照搬。

### 1. `Buddy` 宠物系统本体

这个功能对 `codex-rs` 的主价值帮助不大，更多是趣味性和身份感设计。除非产品方向明确要求，否则优先级很低。

### 2. 依赖内部 Anthropic 平台的能力

像明显依赖内部用户类型、内部远程服务、专有后台平台的能力，不适合直接移植。移植后往往只会留下残缺接口，增加维护成本。

### 3. 过量的 feature gate 碎片化能力

`claude` 的一个问题是门控点太多，阅读和维护成本都很高。`codex-rs` 应该借鉴“模式化能力”，而不是复制“碎片化门控”。

## 18. 为什么这些方向最适合 `codex-rs`

核心原因是：`codex-rs` 的底层已经比 `claude` 还原源码更成熟。

它已经拥有：

- 更清晰的核心运行时
- 更正式的协议层
- 更强的 app-server 宿主
- 更系统的权限、审批和沙箱模型

因此，`codex-rs` 最值得补的是：

- 产品模式
- 远端控制闭环
- 多 agent 协作体验
- 长期运行与异步任务体验

而不是再发明一套底层工具执行框架。

## 19. 推荐实施顺序

如果把这些建议转成实际 roadmap，最合理的顺序是：

1. 远程桥接 + 远程审批
2. 长驻助手模式
3. 多 agent 协调器体验
4. 长时规划模式
5. 后台任务与通知模型增强
6. 再考虑低优先级体验型功能

这个顺序的好处是：

- 每一步都能复用现有 `core + protocol + app-server` 主干
- 每一步都能带来明显的产品体验提升
- 不需要先大规模重写当前 Rust 架构

## 20. `observer` 与 `sqlite-rs` 的位置

如果目标是解决长期记忆、上下文衰减和文件变化驱动的实时状态，那么 `observer` 和 `sqlite-rs` 都是有价值的，但它们更适合作为中期基础设施增强，而不是最先做的事项。

### 先判断结论

- `observer` 有必要
- `sqlite-rs` 也有必要
- 但二者都不应排在“持久助手模式、远程桥接、多 agent 编排”之前

原因是：`codex-rs` 当前更缺的是长期运行的产品模式，而不是底层存储介质本身。

### 为什么 `observer` 值得做

如果这里的 `observer` 指的是对工作区、关键文件、配置和外部状态变化的持续观察层，那么它对 `codex-rs` 的几个方向都很关键：

- 长驻助手模式需要感知文件变化，才能避免记忆和上下文长期过期
- 远程桥接需要把本地状态变化实时推给远端客户端
- 多 agent 模式需要共享“仓库刚刚发生了什么变化”的事实来源
- 背景任务、监控任务和文件索引刷新都需要事件源，而不是纯轮询

换句话说，`observer` 的价值不是“再加一个 watcher”，而是把“文件系统变化”变成正式的会话事件和状态刷新触发器。

### 为什么 `sqlite-rs` 值得做

如果要把长期能力做稳，SQLite 一类本地嵌入式状态层会非常有帮助，尤其适合承载：

- 长期记忆索引
- 文件变化事件的持久化摘要
- 会话派生状态
- 后台任务元数据
- 多 agent 协作状态
- 可恢复的上下文缓存

这类状态如果长期分散在内存、rollout 文件和零散 JSON 文件里，后面会越来越难统一管理。SQLite 能提供更稳定的恢复性、查询能力和一致性。

### 为什么它们现在不是第一优先级

尽管有必要，但如果在没有长期运行模式之前先投入大量时间重做 `observer + sqlite-rs`，很容易出现两个问题：

- 做了很多基础设施，但用户可感知能力提升不明显
- 真实产品模式还没稳定，过早固化状态模型，后面反而容易返工

从当前 `codex-rs` 的架构看，更合理的顺序是：

- 先把“长期驻留、可恢复、可异步继续工作”的产品模式做出来
- 再把 `observer` 作为这些模式的实时事件源
- 再逐步把长期记忆和实时状态收敛到 SQLite

### 更合理的实施方式

如果要兼顾短期收益和长期演进，可以按下面方式推进：

1. 先做持久助手模式的最小闭环
2. 同步补文件变化驱动的 observer 事件流
3. 把 observer 事件接入线程状态、提示构建和通知链路
4. 再用 SQLite 收敛长期记忆、后台任务和实时状态
5. 最后让 bridge / app-server / 多 agent 统一消费这套状态层

这种顺序的优势是：

- 不会阻塞当前最有价值的产品模式建设
- 不会把 SQLite 过早变成全局重构
- 可以先验证哪些状态真的值得持久化

## 21. 实施优先级矩阵

下面这张矩阵给出一个更实际的排序：

### 高收益、应优先启动

- 持久助手模式
  直接解决“会话无法长期存在”的核心问题，是长期记忆和异步任务的前提。
- 远程桥接与远程审批
  能把现有 `app-server` 价值放大，让长期运行状态真正可被外部消费。
- observer 事件流
  应尽早开始，但作为“状态刷新基础设施”并行推进，不应取代产品模式本身。

### 中期重点建设

- 多 agent 协调器体验
  依赖长期状态和更好的任务可视化，但对复杂任务价值很高。
- SQLite 状态汇聚层
  应在长期模式和 observer 的真实需求稳定后引入，避免过度设计。
- 长时规划模式
  适合在基础长期运行模式稳定后叠加。

### 低优先级或后置

- 趣味型体验功能
- 大量 feature gate 碎片化扩展
- 依赖内部平台的专有能力

### 一句话优先级结论

如果只能选一件现在做，优先做持久助手模式。  
如果可以并行推进两条线，主线做“持久助手/远程桥接”，基础设施线做“observer 事件流”。  
SQLite 应该尽早进入设计，但晚于产品模式落地，作为状态收敛层逐步引入，而不是先全局重写。

## 22. 把“持久助手模式”落到当前架构上的最小闭环

如果沿着上面的优先级继续推进，下一步不应该再停留在“这个方向值得做”的层面，而应该把它收敛成一个最小可交付能力。

从现有实现看，`codex-rs` 其实已经具备这条路线的大半基底：

- `codex-core` 已经有长期存在的 `ThreadManager` 与 `CodexThread`
- `app-server` 已经有 `thread/start`、`thread/resume`、`thread/list`、`thread/read`
- `thread/start` / `thread/resume` 已经支持 `resident: true`
- app-server 已经维护 `ThreadStatus`，并能推送 `waitingOnApproval`、`waitingOnUserInput`、`backgroundTerminalRunning`、`workspaceChanged`
- rollout 与 sqlite 状态层已经能承载“线程可恢复”和部分派生状态
- core 已经有 `notify_background_event(...)` 这类后台事件回流机制

因此，“持久助手模式”最合理的定义不是发明一个全新运行时，而是把这些已有能力组合成一个正式产品模式。

### MVP 应该解决什么

最小闭环应该只解决四件事：

1. 线程可以在没有前台 TUI 的情况下继续存在
2. 用户可以稳定地重新连接到这个线程，而不是把“resume”仅当作历史恢复
3. 后台执行、审批等待、用户输入等待、工作区变化都能被统一表达成线程状态
4. 客户端可以区分“普通会话”与“驻留助手会话”，并围绕后者提供更稳定的恢复和通知体验

注意这里刻意不把“长期记忆”“多 agent 编排”“远程网页控制”“复杂调度器”塞进第一阶段。否则范围会立刻失控。

### MVP 的建议边界

第一阶段建议把“持久助手模式”定义为：

- 一个显式的线程模式，而不是隐含地等价于 `resident: true`
- 线程可在最后一个客户端断开后保持 loaded
- 线程状态在 `thread/list`、`thread/read`、`thread/loaded/read` 上保持一致可见
- 用户可重新连接并继续订阅该线程的 turn/item/status 通知
- 后台终端、审批、用户输入、workspace change 继续映射到统一状态面

换句话说，`resident: true` 更像底层机制，而“持久助手模式”应该是面向产品与 API 的正式语义包装。

### 为什么不要直接把它做成“守护进程系统”

一个容易走偏的方向是立刻引入独立 daemon、全局 supervisor、复杂任务调度和跨进程 leader 选举。  
从当前仓库实际情况看，这一步太早。

更务实的方式是：

- 先把 app-server 视为持久宿主
- 让 resident thread 成为长期存在单元
- 把 reconnect、status、notification、resume 体验补完整
- 等产品模式稳定后，再判断是否需要更强的进程级常驻宿主

这条路径复用现有架构最多，返工最少。

## 23. 一个更贴近代码现实的实施拆解

如果把这项工作真正拆成开发任务，我会建议按下面顺序推进。

### 阶段 1：先把“resident thread”升级为正式用户能力

这一步主要是“把已有机制产品化”，不是重写 core。

建议做的事情：

- 在 app-server 协议层给这类线程一个更明确的模式字段，而不只靠请求参数 `resident: true`
- 在 `thread/list` / `thread/read` 返回里明确体现它是否属于长期驻留线程
- 在 TUI 或其他客户端里，把这类线程和普通一次性会话在文案、筛选、恢复入口上区分开
- 明确“最后一个订阅者断开后仍保持 loaded”的行为边界，以及何时真正 shutdown

这一步的核心目标是把“resident”从内部实现细节抬升为用户可理解的产品概念。

### 阶段 2：统一状态面，而不是继续堆零散事件

当前 app-server 已经有较好的线程状态基础，这反而说明第二阶段不该新增太多平行通知类型，而应该继续收敛到统一状态面。

建议补强的点：

- 把“后台有事正在发生”优先表达为线程状态变化，而不是让客户端自己拼装 item 流
- 明确 active / idle / notLoaded 之外哪些状态是产品级一等公民
- 区分“线程活着但空闲”和“线程活着且正在后台等待某件事”
- 让 `notify_background_event(...)` 一类机制更稳定地映射到上层可消费状态

这样做的好处是，远端客户端、TUI、后续 bridge 都能共享一套状态语义，而不是分别解读日志和 item。

### 阶段 3：补 observer，但只把它当事件源

observer 值得尽早做，但职责要收紧。

它在这一阶段最合理的角色是：

- 观察 cwd 下的重要文件变化
- 产出节流后的工作区变化事件
- 驱动 `workspaceChanged` 之类的线程状态
- 为后续提示重建、通知和异步恢复提供事实来源

不建议在 observer 第一版里顺手做：

- 全量索引系统
- 长期记忆系统
- 复杂规则引擎
- 多 agent 协作状态总线

第一版 observer 的目标应该只是把“变化发生了”正式接进线程模型。

### 阶段 4：SQLite 再接管真正需要长期保存的状态

SQLite 在这里不是起点，而是收敛点。

等前面几个阶段稳定后，再把下面这些状态逐步收敛进 sqlite 会更合理：

- 驻留线程元数据
- 后台任务元数据
- observer 事件摘要
- 多 agent 派生状态
- 更稳定的恢复游标和通知游标

只有当产品模式稳定后，才知道哪些状态值得长期持久化，哪些只是瞬时噪声。

## 24. 与远程桥接、多 Agent、长时规划的关系

如果前面的“持久助手模式”没有先做扎实，后面几个高优先级方向都会变成半成品。

### 为什么它是远程桥接的前提

远程桥接真正需要的不是“能远端发一个请求”，而是：

- 本地线程长期存在
- 远端断开后还能重连
- 远端回来时能读到一致状态
- 审批和后台任务不会因为前台窗口关闭而丢语义

这些都更依赖 resident thread + 状态面，而不是新的 transport。

### 为什么它也是多 Agent 体验的前提

多 agent 一旦变得正式，就一定会遇到：

- 子线程是否长期存在
- 协调器是否还能看到已关闭前台的 worker
- 后台 worker 的等待状态怎么展示
- 结果和中间进度如何在稍后恢复查看

这些问题本质上还是“长期线程和统一状态面”的问题。

### 为什么长时规划模式应该后置

长时规划模式当然重要，但它建立在“线程可以长期存在并被恢复”这个前提上。

否则规划线程会变成：

- 能生成计划
- 但生命周期脆弱
- 缺少稳定状态
- 难以在稍后恢复与批准

所以它更像持久助手模式稳定之后的上层产品模式，而不是替代品。

## 25. 协议层面最小应该怎么演进

如果真的要把“持久助手模式”推进成正式能力，第一阶段最值得改的不是 core 内部抽象，而是 app-server 协议对线程语义的表达能力。

因为从当前 `app-server-protocol` 看：

- `Thread` 已经有 `resident: bool`
- `ThreadStatus` 已经区分 `NotLoaded`、`Idle`、`SystemError`、`Active`
- `Active` 已经有 `waitingOnApproval`、`waitingOnUserInput`、`backgroundTerminalRunning`、`workspaceChanged`

这说明现在的问题不是“完全没有协议基础”，而是还缺少一层更稳定的产品语义。

### 当前 `resident: bool` 的问题

`resident: bool` 很有用，但它更像运行时保活开关，而不是用户真正关心的线程模式。

它的问题在于：

- 它描述的是“保不保活”，不是“这个线程是什么类型”
- 客户端很难仅凭这个布尔值推导是否应该把该线程当作长期助手来展示
- 后续如果还要支持不同长期模式，单个布尔值会很快失去表达力

因此，更合理的方向不是删除 `resident`，而是在它之上增加更明确的模式字段。

### 建议的最小协议增量

第一阶段更像是“补可读语义”，不需要激进改协议。

建议增加一个线程模式字段，类似：

- `interactive`
- `residentAssistant`
- 以后如有需要再扩展 `planner`、`coordinator`、`backgroundWorker`

这里的关键不是命名本身，而是：

- 让线程类型成为一等语义
- 让客户端不再通过 `resident + source + nickname + status` 去猜
- 为后续多 agent、长时规划保留扩展位

### 为什么不要一开始就把状态枚举做得很大

另一个容易过度设计的方向，是马上引入一长串新状态，例如：

- `WaitingOnApproval`
- `WaitingOnUserInput`
- `RunningBackgroundTask`
- `WorkspaceChanged`
- `Reconnecting`
- `Detached`
- `Paused`

从当前代码看，这些信息已经主要体现在 `ThreadStatus::Active { activeFlags }` 里。  
所以第一阶段更合理的做法是保留这个结构，只把线程模式补清楚，而不是重做状态体系。

换句话说：

- 模式字段解决“这是什么线程”
- 状态字段解决“它现在在做什么”

这两个维度不要混在一起。

## 26. 状态面应该如何继续收敛

当前 `ThreadStatus` 的方向其实是对的，问题主要不在结构本身，而在它是否足够覆盖“长期线程”的产品需求。

### 已有状态语义的优点

现有设计的优点很明确：

- `NotLoaded` 清楚表达线程当前不在内存中
- `Idle` 清楚表达线程已加载但当前不忙
- `SystemError` 明确保留故障态
- `Active + activeFlags` 能承载多个并发事实，而不是被单一枚举限制

对于长期线程，这种“主状态 + 事实标签”的方式比纯大枚举更稳。

### 下一步最值得补的不是更多状态名，而是状态来源的一致性

真正需要继续强化的是：不同来源的后台活动，最终是否都能稳定收敛到同一个线程状态面。

第一阶段至少应该保证：

- turn 正在运行时，线程进入 active
- 后台 terminal 仍在运行时，即使 turn 结束，也仍能通过 active flag 体现
- 审批和用户输入等待不依赖客户端自己复原 item 流
- workspace 变化是线程级事实，而不是某个前端局部提示

这几个点如果能做稳，远比再发明几个新状态名字更有价值。

### 一个值得警惕的边界

`workspaceChanged` 当前已经被建模成 active flag，这在工程上很方便，但从产品语义看，它和“正在执行任务”不是同类事实。

因此后续可以接受它暂时继续挂在 `Active` 下，但应该意识到这更像：

- “线程需要关注的新外部变化”

而不是：

- “线程正在运行”

这类语义差异在第一阶段不必强行重构，但在第二阶段文档和客户端呈现上要讲清楚，否则很容易出现用户看到 active 就以为代理仍在跑。

## 27. 真正的第一阶段验收标准

如果把“持久助手模式”当成实际项目来做，最重要的是定义什么叫第一阶段完成。

我会建议第一阶段只用下面这些标准验收。

### 协议与状态

- 客户端能明确识别某线程是否为长期驻留助手线程，而不是仅凭 `resident`
- `thread/list`、`thread/read`、`thread/loaded/read` 对这类线程给出一致语义
- 线程断开订阅后若仍驻留，重新连接时状态可被准确重建

### 生命周期

- 最后一个客户端断开后，驻留线程不会被误 shutdown
- 非驻留线程仍保持现有关闭行为，不被新模式破坏
- `thread/resume` 对长期线程表达的是“重新连接/继续会话”，而不仅是“从磁盘恢复历史”

### 客户端体验

- TUI 或其他 app-server 客户端能在列表中区分长期线程与普通线程
- 用户能在稍后重新看到审批等待、后台终端、workspace change 等事实
- 客户端不需要自己拼装复杂 item 历史，仍能拿到可用状态

### 非目标

第一阶段明确不要求：

- 完整远程 bridge
- 多 agent 仪表盘
- 长时规划产品化
- SQLite 全量状态统一
- 跨进程 daemon/supervisor 架构

只要这些非目标不被偷偷夹带进来，范围才会可控。

## 28. 从这里往后的合理文档顺序

如果这份分析文档还要继续往下写，最合理的顺序应该是：

1. 补一份独立的“持久助手模式设计草案”
2. 把协议字段和线程状态变更单独列成 app-server v2 设计说明
3. 再单独写 observer 事件流设计，而不是把 watcher 和状态存储混成一份
4. 最后再写 SQLite 状态汇聚设计

这样拆分的好处是：

- 每份文档都聚焦一个层次
- 不会把产品模式、协议设计、状态存储、远程桥接揉成一锅
- 以后实现时更容易按文档拆 PR

## 29. 下一步文档落点

基于上面的拆分，这里的下一步不再继续把所有内容堆进同一份分析文档，而是开始拆独立设计稿。

第一份拆出的文档建议是：

- `docs/persistent-assistant-mode-design.md`

它应聚焦：

- 持久助手模式的产品定义
- 与 `resident` 的关系
- 最小协议增量
- 生命周期语义
- 状态面与客户端预期
- 第一阶段验收标准

这样做的意义是把“为什么值得做”与“第一阶段怎么做”分离开，避免 `codex-rs-source-analysis.md` 继续膨胀成一份同时承担路线图、设计稿、协议草案和实现计划的总文档。

在这之后，下一份更细的协议文档建议是：

- `docs/app-server-thread-mode-v2.md`

它应该只处理：

- `Thread` 上新增模式字段的必要性
- 与 `resident`、`status` 的关系
- 返回面兼容策略
- 请求面是否暂时维持 `resident`
- 客户端迁移方式

不要把 observer、SQLite、bridge 的设计讨论继续塞进去。

再下一步更合理的基础设施文档建议是：

- `docs/observer-event-flow-design.md`

它应该只处理：

- 如何复用现有 `FileWatcher` / `ThreadWatchManager`
- 如何把工作区变化收敛为线程级事件
- 如何让 `workspaceChanged` 成为正式的 observer 映射结果
- 如何把 watcher、状态面、后续消费者分层

同样不要把 SQLite 状态收敛方案提前揉进去。

在这之后，状态层文档建议再单独拆为：

- `docs/sqlite-state-convergence.md`

它应该只处理：

- 当前哪些 thread / observer / background 状态值得收敛进 SQLite
- 哪些状态仍应留在 rollout 或内存运行态
- SQLite 与 rollout、observer、app-server 状态面的边界
- 为什么不应在产品模式未稳定前过早固化数据库模型

同样不要再把远程 bridge 的消费层讨论混进状态层文档。

如果这几份设计文档都已就位，下一步最合理的不再是继续写同层设计，而是补：

- `docs/persistent-runtime-implementation-plan.md`

它应该只处理：

- 阶段划分
- PR 拆分
- 依赖顺序
- 测试与验收
- 风险控制

让前面的分析和设计文档真正能转化成实现动作。

如果在实现计划之外还要补一份消费侧设计文档，则最值得写的是：

- `docs/remote-bridge-consumption.md`

它应只处理：

- 远端控制面如何消费 `Thread.mode`
- 远端如何使用 `thread/list`、`thread/loaded/read`、`thread/status/changed`
- observer 摘要和 SQLite 元数据对远端摘要的意义
- 为什么远端首页不应依赖全量 item 流重建

同样不要把 transport 和完整 UI 产品设计揉进去。

## 30. 这条线已经进入实现阶段

到目前为止，这条后续任务已经不再只是文档拆分。

第一阶段最小代码闭环已经形成，而且已经开始向 observer 与 SQLite 收敛扩展：

- `app-server-protocol` 已新增 `Thread.mode`
- `app-server` 已在主要 `Thread` 返回面填充 `mode`
- schema / TypeScript 生成物已同步更新
- `app-server/README.md` 已明确 `Thread.mode` 与 `thread.status` 的语义分离
- `thread/read`、`thread/list` 和后续 `thread/resume` 在服务重启后已能消费 SQLite 中持久化的 resident 模式
- resident `thread/start` 也已补上“未物化前不落 SQLite”的回归覆盖，确保返回 `mode = residentAssistant` 不会顺手提前创建持久化线程行
- `thread/fork` 从 resident thread 派生新线程时，默认仍会保持 `interactive`，并且这条模式边界已补上跨重启回归覆盖，不会被 SQLite 误恢复成 `residentAssistant`
- `codex-state` 的 `set_thread_mode` 也已补上“缺失线程不建垃圾行”的回归覆盖，确保模式修复逻辑不会把未物化线程提前写入 SQLite
- `thread_status.rs` 已开始收敛 watcher 生命周期，shutdown 会主动清理 resident thread 的工作区 watch
- `workspaceChanged` 的保留与清理语义已开始稳定：shutdown 后不会被陈旧 watcher 事件重新激活，下一次 turn 完成后会清掉该标记
- resident thread 在最后一个订阅者断开后也已补上负向与读取面回归：保持 loaded 的同时不会错误发出 `thread/closed`；后续 `thread/read` 仍会稳定返回 `ResidentAssistant`，`thread/loaded/read` 也会继续把该线程暴露为 `ResidentAssistant + Idle`
- `tui` 已继续收敛消费侧语义：除了 resume picker 的入口标题和各类 reconnect 文案外，`app_server_session` 对 `thread/start`、`thread/resume`、`thread/fork` 的 resident `Thread.mode` 映射、`ThreadStartedNotification` 进入 TUI 后的 resident 会话推断路径，以及启动前按名称 lookup、latest-session 选择和按线程 ID 的 `thread/read` 路径对 resident `SessionTarget.mode` 的保留，也都已补上回归覆盖，确保会话态不会把 resident assistant 降回普通 interactive 会话
- `tui` 的随机启动 tooltip 也已开始对齐 resident reconnect 语义：`codex resume` 不再只被描述成“恢复历史会话”，而会明确提示它同时覆盖 `resume or reconnect`，避免最前面的轻提示文案继续落回旧的纯 resume 心智
- `cli` 的退出摘要提示也已开始消费 `Thread.mode`，resident assistant 不再一律提示 “continue this session”，而会明确给出 reconnect 语义
- `cli` / `exec` 的命令帮助入口也已继续对齐 resident reconnect 心智：顶层 `resume` 子命令说明和 `exec resume` 帮助摘要都已明确写成 `resume or reconnect`，不再把这条入口继续描述成单纯的历史恢复
- `tui` / `exec` 的内部 CLI 字段注释现在也已继续收口到同一口径：`resume_session_id`、resume 列表开关，以及 resume 后附带 prompt/image 的字段说明都已改成显式覆盖 `resume or reconnect`，避免维护者继续从结构体注释里读出旧的纯 resume 心智
- 顶层 `codex` CLI 的 `ResumeCommand` 内部注释现在也已补到同一粒度：session id、`--last` 与 `--include-non-interactive` 的字段说明都已明确写成 `resume or reconnect` / `resume/reconnect picker`，避免壳层注释再次落回旧的纯 resume 表述
- `app-server` 与 MCP 面向外部集成的顶层说明也已继续收口：Events 总览和 MCP thread 概览都已明确把 `resume/reconnect` 与 `thread.mode` 的关系写到首屏，不再要求外围读者先跳进细节章节再推断 resident reconnect 语义
- `app-server` 方法摘要和 `app-server-client` README 的 typed bootstrap 说明也已继续收口到同一口径：外围调用方现在会被明确引导去区分“普通 interactive resume 目标”和 resident reconnect，而不是继续使用模糊的普通 resume 表述
- `app-server-client` README 的 bootstrap 流程现在也已把这层语义写回正文：它会明确说明 resident assistant 上的 `thread/resume` 同时就是 reconnect bootstrap，调用方应直接把立即返回的 typed response 当成 `resume or reconnect` 的权威启动摘要，而不是等后续 legacy 事件再脑补 reconnect 语义
- `app-server-client` README 的 bootstrap 列表项现在也已继续补齐这层口径：最前面的流程枚举不再只写 `thread/start` 或 `thread/resume`，而会直接标明 `thread/resume` 覆盖 `resume or reconnect`，避免正文已经说明 reconnect bootstrap，但列表项还停留在旧的缩写心智
- `app-server/README.md` 顶部 lifecycle overview 的总览句子也已跟上这条精确化口径：首屏不再只写泛化的 ordinary resume，而会直接写成 ordinary interactive resume target，对外围集成的 reconnect 心智不再留灰区
- `docs/codex_mcp_interface.md` 的 thread 概览和 `app-server/README.md` 的 `thread/list` 摘要也已继续补齐到同一粒度：前者不再只笼统罗列 thread API，而会直接点出 `thread/resume` 覆盖 `resume or reconnect`；后者也明确把列表消费目标写成 ordinary interactive resume target，而不是泛化的 ordinary interactive session
- `docs/codex_mcp_interface.md` 的对照措辞现在也已继续精确化：resident assistant 不再被描述成相对一个 “normal interactive resume target”，而是直接对照成 `ordinary interactive resume target`，让 MCP 文档与 app-server / app-server-client README 保持完全一致的术语
- `app-server-client` README 的 typed bootstrap 说明和 `app-server/README.md` 的历史列表消费段也已继续收紧术语：前者现在直接写成 resident reconnect，后者也不再把 resident 行只对照成泛化的 ordinary resume targets，避免外围列表 UI 和 in-process 调用侧继续留下模糊空间
- `app-server-test-client` README 的摘要说明也已继续贴近真实输出：除了 wire `thread.mode` 值外，它现在也明确把当前 `status` 与派生的 `resume/reconnect` action label 写成输出契约，并说明这是 ordinary interactive resume target 与 resident reconnect target 的对照，不再只让读者自行推断
- `app-server-test-client` 的 CLI help 入口也已继续对齐 resident reconnect 语义：`resume-message-v2` 与 `thread-resume` 的子命令说明、参数 help 和对应回归测试都已改成 `resume or reconnect`，不再让 `--help` 输出落回旧的纯 resume 心智
- `app-server-test-client` 的 compact summary 输出现在也已补成完整回归闭环：除了 `thread_mode_label` / action label 的映射断言外，`thread/read` 与列表摘要的整串 `mode + status + action` 输出也已有测试覆盖，避免后续联调摘要在重构时重新丢掉 resident reconnect 语义
- `app-server-test-client` 的分页列表摘要现在也已继续贴近真实联调需求：`thread/list` 的 compact summary 在有分页时会直接打印 `next_cursor`，并补上有结果/空列表两侧回归，避免继续翻完整 debug struct 才能手动抄出下一页 cursor
- `app-server-test-client` 的分页列表消费现在也已补成可续页闭环：`thread-list` 与 `thread-loaded-read` 命令都新增了 `--cursor`，对应子命令 `--help` 与 README 示例也已补齐，手工联调分页历史或 loaded 线程时不再只能看到 `next_cursor` 却没法直接继续请求下一页
- `app-server-test-client` 的 `thread/loaded/list` 现在也已补成真正可用的 id-only probe：CLI 新增 `thread-loaded-list --cursor --limit`，compact summary 会直接打印 loaded thread ids 与 `next_cursor`，并通过 help/README/回归测试把“这条接口只负责 id probe；需要 resident `mode`、当前 `status` 或 reconnect 语义时继续读 `thread-loaded-read`”这层边界锁住
- `app-server-test-client` 的 `thread/loaded/list` 摘要契约现在也已补上字符串级断言：id-only summary 会稳定渲染 header、每条 loaded thread id 行以及分页 `next_cursor` 尾行，避免这条 probe 在后续重构里悄悄退回完整 debug 输出或丢掉续页提示
- `app-server-test-client` 的 loaded 分页空态边界现在也已单独锁住：无论是 `thread/loaded/read` 还是 id-only 的 `thread/loaded/list`，即使当前页没有线程，compact summary 仍会继续保留 `no threads + next_cursor` 组合；这意味着连 id-only probe 自己在空页续翻时也不会把分页尾行吃掉
- `docs/remote-bridge-consumption.md` 也已继续对齐到同一术语：远端首页/列表现在被明确要求把 `Thread.mode` 直接映射成动作语义，`interactive -> resume`、`residentAssistant -> reconnect`，而不是只停在抽象的“线程模式不同”描述
- `docs/persistent-assistant-mode-design.md` 与 `docs/app-server-thread-mode-v2.md` 这两份前导设计稿也已补上同一条动作映射边界：虽然 `thread/resume` 仍是统一 API，但产品动作必须按 `Thread.mode` 显式映射成 `interactive -> resume`、`residentAssistant -> reconnect`
- `docs/observer-event-flow-design.md` 与 `docs/sqlite-state-convergence.md` 这两份状态文档也已继续跟上：它们现在都明确写出动作文案不是从 observer/status-only 通知或 SQLite 直接推断，而应继续从读取面拿到 `Thread.mode` 后稳定映射成 `interactive -> resume`、`residentAssistant -> reconnect`
- `exec` 的 bootstrap 启动摘要也已开始消费 `thread/start` / `thread/resume` 返回的 `Thread.mode`，resident assistant 不再在 CLI 启动横幅里被压平为普通 session，而会显式展示当前是 `resident assistant`
- `exec` 的人类可读 bootstrap 摘要现在也已继续补齐产品动作：除了 `session mode` 外，还会显式打印 `session action`，把 `interactive -> resume`、`resident assistant -> reconnect` 直接打到首屏配置摘要里，并补上 resident / interactive 两侧回归
- `exec` 的这组首屏摘要回归现在也已补上负向约束：当 bootstrap 里拿不到 `thread_mode` 时，`session mode` 和 `session action` 都会一起省略，避免未知线程被误贴上 resume/reconnect 语义
- `exec resume` 的人类可读 stderr 摘要现在也已补上端到端回归：真实命令输出会稳定带出 `session mode: interactive` 和 `session action: resume`，不再只在 `config_summary_entries` 单元层证明这组提示存在
- `exec resume <thread-id>` 的人类可读 stderr 摘要现在也已补上对应端到端回归：按显式 thread id 恢复普通 interactive 会话时，真实命令输出同样会稳定带出 `session mode: interactive` 和 `session action: resume`
- 普通 `codex exec` 的人类可读 stderr 摘要现在也已补上对应的端到端回归：fresh start 场景下，真实命令输出同样会稳定带出 `session mode: interactive` 和 `session action: resume`
- 普通 `codex exec --json` 的端到端回归现在也已把这层输出边界写死：fresh start 场景下，`thread_mode = interactive` 只通过首个 `thread.started` JSON 事件暴露，不会再额外把 `session mode/session action` 这组人类可读摘要打到 stderr
- `exec --json` 的首个 `thread.started` 事件也已开始透出 bootstrap `thread_mode`，避免下游 JSON 消费方只能拿到 `thread_id`，却继续把 resident session 当成普通 interactive thread
- `exec --json` 的这条 bootstrap 语义现在也已补上序列化回归：`thread.started` 在 resident bootstrap 时会稳定输出 `thread_mode = residentAssistant`，而未知模式时仍会省略该字段，避免 JSON 契约在后续重构里悄悄漂移
- `exec --json` 的 `thread.started` 形状现在也已补到更外层的集成测试：除了内部序列化单测外，面向公开 JSON 输出的集成回归也会直接断言 `type/thread_id/thread_mode` 这三个字段的实际 wire 形状，避免只在内部 helper 上通过却让对外输出悄悄漂移
- `exec --json` 的这层集成回归现在也已补上负向断言：当 bootstrap 里没有 `thread_mode` 时，公开 JSON 输出同样会稳定省略该字段，不会在未知模式场景下伪造 resident/reconnect 语义
- `exec resume --json` 的真实命令输出现在也已补上端到端回归：恢复普通 interactive 会话时，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = interactive`，不再只在 helper/集成层证明这条 wire 契约
- `exec resume --json` 的这条端到端回归现在也已把 stderr 边界写死：和普通 `exec --json` 一样，模式语义只通过首个 `thread.started` JSON 事件暴露，不会再额外把 `session mode/session action` 打到 stderr
- 普通 `codex exec --json` 的真实命令输出现在也已补上对应的端到端回归：fresh start 场景下，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = interactive`，不再只在 helper/集成层证明这条 wire 语义
- `exec resume --json` 的 resident 路径现在也已补到真实命令输出层：当线程模式先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = residentAssistant`，不再只在内部序列化单测里证明 resident wire 形状
- `exec resume --last --json` 的 resident 路径现在也已补上同层端到端回归：当最近会话先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，stdout 首个 `thread.started` 事件同样会稳定带出 `thread_mode = residentAssistant`，避免 `--last` 路径重新退回普通 interactive bootstrap 语义
- `exec resume --last` 的 resident 路径现在也已补上人类可读端到端回归：当最近会话先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，真实 stderr 摘要同样会稳定打印 `session mode: resident assistant` 和 `session action: reconnect`
- `exec resume --last --all --json` 的 resident 路径现在也已补上同层端到端回归：当跨 cwd 选择最终命中一个先通过 SQLite 稳定元数据恢复成 `residentAssistant` 的最近会话时，stdout 首个 `thread.started` 事件同样会稳定带出 `thread_mode = residentAssistant`，避免 `--all` 选择面重新退回普通 interactive bootstrap 语义
- `exec resume --last --all` 的 resident 路径现在也已补上人类可读端到端回归：当跨 cwd 选择最终命中一个先通过 SQLite 稳定元数据恢复成 `residentAssistant` 的最近会话时，真实 stderr 摘要同样会稳定打印 `session mode: resident assistant` 和 `session action: reconnect`
- `exec resume <thread-id>` 的 resident 路径现在也已补上人类可读端到端回归：当线程模式先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，真实 stderr 摘要会稳定打印 `session mode: resident assistant` 和 `session action: reconnect`
- `exec` 这两条真实 `--json` 首事件回归现在也已共用同一个 stdout 首行解析 helper，测试会更稳定地把“首个非空 JSON 行就是 `thread.started`”这条契约固定下来，而不是在每条用例里各自手写解析逻辑
- `exec` 的 `thread.started` 事件类型注释现在也已收口到真实行为：它不再只被描述成“new thread started”，而会明确写成 bootstrap 后的首事件，覆盖 fresh start 与 resume/reconnect 两种入口
- `exec` 的公开 JSON event 类型注释现在也已同步收口：`ThreadStartedEvent.thread_id` 不再只被描述成“later resume the thread”，而会明确写成后续可按 `thread_mode` 用于 `resume or reconnect`
- `exec` 的 `ThreadStartedEvent.thread_mode` 字段注释现在也已补齐到同一粒度：公开类型说明里会直接写清这就是下游区分 ordinary interactive resume target 与 resident reconnect target 的产品信号，不再只靠字段名暗示
- `codex-rs/README.md` 的顶层 `codex exec` 说明现在也已补上同一层脚本消费语义：README 会直接写明 `codex exec --json` 的首个 `thread.started` 事件会带 bootstrap `thread_id`，fresh start 时给出 `interactive`、resident reconnect 时给出 `residentAssistant`，未知模式时则显式省略该字段；同时也明确这层 bootstrap 元数据只走 JSON 事件面，而不是 stderr 的人类可读摘要
- `codex-rs/README.md` 的顶层 `codex exec` 说明现在也已把默认 human-readable 输出补齐：README 会直接写出非 `--json` 模式下的 bootstrap stderr 摘要包含 `session mode` / `session action`，用于区分普通 resume 与 resident reconnect
- `codex-rs/README.md` 的这句 human-readable bootstrap 说明现在也已继续精确化：interactive 一侧不再笼统写成 “ordinary interactive runs”，而会直接写成 ordinary interactive resume flows，让顶层 README 对 `session action = resume` 的解释更贴近当前 thread-mode 语义
- `debug-client` 也已开始消费 `Thread.mode`：连接成功提示、活跃线程切换提示、线程列表标记和 `:resume` 帮助文案都已按 resident assistant 收口，不再把 reconnect 路径统一描述成普通 resume
- `debug-client` 的线程列表摘要也已继续补齐：`:refresh-thread` 现在会同时显示线程模式标签、当前状态和推荐动作，不再只给出模糊的 `resume/reconnect` 动词，让 resident thread 与普通 interactive thread 在联调输出里更容易一眼区分
- `debug-client` 的 `:refresh-thread` 文本渲染现在也已补上整串回归：真实列表输出会稳定覆盖 header、每条 `thread-id + (mode, status, action)` 行以及 `next cursor` 尾行，不再只靠 `thread_mode_label` / `thread_resume_label` 这些 helper 侧面证明 resident reconnect 语义
- `debug-client` 的 `:refresh-thread` 空列表边界现在也已单独锁住：当当前页没有线程时，最终输出会稳定保留 `threads: (none)`，避免分页/过滤命中空页时重新退回泛化空白输出
- `debug-client` 的 resident 列表行文案现在也已补上最终字符串级断言：resident thread 会稳定渲染成带 `resident assistant + current status + reconnect` 的列表行，避免 mode/status/action helper 仍正确，但最终拼接到列表输出时又悄悄退回泛化 thread/reconnect 文案
- `debug-client` 的分页列表消费现在也已补成最小闭环：`:refresh-thread` 支持可选 cursor 参数，README 和 parser 回归也已同步补齐，看到 `next cursor` 后可以直接用 `:refresh-thread <cursor>` 续页，而不必切到其他联调客户端
- `debug-client` 的 `:refresh-thread` 请求侧闭环现在也已补上 cursor 文案回归：当用户传入续页 cursor 时，客户端请求日志会稳定写成 `requested thread list (..., cursor=...)`，避免 thread 列表只有响应侧能看见分页连续性，而请求侧调试日志继续退回不带 cursor 的泛化列表请求
- `debug-client` 的已知线程切换提示也已继续补齐：`:use <thread-id>` 在本地已缓存线程模式时，不只会明确区分 `thread` 与 `resident assistant thread`，还会带上最新已知 `status`，避免本地切换确认提示继续落后于 `:refresh-thread` 的 mode/status/action 摘要
- `debug-client` 的连接/就绪提示现在也已继续按 resident 模式收口：已 attach 的 resident thread 会稳定显示 `connected to resident assistant thread`，而对应 ready/action label 也会继续保留 `resident assistant thread` 与 `reconnect`，避免交互状态提示只在列表或切换提示里 resident-aware，而在真正 attach 后又退回泛化 thread/resume 文案
- `debug-client` 的这条消费面也已补回到可编译闭环：此前被截断的事件/帮助尾部逻辑已经恢复，并补上 resident reconnect 文案回归，避免这块客户端入口继续停留在“文案方向正确但 crate 本身不工作”的状态
- `debug-client` 的内置 `:help` 文案现在也已对齐 resident-aware 行为：`:use` 与 `:refresh-thread` 不再在帮助输出里退回旧的泛化 thread 说明，而会明确提示模式保留与 mode/status/action 摘要语义
- `debug-client` 的 clap 顶层 `--help` 现在也已补上 resident-aware 回归：不仅 `--thread-id` 会稳定写成 `resume or reconnect`，连 `--model` / provider / cwd 这些 start/resume override 说明也会继续保留 “starting/resuming or reconnecting” 口径，不再只靠交互内置 `:help` 和 README 承担这层收口
- `debug-client` 的 `:use` 帮助文案现在也已继续精确化：它不只会明确写成“switch active thread without resuming/reconnecting”，还会直接提示“preserve known mode/status when available”，避免本地切换活动线程的纯 selector 行为，被误读成也会立即 attach live thread 或触发 resident reconnect，同时也让帮助输出和当前本地状态缓存行为保持一致
- `debug-client` 的 fallback 提示现在也已继续收口到同一口径：当当前没有活跃线程，或只是本地切到一个未知 thread id 时，CLI 会稳定提示 `:resume` 可用于 `resume or reconnect`，不再把这类兜底提示写成模糊的 “load or reconnect” 文案
- `debug-client` 的 README 现在也已同步跟上这条 fallback 语义：文档会明确说明“尚未 attach 线程”和“`:use` 切到未知 thread id”这两种场景下，同样应通过 `:resume <thread-id>` 去 `resume or reconnect`，避免帮助输出与 README 再次分叉
- `debug-client` 的 `:resume` 缺参错误现在也已补上 resident-aware 用法提示：当用户只输入 `:resume` 时，CLI 会直接提示 `:resume <thread-id>` 可用于 `resume or reconnect`，不再只返回泛化的 `missing required argument: thread-id`
- `debug-client` 的 `:use` 缺参错误现在也已继续收口到同一条 selector 边界：当用户只输入 `:use` 时，CLI 会直接提示这是“without resuming/reconnecting”的本地活动线程切换，不再只返回泛化的缺参报错
- `debug-client` 的 README 现在也已把已知线程来源的口径写得更贴近实现：`:use <thread-id>` 说明不再笼统写成 `start/resume/list`，而会明确写成 `start, resume/reconnect, or list`，避免 resident reconnect 被 README 自己重新压回普通 resume 来源
- `app-server-test-client` 的 `thread/start`、`thread/resume`、`thread/list` 和 `thread/started` 输出也已补上 resident-aware 摘要，手工联调时不再需要从整段 debug struct 里自己辨认这是不是 reconnect 场景
- `app-server-test-client` 的最常用恢复入口现在也已各自补上命令级摘要断言：`thread/start`、`thread/resume` 和 `thread/fork` 这三条路径都会稳定打印各自的 resident reconnect label，不再只靠共享 helper 和 README 侧面兜底
- `app-server-test-client` 的 mode/action 摘要 helper 现在也已把 interactive 对照项单独锁住：普通线程会稳定落成 `mode=interactive ... action=resume`，也就是连 `interactive` 这侧模式标签本身都不会在后续重构里静默漂移，而不只是 `resume` 动作文案被顺带覆盖
- `app-server-test-client` 的 resident-aware 摘要回归现在也已把 `thread/started` 通知入口单独锁住：除了 `thread/read` 这类响应面外，通知摘要字符串同样会稳定打印 `mode=residentAssistant` 和 `action=reconnect`，避免通知入口在后续重构时悄悄退回普通 resume 文案
- `app-server-test-client` 的 README 现在也已同步补上这条通知侧契约：`thread/started` 在流式 start/resume 过程中同样会打印与响应面一致的 compact `mode + status + action` summary，不再只让代码和测试承担这层说明
- `app-server-test-client` 的联调入口也已继续扩到边缘恢复面：`thread-read` 与 `thread-metadata-update` 命令现在同样会输出 resident-aware 摘要，手工验证 `Thread.mode` 在只读 lookup 和 metadata-only repair 路径上的连续性不再需要额外脚本
- `app-server-test-client` 的另外三条主要 thread 恢复入口也已补齐 resident-aware 摘要：`thread-fork`、`thread-loaded-read` 和 `thread-unarchive` 现在同样会把 `mode + status + action` 直接打到联调输出里，避免 fork、loaded list 和 archived restore 这些路径继续退回手工读完整响应结构
- `app-server-test-client` 的命令级摘要回归也已继续补细：`thread/metadata/update` 与 `thread/unarchive` 这两条边缘恢复入口现在各自都有独立的 label 级断言，避免通用 helper 仍在但具体命令标签悄悄漂移
- `app-server-test-client` 的边缘恢复入口现在也已继续补到 `thread/rollback`：CLI 新增独立 `thread-rollback` 联调命令，会直接打印 `thread/rollback` 的 resident-aware compact summary，而且这条摘要字符串也已补上专门回归，避免 rollback 响应面在后续重构时重新丢掉 `mode=residentAssistant` / `action=reconnect`
- `app-server-test-client` 的单线程子命令帮助现在也已继续跟上这层输出契约：`thread-read`、`thread-fork`、`thread-unarchive` 与 `thread-rollback` 的 `--help` 会直接写出它们打印 compact `thread.mode + status + resume/reconnect action` 摘要，避免这些高频联调入口仍只在 README 或实际输出里隐含这层语义
- `app-server-test-client` 的列表摘要契约现在也已补成整串回归：`thread/list` 与 `thread/loaded/read` 的 summary header、每条 `mode + status + action` 行、空列表时的 `no threads` 标记以及分页 `next_cursor` 尾行都已被单独锁住，避免后续重构只剩单条 response/notification 覆盖
- `app-server-test-client` 的 README 说明也已继续对齐到实现现状：loaded / unarchive 这类边缘恢复命令同样会打印紧凑的 `mode + status + action` 摘要，不再只是笼统说“resident-aware”
- `app-server-test-client` 的 README 说明现在也已把 rollback 补进同一口径：`thread-rollback` 示例会明确说明它和 loaded / unarchive 一样打印 wire `thread.mode + status` 再附带派生 `resume/reconnect` action，手工验证 rollback 后的 resident continuity 不再需要读完整 debug struct
- `app-server-test-client` 的 README 还已补成更贴近实际输出的口径：摘要里打印的是 wire `thread.mode` 值（例如 `interactive` / `residentAssistant`）、当前 `status`，再配上 `resume/reconnect` 动作，不需要再靠读者自行猜测 mode 文本是不是产品化标签
- `app-server-test-client` 的 README 章节标题和示例 seed 文案现在也已继续从旧的 “rejoin” 术语收口到 `resume/reconnect`：手工联调说明会直接把这条流程表述成“thread resume/reconnect behavior”，避免外围读者继续把 resident reconnect 当成另一套语义漂移的旧命名
- `app-server-test-client` 的 README 流式说明现在也已继续补齐同一口径：`thread/started` 相关段落不再只写 `resume/start commands`，而会直接写成 `start/resume/reconnect commands`，让通知面文档和 resident reconnect 主线保持一致
- `app-server-test-client` 的流式通知面现在也已补到 `thread/status/changed`：客户端会缓存先前从响应面或 `thread/started` 拿到的线程摘要，因此后续 status 变更在已知 thread 上也能继续打印 compact `mode + status + action` summary；同时这层边界也已补上负向回归，确保未知 thread id 仍保持 status-only，而不会凭空猜 mode / reconnect 语义
- `app-server-test-client` 的这层通知消费现在也已抽成共享路径：不只是 turn 内的 `stream_turn(...)`，连 `thread-resume` / `watch` 这类持续观察入口也会复用同一套 resident-aware `thread/started` / `thread/status/changed` 摘要，避免“发起 turn 时有 compact summary，但纯观察模式又退回只剩原始 JSON”这类体验分叉
- `app-server-test-client` 的 clap 顶层 `--help` 现在也已补上 resident-aware 回归：`resume-message-v2` 与 `thread-resume` 这两个恢复入口的子命令说明会稳定体现 `resume or reconnect`，不再只让 README 或联调输出承担这层语义
- `app-server-test-client` 的这层 resident-aware 帮助回归现在也已明确是根帮助级别：`Cli::command().render_long_help()` 会直接锁住 `resume-message-v2` 与 `thread-resume` 在最外层命令总览里的 `resume or reconnect` 摘要，避免只覆盖子命令 help 后，顶层列表简介重新漂移
- `app-server-client` 的 README 与 typed request 回归也已开始明确 `Thread.mode` 是 bootstrap 阶段区分 reconnect 的权威来源；除了 metadata-only update 的 resident mode 保留覆盖外，`thread/loaded/read`、`thread/list`、`thread/unarchive` 和 archived 后的 `thread/read` 这几个只读恢复面也都已补上 resident assistant 的 typed request 回归，而且 archived resident thread 的 `thread/metadata/update` 也已补成专门回归，避免 in-process 集成到了 loaded/history/restore/archived-repair 路径后又退回通用线程摘要假设
- `app-server-client` 的 typed 分页读取面现在也已继续补齐：`thread/list` 与 `thread/loaded/read` 在 `limit = 1` 的分页场景下，会稳定返回并消费 `next_cursor`，而后续页上的 resident thread 也会继续保留 `ResidentAssistant`，避免 in-process 调用侧只在第一页锁住模式连续性、翻页后又退回通用线程摘要
- `codex-app-server-client` 的事件流边界现在也已补上直接回归：共享 in-process facade 的 `next_event()` 会继续把 `thread/started` 作为带 `mode + status` 的 resident-aware snapshot，而后续 `thread/status/changed` 仍只透出 status 增量；也就是说，这条“started 提供角色、changed 只推状态”的分工现在不只写在 README 里，也已经被客户端事件层测试锁住
- `codex-app-server-client` 的 typed 回归现在也已把 `thread/loaded/list` 这条 id-only probe 纳入闭环：in-process 请求会直接锁住 loaded ids 与 `next_cursor` 的续页连续性，同时 README 也明确这条接口只负责 loaded id 探针；如果 typed 调用方还需要 resident `mode`、当前 `status` 或 reconnect 语义，仍应继续读 `thread/loaded/read`
- `debug-client` 现在也已把 `thread/loaded/list` 暴露成真实调试入口：新增 `:refresh-loaded [cursor]`，输出会稳定列出 loaded thread ids 与 `next cursor`，并在 README / help / 回归里明确这只是 id-only probe；如果需要 mode/status/action、resident mode 或 reconnect 语义，仍应继续消费 `:refresh-thread` 或直接 `:resume`
- `debug-client` 的 `:refresh-loaded` 摘要契约现在也已补上字符串级断言：最终输出会稳定渲染 `loaded threads:` header、每条 loaded thread id 行、空列表时的 `loaded threads: (none)` 空态，以及 `more loaded threads available, next cursor: ...` 尾行，避免这条 id-only probe 在后续重构里悄悄丢掉分页提示或退回泛化列表文案
- `debug-client` 的 `:refresh-loaded` 请求侧闭环现在也已补上 cursor 文案回归：当用户传入续页 cursor 时，客户端请求日志会稳定写成 `requested loaded thread list (..., cursor=...)`，避免 loaded probe 只有响应侧能看见分页连续性，而请求侧调试日志继续退回不带 cursor 的泛化列表请求
- `debug-client` 的 reader 状态缓存现在也已跟上这条输出升级：`thread/list` 响应不再只写回 `thread_mode`，而会把 `thread_status` 一起落进 `known_threads` 并透传到 `ReaderEvent::ThreadList`，避免 `:refresh-thread` 的 mode/status/action 渲染只是显示层局部改动，却没有把 status 连续性锁进 reader/state 边界
- `debug-client` 的通知侧状态缓存现在也已补成闭环：除了 `thread/list` / `thread/start` / `thread/resume` 响应外，`thread/started` 与 `thread/status/changed` 也会继续刷新本地 `known_threads`；同时这层边界也已补上负向回归，确保 status-only 通知不会为未知 thread id 凭空推断 `mode`
- `debug-client` 的 client 状态读取面现在也已补上对称回归：`use_thread(...)` 在本地已知线程场景下不再只返回 `ThreadMode`，而会直接返回包含 `thread_mode + thread_status` 的完整缓存线程，避免 `:use` 提示虽然已经展示当前状态，但这层依赖仍只靠上层字符串测试间接覆盖
- `codex-tui` 的 `AppServerSession` 现在也已把 `thread/loaded/list` 纳入 typed 会话层边界：新增直接的 `thread_loaded_list(...)` 包装，并锁住 loaded ids 与 `next_cursor` 的续页连续性，避免 TUI 只覆盖 `thread/loaded/read` 这条带 mode 的恢复面，却让同层 id-only probe 在重构里静默漂移
- `codex-tui` 的 subagent backfill 边界现在也已在源码侧写清并补上纯函数回归：`backfill_loaded_subagent_threads` / `loaded_threads.rs` 明确说明这里仍必须继续消费 `thread/loaded/read`，因为 spawn tree、status badge 和 agent metadata 都不在 `thread/loaded/list` 的 id-only probe 里；相应测试也锁住“看起来像 loaded thread 但没有 spawn metadata 的条目不会被误认成子线程”
- `codex-tui` 的 latest-session lookup / resume picker 现在也已把 `include_non_interactive` 的 source filter 语义修正并锁住：由于 app-server 上 `source_kinds: None` 实际表示“interactive only”而不是“all sources”，TUI 现在会在 `resume --last` 和 picker 里显式传完整 `ThreadSourceKind` 列表；对应回归同时锁住默认过滤仍会忽略较新的 `Exec` 等非交互线程，而显式开启 include-non-interactive 后会正确选中更新更晚的 resident non-interactive thread 并保留 `ResidentAssistant`
- `codex-app-server-client` 的 in-process typed 回归现在也已把 resident `thread/unsubscribe` 的后续读取面锁住：最后一个订阅者断开后，`thread/loaded/read`、`thread/read` 与 `thread/resume` 仍会稳定保留 `ResidentAssistant + Idle`，避免共享客户端在“已断开但仍常驻”的路径上把 reconnect 目标重新降回普通 interactive 线程
- `thread/rollback` 的 resident 路径现在也已补上真实 typed 回归，并顺手修掉了实现层的模式回退缺口：rollback 完成后 app-server 不再只按 rollout summary 重建一个默认 interactive `Thread`，而会像其他读取面一样合并持久化 metadata，所以 resident assistant 在 `thread/rollback` 响应里也会继续稳定保留 `ResidentAssistant`
- `app-server/README.md` 的主接口说明也已继续收口：`thread/start`、`thread/resume`、`thread/fork`、`thread/list` 和 `thread/read` 都已明确把 `Thread.mode` 写成区分 resident reconnect 的主信号，而不是只讲 `resident: true`
- `app-server/README.md` 的 API Overview 方法行现在也已继续补齐到同一口径：`thread/resume` 不再只写成“reopen an existing thread”，而会直接写成 `resume or reconnect`，避免最常读的接口摘要再次落回旧的纯 resume 心智
- `app-server/README.md` 的 `thread/start` 摘要措辞现在也已继续统一：interactive 一侧不再笼统写成 “ordinary interactive session”，而会直接对齐到 `ordinary interactive resume target`，让 `thread/start`、`thread/resume` 与列表/通知面使用完全一致的 mode 对照术语
- `app-server/README.md` 的详细 `thread/resume` 章节现在也已补齐同一口径：正文不再写成 “normal interactive resume target”，而会统一对齐到 `ordinary interactive resume target`，避免 overview 和详细说明之间继续出现双轨术语
- `app-server-protocol` 里 `thread_history` 的源码注释现在也已把旧的 `resume/rejoin` 术语收口到 `resume/reconnect`：底层 replay reducer 的职责说明不再残留过时命名，避免协议实现层和对外文档继续使用两套词汇
- `codex-app-server` 的 `thread_resume` 集成测试命名现在也已继续去掉旧的 `rejoin` 术语：running-thread override mismatch 这条 resident continuity 回归已经改成显式使用 `reconnects`，避免测试层继续保留和产品/API 口径不一致的旧命名
- `app-server-protocol` 的 `Thread.mode` 源码注释与生成 schema 描述现在也已继续统一术语：它不再把 interactive 对照项泛化成 ordinary interactive session，而会直接写成 `ordinary interactive resume target`，让协议层的字段说明和 README / MCP 文档保持一致
- `app-server/README.md` 顶部 lifecycle overview 的总览入口也已补上 reconnect 语义，不再在首屏继续把 `thread/resume` 描述成单纯的普通历史恢复
- `app-server/README.md` 的 lifecycle overview 现在也已把 `mode` / `status` 分工写进首屏：无论是直接响应还是 `thread/started` snapshot，`thread.mode` 都是 reconnect 信号，而 `thread.status` 只描述当前 runtime 状态
- `app-server/README.md` 的恢复面示例 JSON 也已继续对齐：`thread/list`、`thread/loaded/read` 和 `thread/unarchive` 的示例结果现在显式带上 `mode`，避免正文强调 `Thread.mode` 是权威信号，但示例又把它省掉
- `app-server/README.md` 的 `thread/start`、`thread/resume` 和 `thread/fork` 主示例也已补成显式带 `mode` 的版本，避免最靠前的线程生命周期示例继续把 resident / interactive 语义藏在省略号里
- `app-server/README.md` 里的 `thread/started` 通知示例也已继续对齐：`thread/start` 和 `thread/fork` 后续通知现在同样显式带上 `mode`，不再只让响应面体现 resident / interactive 区分
- `app-server/README.md` 的 Events 总览现在也已把 notification 面的消费契约写明：凡是生命周期通知里附带 `thread` snapshot，都应直接消费其中的 `thread.mode`，而不是只把它当成请求响应侧的约束
- `app-server/README.md` 的 detached review 说明也已补上 `thread.mode` 消费约束：review fork 发出的 `thread/started` snapshot 同样应该直接读取 `mode`，而不是把这类 review 线程通知继续当成通用 resumed session
- `app-server/README.md` 的详细 `thread/status/changed` 说明现在也已把边界写清：这条通知只重复 runtime `status`，不会再次携带 `mode`；需要 reconnect 语义时，外围消费者应保留之前从 `thread/started` / `thread/loaded/read` / `thread/read` 拿到的 `thread.mode`
- `app-server/README.md` 的 API Overview 顶层摘要现在也已同步到同一口径：`thread/status/changed` 被明确标记为 status-only，`Thread.mode` 也被明确声明不能从 `thread.status` 反推
- `app-server/README.md` 的顶层方法摘要也已继续收口：`thread/unsubscribe` 现在不再只写“resident 不 unload”，而是明确后续读取面仍保留既有 `mode` 与 runtime 状态；`thread/rollback` 也已明确返回的 `thread` 会保留既有 `mode`
- `app-server/README.md` 的顶层 `thread/unarchive` 摘要也已补上同样口径：这条恢复路径返回的 `thread` 会保留既有 `mode`，不再只在下方详细示例里隐含体现
- `app-server/README.md` 的顶层 `thread/loaded/read` 摘要现在也已显式写出会返回当前 `mode`，避免 loaded 恢复面只在详细说明里体现 resident 语义
- `app-server/README.md` 的顶层 `thread/fork` 摘要也已把模式边界写清：源线程是 resident 不代表默认 fork 也变 resident，外围消费者应该以返回的 `thread.mode` 为准，而不是按来源线程心智继承
- `app-server/README.md` 的详细 `thread/list` 说明现在也已把 archived 列表的模式语义写回正文：即使 `archived: true`，外围消费者也应继续信任返回的 `thread.mode`，而不是把 archived 行默认为普通 interactive 历史
- `app-server/README.md` 的详细 `thread/loaded/list` 说明现在也已把边界写清：这条接口只是 id-only probe；如果外围还需要 reconnect 语义、线程角色或当前 runtime `status`，就应该继续调用 `thread/loaded/read` 并直接消费其中的 `thread.mode + thread.status`
- `app-server/README.md` 的详细 `thread/loaded/read` 说明现在也已明确：loaded polling 返回的不是无模式状态探针，而是可直接消费 `thread.mode + thread.status` 的恢复面
- `app-server/README.md` 的详细 `thread/fork` 章节现在也已把这条模式边界写回正文，而不再只靠示例里的 `interactive` 值暗示“resident 源线程默认不会把 fork 变成 reconnect 目标”
- `thread_status.rs` 也已补上 resident workspace watch 迁移/清理回归：同一线程切换 `cwd` 后旧工作区变化不会再重新激活 `workspaceChanged`，切回非 resident 后也会移除 watch，避免 observer 状态被陈旧目录继续污染
- observer 的读取面回归也已继续补齐：resident thread 发生工作区变化后，后续 `thread/read` 与 `thread/loaded/read` 都会稳定返回带 `workspaceChanged` 的 `status`，不再只有 `thread/status/changed` 通知面单独暴露这层 observer 事实
- observer 的摘要列表面也已补上同层回归：resident thread 命中工作区变化后，`thread/list` 返回的线程摘要同样会继续保留 `workspaceChanged`，避免列表面和 `thread/read` / `thread/loaded/read` 的状态入口重新分叉
- reconnect 响应面也已补上 observer 连续性回归：resident thread 保持 loaded 时命中工作区变化，后续 `thread/resume` 响应同样会继续返回带 `workspaceChanged` 的 `status`，避免 reconnect 入口重新退回成只会恢复 `mode`、却丢掉当前 observer 脏标记的半状态摘要
- `thread_status.rs` 现在也已把 `thread/status/changed` 的负向协议边界单独锁住：resident thread 命中 `workspaceChanged` 时，测试直接检查原始 notification payload 只带 `threadId + status`，不会偷偷重复 `mode`，避免 README 已写明的 status-only 契约在后续重构里被破坏
- `codex-tui` 的 `app_server_session.rs` 现在也已把 typed 读取面单独锁住：除了更上层的 latest/by-name/by-id lookup 回归外，`thread/read`、`thread/list` 与 `thread/loaded/read` 这三个直接供 TUI 会话层调用的恢复入口也都已有 resident 模式断言，避免中间层在后续重构里把 `ResidentAssistant` 静默压回通用线程摘要
- `codex-tui` 的 `AppServerSession` 现在也已把 live attach 的 `thread/resume` 路径补上真实回归：TUI 会话层直接发起 reconnect 时，返回的 `ThreadSessionState.thread_mode` 仍会继续保留 `ResidentAssistant`，避免消费侧只在 read/list 这类摘要面证明 resident continuity，却在真正 attach live thread 的入口重新退回通用 interactive 会话
- `codex-tui` 更靠近 UI 的 thread-read fallback 映射现在也已补上 resident 回归：`session_state_for_thread_read(...)` 在把 app-server 的线程摘要转成 TUI 本地 `ThreadSessionState` 时，会继续保留 `ResidentAssistant`，避免更上层 attach / replay 路径在消费 `thread/read` 摘要时重新把 reconnect 目标降回普通 interactive 会话
- `codex-tui` 的 `chatwidget` 现在也已把 resident UI label 自己单独锁住：`thread_mode_label()` 在 thread mode 为 `ResidentAssistant` 时会稳定显示 `"Resident assistant"`，避免状态栏和会话头这类最终展示面在后续重构里悄悄退回无模式标签
- `codex-tui` 的 `chatwidget` 会话注入路径现在也已补上 resident 回归：`handle_thread_session(...)` 在消费 `ThreadSessionState` 时，会继续把 `ResidentAssistant` 写回 widget 自身状态，避免上游会话层已经保留 resident 模式，但真正进入聊天视图时又被清成无模式
- `codex-tui` 的 `resume_picker` 现在也已把 resident 标签分支单独锁住：`thread_mode_label(ThreadMode::ResidentAssistant)` 会稳定返回 `"[assistant]"`，避免 picker 列表在继续保留 reconnect 动作文案的同时，模式徽标却在后续重构里静默消失
- `codex-tui` 的 `resume_picker` 上游 row 映射现在也已补上 resident 回归：`row_from_app_server_thread(...)` 在消费 app-server 线程摘要时，会继续保留 `mode = ResidentAssistant`，避免 picker 只是标签 helper 还在，但真正落到列表行模型时又退回普通 interactive 模式
- `codex-tui` 的 `resume_picker` 现在也已把 app-server `SystemError` 状态映射单独锁住：`row_from_app_server_thread(...)` 在消费 resident thread 的 `SystemError` 时，会继续保留 `ResidentAssistant` 并打上错误标记，避免 picker 列表只保留 reconnect 模式却静默丢掉线程错误态
- `codex-tui` 的 `resume_picker` 列表渲染面现在也已把 resident 徽标单独锁住：resident row 实际 render 到终端时会继续显示 `"[assistant]"`，避免 row 模型和 helper 都还保留 resident 语义，但最终列表展示在重构里静默丢 badge
- `codex-tui` 的 `resume_picker` 列表渲染面现在也已把 resident system error badge 单独锁住：resident row 命中 app-server `SystemError` 时，最终 render 到终端会继续显示 `"[error]"`，避免状态读取面还保留错误态，但列表展示在重构里静默丢掉错误提示
- `codex-tui` 的 `resume_picker` 入口标题现在也已补上 resident 回归：`SessionPickerAction::Resume.title()` 会稳定保留 `"Resume or reconnect to a previous session"`，避免列表页最外层标题在后续重构里退回旧的纯 resume 心智
- `codex-tui` 的 `resume_picker` hint 行现在也已补上 resident 回归：最终提示会稳定保留 `"enter to reconnect"`，避免 action label helper 仍然正确，但底部操作提示在重构里静默退回泛化的 resume 文案
- `codex-tui` 里一份停用的 `resume_picker` 旧 snapshot 残留也已一并清掉：被注释掉的示例测试文案和遗留 `.snap` 文件不再继续保留 `"Resume a previous session"` / `"enter to resume"` 这类旧口径，避免维护者从废弃测试资产里重新抄回过时措辞
- `codex-tui` 的 `resume_picker.rs` 里那整段长期注释掉的 `resume_picker_screen_snapshot` 草稿也已一并删除：这类既不运行、又重复手写 UI 布局的死代码不再继续滞留在测试模块里，避免后续维护者把它误当成仍待恢复的权威覆盖
- `codex-tui` 的 `resume_picker` 排序回归现在也已把另一条陈旧 TODO 收回来：此前被注释掉的 `resume_picker_orders_by_updated_at` 已恢复成真实测试，并对齐到当前 `RolloutConfig` / `SessionMeta` 接口；同时也补上了对称的 `CreatedAt` 回归，直接锁住 `UpdatedAt` 会按 rollout mtime 排序、而 `CreatedAt` 不会被 mtime 干扰
- `codex-tui` 的 tooltip 提示现在也已补上 resident 回归：`codex resume` 相关提示会稳定保留 `"resume or reconnect"` 措辞，避免最外围新手引导在后续调整里重新退回纯 resume 心智
- `codex-tui` 的底部 slash popup 现在也已补上 resident 回归：输入 `/res` 后 `/resume` 条目的渲染会稳定保留 `"resume chat or reconnect to resident assistant"`，避免底部命令选择器继续沿用旧的纯 resume 帮助文案
- `codex-tui` 的 rename 确认文案现在也已补上 resident 回归：线程重命名后的确认提示会稳定保留 `"to reconnect to this resident assistant run codex resume ..."`，避免聊天视图里的后续引导在重构里退回普通 thread resume 心智
- `codex-tui` 的 slash command 文案现在也已补上 resident 回归：`SlashCommand::Resume.description()` 会稳定保留 “resume chat or reconnect to resident assistant”，避免最外层帮助文案在重构时退回旧的纯 resume 心智
- `codex-tui` 的 resident reconnect history cell 现在也已补上无名字边界回归：`new_resident_thread_reconnected(...)` 在 thread name 为空时仍会保留通用的 resident assistant reconnect 文案，不会因为空白名字把提示渲染坏掉
- `codex-tui` 的主界面 session summary reconnect hint 现在也已补上快照回归：resident thread 的摘要卡片会稳定保留 `"To reconnect to this resident assistant, run codex resume ..."`，避免最终主界面上的收口提示在后续重构里退回普通 continue/resume 文案
- `codex-tui` 的 cwd prompt resident 文案现在也已补上最小回归：`CwdPromptAction::Reconnect` 的核心措辞会稳定保留 `"reconnect to"` 和 `"resident assistant"`，避免切目录提示在后续重构里退回普通 resume/fork 心智
- `thread_loaded_read.rs` 现在也已把 loaded polling 自己的 observer 连续性单独锁住：resident thread 命中工作区变化后，直接调用 `thread/loaded/read` 也会稳定返回带 `workspaceChanged` 的摘要，不再只靠 `thread_read.rs` 里的顺带覆盖来证明这条读取面成立
- `codex-app-server-client` 的 in-process typed 回归也已补到 observer 连续性：resident thread 命中工作区变化后，`thread/read`、`thread/loaded/read` 与 `thread/resume` 这三条 typed 请求都会继续保留同一个 `workspaceChanged` 状态，避免共享客户端在 reconnect 路径上只保留 `mode` 却丢掉 observer 脏标记
- `thread/unarchive` 也已开始复用 SQLite 稳定元数据来组响应：resident 线程反归档后不再因为只读 rollout 摘要而回退成 `interactive`，而且 `codex-app-server-client` 的 typed 回归现在也已把 `thread/unarchive`、后续 `thread/read` 与 `thread/list` 一起锁住，避免共享客户端只在反归档响应面保留 `ResidentAssistant`、却在随后的 stored 列表恢复面重新退回通用 interactive 摘要
- `app-server/README.md` 的详细 `thread/unarchive` 章节现在也已明确要求直接消费返回 `thread.mode`，恢复 resident thread 时不需要再额外补一次 `thread/read` 才能恢复 reconnect 语义
- `app-server/README.md` 的 `thread/unsubscribe` 说明也已继续补齐 resident 连续性：对 resident thread 来说，这条路径不只是“不会 unload”，而是后续 `thread/loaded/read`、`thread/read` 与 `thread/resume` 仍会继续暴露既有 `mode` 与 loaded/runtime 状态
- resident thread 的归档读取面也已补上回归：进入 archived 状态后，`thread/read` 与 `thread/list archived=true` 仍会稳定保留 `ResidentAssistant`，不会因为只走 SQLite 摘要路径就掉回普通 interactive 线程
- `codex-app-server-client` 的 typed archived 消费面现在也已跟上这条契约：无论是纯 archived read，还是 archived `thread/metadata/update` 之后的后续读取，`thread/list archived=true` 都会继续保留 `ResidentAssistant`，避免共享客户端只在 `thread/read` 上看见 resident continuity、却在 archived 列表面退回通用历史摘要
- `codex-tui` 的 `AppServerSession` 现在也已把 archived resident 读取面单独锁住：归档后的 `thread/read` 与 `thread/list archived=true` 都会继续保留 `ResidentAssistant`，避免 TUI 会话层只在普通 stored/loaded 路径上证明模式连续性，却在 archived 恢复面退回通用线程摘要
- `codex-tui` 更上层的 session lookup 现在也已把 archived resident 的 id 恢复路径锁住：按 thread id 查 archived thread 时，`lookup_session_target_with_app_server(...)` 仍会继续返回 `ResidentAssistant`，避免上层 selector 在 archived 路径上重新把 reconnect 目标压回普通 session target
- `app-server/README.md` 的 archive 段落现在也已把这点写明：虽然 archived thread 默认不会出现在 `thread/list`，但在 `thread/list archived=true` 与 `thread/read` 返回时仍会保留既有 `mode`
- `thread/metadata/update` 的 resident 回归也已继续扩到 stored + archived 面：未加载 resident thread 走纯 SQLite 稳定元数据路径时，更新响应与后续 `thread/read` / `thread/list` 会继续保持 `ResidentAssistant`；loaded resident thread 在修补元数据行后也不会因为 fallback bootstrap 路径丢掉 `ResidentAssistant`；已归档 resident thread 更新 git metadata 时，响应与后续 `thread/read` / `thread/list archived=true` 同样会继续保持 `ResidentAssistant`
- `codex-state` 的 SQLite 边界测试也已继续补强：`update_thread_git_info` 在 resident thread 上不会意外覆盖 `threads.mode`，避免元数据补丁把已持久化的 `ResidentAssistant` 降回 interactive
- `app-server/README.md` 也已把 `thread/metadata/update` 的 resident 语义补齐：API 说明现在明确要求返回的 `thread` 保留既有 `mode`，避免外围集成把 metadata-only update 错当成需要额外 `thread/read` 才能恢复 reconnect 语义的特殊路径
- `docs/app-server-thread-mode-v2.md` 现在也已同步跟上这些已落地边界：设计稿里明确写出 `thread/loaded/list` 只是 id-only probe，而 `thread/status/changed` 继续保持 status-only，`mode` 仍应来自 `thread/started` / `thread/read` / `thread/list` / `thread/loaded/read`
- `docs/persistent-assistant-mode-design.md` 这份更早的前导草案现在也已补上承接关系：它明确把 `thread/loaded/list`、`thread/loaded/read`、`thread/status/changed` 这些细边界交给 `app-server-thread-mode-v2.md` 作为当前权威协议说明，并同步写清 `thread/loaded/list` 不承担完整 loaded 恢复摘要语义
- `docs/observer-event-flow-design.md` 现在也已显式对齐协议边界：observer 事件虽然通过线程状态面进入客户端，但 `thread/status/changed` 继续只是 status-only 增量，线程角色仍应从 `thread/started` / `thread/read` / `thread/list` / `thread/loaded/read` 保留
- `docs/remote-bridge-consumption.md` 现在也已继续对齐到同一口径：远端控制面应把 `thread/loaded/read` 视为带 `mode + status` 的 loaded 恢复面，而把 `thread/status/changed` 视为不重复 `mode` 的 status-only 增量通知
- `docs/sqlite-state-convergence.md` 现在也已把这层边界补进状态分层：`thread/loaded/read` 属于当前线程摘要读取面，而 `thread/status/changed` 仍只是后续 status 增量，不应被当成需要落 SQLite 的完整线程恢复来源
- `docs/sqlite-state-convergence.md` 也已从纯前瞻草案补成“当前状态 + 渐进收敛”视角，明确把 `threads.mode`、archive / unarchive、metadata update resident 连续性这些已落地边界写进 SQLite 文档，不再只停留在泛化设计层
- `codex-rs/docs/codex_mcp_interface.md` 这份外围接口文档现在也已补齐同一消费约束：MCP 概览除了 `thread/start` / `thread/resume` / `thread/read` / `thread/list` 外，也已把 `thread/loaded/list`、`thread/loaded/read` 和 `thread/status/changed` 纳入主说明，并明确写出 `thread/loaded/list` 只是 id-only probe、`thread/loaded/read` 才是 loaded `mode + status` 摘要，而 `thread/status/changed` 继续只做 status-only 增量
- 到这一步，这条“`thread/loaded/list` 只是 id-only probe、`thread/loaded/read` 才是 loaded `mode + status` 恢复面”的文档链已经基本收口：debug-client / app-server-test-client 的 help 与 README、app-server / MCP / app-server-client 的外围说明、以及几份设计稿和跟踪文档都已统一到同一消费边界；后续更合适的动作应是整理提交或切回新的代码闭环，而不是继续在同层文档里做低收益扩写
- `debug-client` 的默认列表过滤现在也已补上 source-kinds 显式化：由于 app-server 上缺省 `source_kinds` 仍表示 interactive-only，`:refresh-thread` 与 `:refresh-loaded` 现在都会主动传完整 `ThreadSourceKind` 列表，并通过 README / 单测把“调试客户端默认看全量 interactive + non-interactive 线程来源”这层语义锁住，避免联调时把较新的 `Exec` 或其他非交互 resident 线程静默漏掉
- `debug-client` 的内置 `:help` 现在也已继续跟上同一边界：`:refresh-thread` 会明确提示它跨 interactive + non-interactive 来源列出带 mode/status/action 的线程摘要，而 `:refresh-loaded` 会明确提示它只是跨这两类来源的 loaded thread id-only probe，避免 README 已经收口后，交互内置帮助仍把这两条命令写成泛化列表入口
- `app-server-test-client` 的默认分页列表过滤现在也已补上同类修正：`thread-list`、`thread-loaded-read` 与 `thread-loaded-list` 不再依赖 app-server 的缺省 interactive-only `source_kinds` 语义，而会显式传完整 `ThreadSourceKind` 列表，并通过 README / 单测把“联调客户端默认看全量 interactive + non-interactive 线程来源”这层行为写死，避免手工验证 resident non-interactive 线程时被默认过滤误导
- `app-server-test-client` 的 clap 子命令帮助现在也已继续补齐同一口径：`thread-list` 与 `thread-loaded-read` 的 `--help` 会直接写出它们会打印 compact `thread.mode + status + resume/reconnect action` 摘要，而 `thread-loaded-list` 则会明确提示自己只是 id-only probe、需要 resident `mode`、当前 `status` 或 reconnect 语义时应继续读 `thread-loaded-read`，避免 README 已更新但联调入口帮助仍停在泛化列表说明
- 顶层 `codex --help` 现在也已继续保留 resident-aware 摘要：根命令帮助里的 `resume` 入口会稳定显示 `resume or reconnect` 与 picker/`--last` 提示，避免只锁住 `codex resume --help` 后，最外层命令总览里的用户可见简介重新漂移
- 顶层 `codex resume --help` 现在也已补上单独回归：最外层子命令帮助会稳定保留 `resume or reconnect` 和 `--include-non-interactive` 的说明，避免这条最靠前的用户入口只在结构体注释或总 help 里对齐 resident 语义，却在子命令 help 自己的回归层重新失守
- `codex-exec --help` 现在也已继续保留同类 resident-aware 摘要：根命令帮助里的 `resume` 入口会稳定显示 `resume or reconnect` 与 `--last` 提示，避免只锁住 `codex-exec resume --help` 后，独立二进制的总 help 简介再次退回旧语义
- `codex-exec resume --help` 现在也已补上对称回归：`exec` 子命令自己的帮助会稳定保留 `resume or reconnect` 与 `--last` 的 resident-aware 说明，避免这层入口只靠总 help 或结构体注释兜底
- 多工具包装层 `codex exec resume --help` 现在也已补上同类回归：顶层 `codex` 内嵌的 `exec resume` 子命令帮助会稳定保留 `resume or reconnect` 与 `--last` 的说明，避免只锁住独立 `codex-exec` 二进制后，主入口包装层的 clap 帮助在后续重构里悄悄退回旧语义
- 多工具包装层 `codex exec --help` 现在也已补上更上层摘要回归：这层帮助里 `resume` 子命令的摘要会稳定保留 `resume or reconnect` 与 `--last` 提示，避免只覆盖最里层 `exec resume --help` 后，外层命令列表里的用户可见简介重新漂移
- `codex-exec` 自己的最外层 `TopCli --help` 现在也已补上对称摘要回归：这层包装 CLI 里的 `resume` 子命令简介会稳定保留 `resume or reconnect` 与 `--last` 提示，避免只锁住 `codex-exec` 内层 `Cli::command()` 后，真正的二进制根入口帮助再次漂移
- `codex-exec` 的公开 JSON event 注释与顶层 `codex-rs/README.md` 现在也已继续统一术语：对外说明不再混用带连字符的 `resident-assistant` 变体，而会稳定写成 `resident assistant reconnect target`，减少脚本消费说明、README 和事件类型注释之间的措辞漂移
- `app-server/README.md` 的事件通知总览现在也已补上同一术语收口：`thread.mode` 的 reconnect 语义说明不再写成 `resident-assistant semantics`，而与其他 README / 注释统一成 `resident assistant semantics`
- `codex-exec` 的 bootstrap stderr 摘要也已在真实进程级回归里继续锁住：`exec/tests/suite/resume.rs` 不只检查 help 文案，而会直接断言 human-readable 模式下 interactive 路径输出 `session mode: interactive` / `session action: resume`，resident 路径输出 `session mode: resident assistant` / `session action: reconnect`，同时 `--json` 模式不会泄露这层 human summary
- 顶层 `codex` 的退出提示现在也已继续对齐 resident reconnect 语义：`format_exit_messages(...)` 在 thread mode 为 `ResidentAssistant` 时会稳定输出 `"To reconnect to this resident assistant, run codex resume ..."`，而普通 interactive thread 仍保持 `"To continue this session"`，避免主入口在最终收口提示上重新退回泛化 continue 文案
- 顶层 `codex-rs/README.md` 对 `codex exec` bootstrap summary 的说明现在也已与现有进程级回归形成闭环：README 里写明的 `session mode/session action` 区分，不再只是孤立说明，而是直接对应 `exec/tests/suite/resume.rs` 里 interactive/resident human-readable stderr 断言与 `--json` 负向断言
- `codex-tui` 的 `AppServerSession` 事件流边界现在也已补上直接回归：这个 typed 会话层自己的 `next_event()` 会继续把 `thread/started` 消费成带 `mode + status` 的 resident-aware snapshot，而后续 `thread/status/changed` 仍只透出 status 增量，避免这层边界只在更底下的 `codex-app-server-client` 或更上面的 `App` 集成测试里间接成立
- `codex-tui` 的 subagent backfill 现在也已把 source-kinds 边界和实现重新对齐：共享 helper 会统一产出 interactive-only 与全量 `ThreadSourceKind` 列表，`latest`/picker lookup 不会各自漂移，而 `backfill_loaded_subagent_threads(...)` 也会显式用全量 source kinds 调 `thread/loaded/read`，避免 app-server 的 interactive-only 默认过滤把 non-interactive subagent loaded threads 静默漏掉
- `codex-tui` 的显式名字恢复路径现在也已继续跟上同一条 `include_non_interactive` 边界：`codex resume <SESSION_NAME> --include-non-interactive` 不再只修好 picker / `--last` 两条路径，却在 by-name lookup 上继续落回 interactive-only 过滤；对应回归同时锁住“默认仍忽略 non-interactive named thread，而显式开启后会保留 resident target”
- `codex-tui` 的应用内 resume picker 现在也已补上同一条显式 source-filter 边界：不只是顶层 `codex resume --include-non-interactive` 能看见 resident / exec 这类 non-interactive 线程，TUI 内部通过 slash command 或快捷入口打开的 app-server picker 也能直接按 `i` 切换 interactive-only 与全量 source kinds；对应回归同时锁住请求参数重载和 hint 文案，避免应用内入口继续悄悄退回成“只有 CLI 包装层能显式包含 non-interactive”

这说明前面这条文档链已经基本完成当前轮次的收口：它不再只是“解释为什么应该做”，而是已经开始约束实现边界。

接下来的工作重点不应该再继续膨胀同层设计文档，而应该转向：

- 按 PR 边界收敛已经落地的协议、observer 和 SQLite 最小闭环
- 继续整理消费侧改动，也就是 TUI 或其他客户端对 `Thread.mode` 的正式使用
- 在合适时机把这一批 README / 设计稿同步改动整理成提交，而不是继续无边界扩写同层文档
- 再决定是否进入新的代码闭环，而不是回到同层大文档扩写

换句话说，后续主线已经从“拆文档”切换为“按实现计划逐阶段落代码”。

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
