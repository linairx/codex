# Claude 源码分析

本文基于本仓库 `claude` 分支中的 `claude/` 目录进行分析，对象是一个从 npm source map 还原出的 Claude Code TypeScript 源码树，而不是当前仓库主线的 Rust 实现。

## 结论摘要

- 这是一个规模很大的 Bun + TypeScript + React/Ink 终端应用，`src/` 下约有 2039 个源码文件。
- 它不是单一 REPL 程序，而是一个由 CLI 启动层、交互式 UI、查询执行器、工具系统、命令系统、MCP 集成、权限系统、远程桥接、插件与技能体系拼接而成的平台型客户端。
- 代码里大量使用 `feature('...')` 和 `process.env.USER_TYPE === 'ant'` 做编译期或运行时门控，说明内部版和外部版共享同一套代码基底，但功能暴露面差异很大。
- 这份源码是“可研究的还原工程”，不是完整无缺的官方开发仓库。`src/dev-entry.ts` 会先扫描缺失的相对导入；只有缺失数为 0 时才转发到真正的 CLI 入口。

## 1. 启动链路

启动路径可以概括为三层：

1. `package.json`
   `dev`、`start`、`version` 都指向 `bun run ./src/dev-entry.ts`。
2. `src/dev-entry.ts`
   这是还原仓库专用的保护入口。它会扫描 `src/` 和 `vendor/` 下的相对导入是否都能解析。
3. `src/main.tsx`
   当还原完整度足够时，`dev-entry.ts` 会转发到 `./entrypoints/cli.tsx`，再进入真正的 CLI 初始化流程。

这意味着当前 `claude` 目录的首要目标并不是“像正常应用一样直接启动”，而是先保证还原后的源码树结构完整。`dev-entry.ts` 本质上是一个恢复态工作区的看门人。

## 2. 顶层架构

从 `src/main.tsx`、`src/commands.ts`、`src/tools.ts`、`src/Tool.ts` 可以看出，Claude 的主干架构大致如下：

- CLI 引导层：解析参数、准备环境、初始化遥测、策略、认证、远程托管设置、MCP 配置、插件、技能。
- 会话层：维护会话 ID、模型选择、工作目录、会话恢复、远程会话、Teleport、Assistant/Kairos 等状态。
- 命令层：用 `/command` 的形式暴露用户能力，既有纯本地命令，也有把命令转换为 prompt 或工具链动作的复合命令。
- 查询层：`QueryEngine` 持有一段对话的状态，负责把用户输入、系统提示、工具调用、权限、使用量统计和消息持久化串起来。
- 工具层：Claude 的核心能力主要不是硬编码在主循环里，而是抽象成 `Tool` 集合，由模型按 schema 调用。
- UI 层：基于 React + Ink，`App.tsx` 提供 AppState、Stats、FPS 等上下文，终端界面是完整的组件树，不是简单 stdout 拼接。
- 集成层：MCP、Bridge、LSP、插件、技能、Webhook、远程控制等都作为横切能力接入。

这套结构说明 Claude Code 更像“面向模型的交互式代理运行时”，而不是传统的命令行助手。

## 3. 命令系统

`src/commands.ts` 是命令注册中心。几个特征非常明显：

- 命令量很大，既包括基础能力，也包括内部命令和实验命令。
- 命令不是全部静态暴露，很多命令受 `feature()` 或 `USER_TYPE` 控制。
- 命令来源不止内置代码，还包括技能目录、插件命令、内置插件技能。

从代码组织看，命令大致分成几类：

- 基础交互命令：如 `help`、`config`、`resume`、`model`、`permissions`、`theme`。
- 开发工作流命令：如 `review`、`diff`、`branch`、`pr_comments`、`tasks`、`files`。
- 平台扩展命令：如 `mcp`、`plugin`、`skills`、`hooks`。
- 内部门控命令：如 `teleport`、`autofix-pr`、`ant-trace`、`debug-tool-call`。
- 高阶实验命令：如 `assistant`、`brief`、`bridge`、`voice`、`ultraplan`、`buddy`。

