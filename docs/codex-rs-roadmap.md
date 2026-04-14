# codex-rs 路线图

本文是当前 `codex-rs` 的长期路线图，只保留三类信息：

- 哪些基础已经落地
- 哪些高层能力仍未完成
- 下一阶段最合理的优先级

它不记录：

- 某一轮 PR 拆包
- checklist / template / draft
- 单次工作树的提交流程

这类过程性材料已经移到：

- `docs/archive/persistent-runtime/`

## 1. 当前已落地的基础

### 线程模式与线程状态

协议层已经有：

- `ThreadMode::{Interactive, ResidentAssistant}`
- `ThreadActiveFlag::{WaitingOnApproval, WaitingOnUserInput, BackgroundTerminalRunning, WorkspaceChanged}`

这意味着：

- “这是什么线程”
- “它现在在做什么”

已经有正式的协议表达。

### resident continuity 与恢复面

`thread/read`、`thread/list`、`thread/resume`、`thread/loaded/read` 等读取面已经在逐步对齐：

- resident 语义
- reconnect 心智
- loaded vs persisted 的边界

### SQLite repaired-summary 收口

SQLite 已经不是空设计，而是正在承担稳定元数据与 repaired-summary 收敛层的职责。

### observer / fs-watch 基础

协议层已经有：

- `fs/watch`
- `fs/unwatch`
- `fs/changed`

这足以作为 observer 的第一阶段事件源。

### subagent 与 planning 基底

仓库已经有：

- `spawn_agent`
- subagent metadata / notifications
- `PlanDeltaNotification`
- `TurnPlanUpdatedNotification`

这说明多 agent 和 planning 都有基础，但还不是完整产品面。

### experimental feature 装配

协议与 TUI 已有实验功能列表和开关能力，说明 feature organization 已有基础形态。

## 2. 当前仍未完成的高层能力

### 1. 正式持久助手模式

当前最接近“下一阶段主线”的能力。

仍缺：

- 面向产品而不是实现细节的长期线程定义
- 明确的断开、继续运行、reconnect、关闭语义
- 统一后台状态与通知模型

### 2. 完整远程 bridge / 远程审批

当前已有 app-server 和最小远端消费基础，但还没有完整远端控制面。

仍缺：

- 远程控制面产品闭环
- 审批从本地回传到远端
- 会话与后台状态同步体验

### 3. 更完整的多 agent 编排体验

当前已有 subagent 基础，但缺：

- coordinator / worker 的正式产品模型
- 多 agent 列表和任务面板
- 子代理状态和结果汇总体验

### 4. 长时规划模式

当前有 planning 事件流，但缺：

- planning mode
- 规划与执行切换点
- 长时计划审阅/修改/批准语义

### 5. 后台任务与异步结果体验

当前有状态位，但缺：

- 任务面板
- 通知中心
- 稍后恢复查看结果的统一路径

## 3. 当前优先级

最合理的顺序：

1. 持久助手模式产品化
2. 远程 bridge / 远程审批
3. 多 agent 编排体验
4. 长时规划模式
5. 后台任务与异步结果体验

原因很简单：

- remote bridge、多 agent、planning 都建立在“线程可长期存在且可恢复”这个前提上
- 如果持久助手模式没先做扎实，后面几项都会停在半成品状态

## 4. 当前仍然建议保留的设计稿

下面这些文档仍然是有效入口：

- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/remote-bridge-consumption.md`
- `docs/sqlite-state-convergence.md`
- `docs/persistent-runtime-implementation-plan.md`

## 5. 下一阶段建议

如果现在回到真实开发，最值得先推进的是：

### A. 持久助手模式第一阶段

目标：

- 把 `residentAssistant` 从“基础语义”推进成正式产品模式
- 明确 reconnect、后台运行、状态展示、关闭语义

### B. remote bridge 最小产品闭环

前提：

- A 基本稳定

目标：

- 让远端控制面能稳定消费 thread summary、状态增量、reconnect 语义和审批状态

### C. 多 agent 体验

前提：

- A、B 已建立稳定线程语义

目标：

- 把当前已有的 subagent 基底变成真正可见、可管理、可汇总的产品能力

## 6. 如何判断路线图需要更新

只有下面两类情况，才应该改这份路线图：

1. 某个高层能力真实落地，完成度判断需要更新
2. 优先级发生变化

如果只是：

- 某一轮 PR 怎么拆
- 某一包 checklist 怎么写
- 某个提交正文怎么组织

不该改这份路线图。
