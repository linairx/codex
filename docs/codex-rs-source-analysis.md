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
- `thread_status.rs` 已开始收敛 watcher 生命周期，shutdown 会主动清理 resident thread 的工作区 watch
- `workspaceChanged` 的保留与清理语义已开始稳定：shutdown 后不会被陈旧 watcher 事件重新激活，下一次 turn 完成后会清掉该标记
- `tui` 已继续收敛消费侧语义：resume picker 的入口标题之外，新线程切换、fork 后摘要提示、线程改名确认提示、replay-only 的 agent thread 回退提示、resume 路径 attach 失败提示、切 cwd 后配置重建失败提示，以及 session 级恢复失败提示，也开始按 `Thread.mode` 区分 continue / resume 与 reconnect
- `cli` 的退出摘要提示也已开始消费 `Thread.mode`，resident assistant 不再一律提示 “continue this session”，而会明确给出 reconnect 语义

这说明前面文档链的作用已经完成了一半：它不再只是“解释为什么应该做”，而是已经开始约束实现边界。

接下来的工作重点不应该再继续膨胀同层设计文档，而应该转向：

- 按 PR 边界收敛已经落地的协议、observer 和 SQLite 最小闭环
- 继续整理消费侧改动，也就是 TUI 或其他客户端对 `Thread.mode` 的正式使用
- 再决定是否进入远端消费与 bridge 侧摘要设计，而不是回到同层大文档扩写

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