命令系统的设计重点不是“把所有事做成 shell 子命令”，而是把不同入口统一成一套 REPL/Prompt/Local JSX 的交互协议。

## 4. 工具系统是核心

真正支撑 Claude 执行代码任务的是工具系统，而不是命令系统。

`src/Tool.ts` 定义了工具的公共类型，包括：

- 输入 schema
- 工具执行上下文 `ToolUseContext`
- 权限上下文 `ToolPermissionContext`
- UI 更新接口
- AppState 读写
- MCP、消息、通知、文件状态、预算、模型、系统提示等运行时依赖

`src/tools.ts` 则负责汇总当前环境下可用的全部工具。这里能直接看出 Claude 的能力边界：

- 文件系统：`FileReadTool`、`FileEditTool`、`FileWriteTool`、`NotebookEditTool`
- 代码检索：`GlobTool`、`GrepTool`
- 终端执行：`BashTool`、条件启用的 `PowerShellTool`
- 网络与检索：`WebFetchTool`、`WebSearchTool`
- 代理与协作：`AgentTool`、`SendMessageTool`、团队相关工具
- 用户交互：`AskUserQuestionTool`、`EnterPlanModeTool`、`ExitPlanModeV2Tool`
- 平台集成：`ListMcpResourcesTool`、`ReadMcpResourceTool`、`LSPTool`
- 任务管理：`TodoWriteTool` 或新版 task 系列工具

从实现方式看，Claude 明显采用“模型优先的工具编排”思路：

- 主循环把工具列表和 schema 暴露给模型。
- 权限系统在工具调用时做二次裁决，而不是只靠提示词约束。
- 工具既有执行逻辑，也有配套的 UI 组件和 prompt 文本。

这种设计比传统 CLI agent 更重，但扩展性强，适合持续叠加新能力。

## 5. QueryEngine 才是对话主循环

`src/QueryEngine.ts` 是最值得关注的核心文件之一。

它负责：

- 持有会话级消息历史
- 在每轮提交时组装上下文
- 接入系统提示、用户输入处理、memory prompt、插件缓存
- 执行查询 `query()`
- 跟踪工具调用、权限拒绝、文件读取缓存、使用量与成本
- 在需要时做消息压缩、会话持久化和 transcript 记录

从职责划分看，`QueryEngine` 更像“headless conversation runtime”。UI 可以围绕它构建，但不决定它的核心逻辑。

这很重要，因为它说明 Claude 的架构并不是把业务逻辑塞进 React 组件里，而是先有可复用的查询引擎，再把 REPL、SDK、子代理、后台模式挂接上去。

## 6. UI 不是附属层，而是完整终端应用

`src/components/App.tsx` 只是最薄的一层包装，但已经能看出 UI 的基本策略：

- 用 React/Ink 渲染终端界面
- 用 Context 提供状态、统计与 FPS 指标
- 用错误边界隔离启动期失败

从 `src/components/` 的规模也能看出，这个项目不是“命令执行后打印几行字”：

- 有大量对话、反馈、权限、Diff、状态、选择器、导出、Bridge、Desktop、Coordinator 相关组件
- 有独立的状态管理目录 `src/state/`
- 有输入框、状态栏、快捷键、视觉化面板等 UI 资产

因此它更接近一个终端里的应用框架，而不是普通 CLI。

## 7. 权限与安全控制占比很高

目录统计里，`permissions` 是最密集的子域之一。结合 `BashTool`、`PowerShellTool`、`ToolPermissionContext` 可以看出：

- Claude 不把 shell 能力当作“直接放行”的工具。
- 它有专门的权限规则、危险命令校验、路径校验、只读校验、自动模式状态、解释器与分类器逻辑。
- 权限决策不仅影响运行时，也会影响模型在当前回合能看到哪些工具。

