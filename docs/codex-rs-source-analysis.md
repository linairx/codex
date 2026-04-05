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