这意味着其核心产品假设不是“模型天然可靠”，而是“模型能力必须被显式治理”。从工程角度看，这是整套系统能产品化的关键。

## 8. MCP、插件、技能构成扩展面

Claude 的扩展能力不是单一路径，而是三套并行机制：

- MCP：位于 `src/services/mcp/` 和相关工具中，负责外部 server、资源和工具接入。
- 插件：`src/plugins/` 与插件命令负责插件加载、缓存和命令扩展。
- 技能：`src/skills/` 与技能加载器把一组说明、脚本或可调用能力纳入 prompt 与命令生态。

这三者的区别大致可以理解为：

- MCP 偏“协议级能力接入”
- 插件偏“运行时扩展包”
- 技能偏“面向提示与任务组织的高层封装”

这也是 Claude 能不断增加工作流能力而不把所有逻辑塞回主仓主循环的原因。

## 9. Bridge、Assistant、Agent Swarm 说明它在往平台化发展

从目录和门控代码看，Claude 不只是本地单会话助手，还在持续向多形态运行时扩张：

- `src/bridge/`：远程桥接和状态同步，明显对应远程控制、本地 CLI 与 Web/其他端联动。
- `src/assistant/`：持久化助手模式，说明它支持超越单次 REPL 生命周期的长期会话。
- `src/tools/AgentTool/`、`src/utils/swarm/`、`src/coordinator/`：说明多 agent、团队协作、子代理、协调器模式已经是正式架构的一部分。

这里最重要的判断是：Claude 的长期形态不是“一个聊天框调用几个工具”，而是“一个可以切换运行模式的代理平台”。

## 10. 代码组织上的优点与问题

### 优点

- 分层清晰：命令、工具、服务、状态、UI、扩展机制分层明显。
- 可门控：大量特性通过 `feature()` 和用户类型控制，便于按版本或用户群裁剪。
- 可扩展：新工具、新命令、新集成点有比较固定的落位方式。
- 平台化能力强：MCP、插件、技能、Bridge、Agent 都不是临时拼接。

### 问题

- 规模极大，认知成本很高。
- 条件导入和特性门控非常多，导致“真实执行路径”不容易一次看清。
- 内部版与外部版混在同一源码树中，很多代码阅读时必须先分清可达性。
- 还原仓库额外叠加了“可能缺文件/缺导入”的不确定性，不能把每段代码都视为可直接运行。

## 11. 建议的阅读顺序

如果后续要继续深入分析 `claude`，建议按下面顺序读：

1. `claude/package.json`
2. `claude/src/dev-entry.ts`
3. `claude/src/main.tsx`
4. `claude/src/commands.ts`
5. `claude/src/Tool.ts`
6. `claude/src/tools.ts`
7. `claude/src/QueryEngine.ts`
8. `claude/src/services/mcp/`
9. `claude/src/tools/BashTool/`、`claude/src/tools/FileEditTool/`
10. `claude/src/bridge/`、`claude/src/assistant/`、`claude/src/tools/AgentTool/`

这个顺序能先建立全局脑图，再进入高价值子系统。

## 12. 总体判断

从工程形态看，这份 Claude 源码最接近：

- 一个终端内的 React 应用
- 一个面向 LLM 的工具执行运行时
- 一个带权限治理的代码代理平台
- 一个可通过 MCP、插件、技能和子代理持续扩展的宿主

如果只把它理解成“会调用 shell 的 CLI”，会严重低估它的复杂度。更准确的理解应该是：Shell 只是其中一个工具，Claude 真正的核心在于对话运行时、工具编排、权限治理和多模式会话管理。

## 附：本分析的依据

- `claude/package.json`
- `claude/src/dev-entry.ts`
- `claude/src/main.tsx`
- `claude/src/commands.ts`
- `claude/src/tools.ts`
- `claude/src/Tool.ts`
- `claude/src/QueryEngine.ts`
- `claude/src/components/App.tsx`
- `claude/src/` 目录结构与文件数量统计

