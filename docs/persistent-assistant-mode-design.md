# 持久助手模式设计草案

本文承接 `docs/codex-rs-source-analysis.md` 中第 22-28 节，目标不是重新分析 `codex-rs` 架构，而是把其中“持久助手模式”这个高优先级方向收敛成一份可以继续拆实现任务的设计草案。

## 1. 目标

“持久助手模式”要解决的核心问题是：

- 会话不应该绑定于某个前台 TUI 生命周期
- 用户离开后，线程仍可继续存在并保持可恢复语义
- 用户稍后回来时，拿到的不是“冷启动恢复历史”，而是“重新连接到仍然存在的助手线程”
- 后台执行、审批等待、用户输入等待、工作区变化等事实，应该通过统一线程状态面对外暴露

这项能力的重点不是“做一个守护进程”，而是把现有线程运行时、app-server 和状态语义产品化。

## 2. 非目标

第一阶段明确不做下面这些事情：

- 不做完整远程 bridge 产品
- 不做多 agent 仪表盘
- 不做长时规划模式产品化
- 不做 SQLite 全量状态统一
- 不做跨进程 daemon / supervisor / leader election
- 不引入新的全局任务系统

这些方向都可能与持久助手模式有关，但不属于第一阶段最小闭环。

## 3. 当前基础

从仓库现状看，这个方向已经有较多可复用基底：

- `codex-core` 已经有 `ThreadManager` 和 `CodexThread`
- `app-server` 已经支持 `thread/start`、`thread/resume`、`thread/list`、`thread/read`
- `thread/start` 和 `thread/resume` 已经有 `resident: true`
- `Thread` 已经暴露 `resident: bool`
- `ThreadStatus` 已经能表达 `NotLoaded`、`Idle`、`SystemError`、`Active`
- `Active` 已经支持 `waitingOnApproval`、`waitingOnUserInput`、`backgroundTerminalRunning`、`workspaceChanged`

因此第一阶段不需要发明新的线程抽象，重点是把已有机制上升为正式模式。

## 4. 产品定义

第一阶段建议把“持久助手模式”定义为一种显式线程模式：

- 它是长期驻留线程，而不是一次性前台会话
- 该线程可以在最后一个客户端断开后继续保持 loaded
- 客户端重新连接时，应把该线程视为仍在运行时语义中的会话
- 线程状态需要持续反映后台活动和等待状态

这里有一个重要区分：

- `resident` 是底层保活机制
- “持久助手模式”是面向产品与客户端的线程语义

第一阶段不应再让客户端自己从 `resident + status + source` 推断线程类型。

## 5. 最小协议建议

### 保留现有字段

第一阶段不建议移除：

- `Thread.resident`
- `ThreadStatus`
- `ThreadActiveFlag`

因为这些字段已经有现实语义和现有消费者。

### 增加线程模式字段

建议在 `Thread` 上增加一个正式模式字段，例如：

- `interactive`
- `residentAssistant`

后续如有需要，再扩展：

- `planner`
- `coordinator`
- `backgroundWorker`

第一阶段只要先把 `interactive` 和 `residentAssistant` 区分开即可。

### 模式和状态分离

协议上应坚持两个维度：

- 模式字段回答“这是什么线程”
- 状态字段回答“它现在在做什么”

不要把两者混成一个超大状态枚举，否则很快会失控。

## 6. 生命周期语义

第一阶段建议明确以下规则。

### 创建

- `thread/start` 可以创建普通交互线程，也可以创建持久助手线程
- 创建持久助手线程时，底层仍可复用 `resident: true` 机制
- 返回的 `Thread` 必须明确标记其模式

### 断开订阅

- 普通线程在最后一个客户端断开后，保持现有 unload / shutdown 行为
- 持久助手线程在最后一个客户端断开后，不应立即 shutdown
- 客户端断开不应破坏该线程的后续状态观察与恢复语义

### 重新连接

- `thread/resume` 对普通线程更偏向“恢复”
- `thread/resume` 对持久助手线程更偏向“重新连接到仍存在的线程”
- 如果客户端直接展示动作文案，默认映射也应固定为：
  - `interactive -> resume`
  - `residentAssistant -> reconnect`

这两个路径底层也许复用同一个 API，但产品语义和客户端展示应不同。

### 关闭

第一阶段仍允许通过现有机制显式关闭线程，但应把“驻留线程何时真正结束”定义清楚，例如：

- 显式关闭
- 明确的线程终止操作
- 进程退出

不要让它退化成“没人订阅一会儿就随机消失”。

## 7. 状态语义

第一阶段建议延续当前 `ThreadStatus` 结构，不急于重做。

### 建议保留的主状态

- `NotLoaded`
- `Idle`
- `SystemError`
- `Active { activeFlags }`

### 第一阶段需要保证的事实

- turn 运行中，线程进入 active
- 后台 terminal 存在时，即使 turn 已结束，线程仍能通过 active flag 体现
- 审批等待和用户输入等待不依赖客户端复盘 item 历史
- workspace 变化能作为线程级事实暴露出来
- 对持久助手线程来说，这四类 active flag 在 unsubscribe、transport disconnect
  以及后续 `thread/read` / `thread/loaded/read` / `thread/resume` reconnect 路径上
  都应尽量继续保留，只要对应事实仍然成立，就不应被提前压扁回 `idle`

### 一个应被文档化的边界

`workspaceChanged` 与“线程正在执行”不是同一种事实。  
因此客户端展示时，不应简单把所有 `Active` 都渲染为“正在工作”。

更合适的理解是：

- `Active` 表示线程当前存在值得前端关注的运行时事实
- 具体含义由 `activeFlags` 决定

## 8. 客户端行为建议

第一阶段至少要求 TUI 或其他 app-server 客户端做到：

- 在列表中区分普通线程与持久助手线程
- 能重新连接持久助手线程，而不是只把它当作旧会话恢复
- 能展示审批等待、后台 terminal、workspace 变化等状态
- 不要求客户端自己拼装复杂 item 历史来恢复线程状态

这意味着客户端应优先消费线程模式和线程状态，而不是试图从零散通知中反推。

这里也应明确一条 reconnect 产品心智：

- `waitingOnApproval`、`waitingOnUserInput`、`backgroundTerminalRunning`、
  `workspaceChanged` 都属于“线程仍然带着的运行时事实”
- 它们不是“只有当前连接在线时才临时存在的 UI 状态”
- 因此客户端在 reconnect 后应继续直接信返回的 `thread.status`，而不是把这些
  active flag 当作需要本地重建或事后猜测的显示结果

即使把第一阶段和后续 SQLite 收敛拆开看，这里也有一条应该尽早固定的读取面心智：

- `thread/read`、`thread/resume`，以及后续的 metadata-only / restore 路径返回的 `thread`
  都应逐步成为“客户端可直接信”的线程摘要
- 这条“直接信返回的 `Thread`”心智也应继续覆盖远端 typed facade：如果 websocket 远端直接返回 repaired `thread/resume`、`thread/read`、`thread/list` 或 `thread/loaded/read` 摘要，消费侧也不应重新退回“再补一次 `thread/read` 才算恢复完成”
- 如果服务端后面需要在 rollout 与 SQLite 之间做 stored-summary repair，这层修补也应尽量停留在服务端返回面，而不是重新推回客户端做额外 `thread/read` 或本地脑补

与这条读取面心智配套，第一阶段也应尽早固定通知边界：

- `thread/status/changed` 继续只做 status-only 增量
- `thread/closed`、`thread/archived`、`thread/unarchived` 继续只做 lifecycle
  边事件
- `thread/name/updated` 继续只做 name 增量
- 客户端和远端消费侧都应把这些通知应用到最近一次权威 `Thread` 摘要上，而
  不是期待这些通知本身重复 `mode`、`preview`、`resident` 或其他 repaired
  summary 字段

这里还应再固定一条更具体的消费约束：

- `thread/closed`、`thread/archived` 这类 lifecycle 边事件，不应被消费侧解释成
  “把这个线程从本地摘要缓存里删除”
- 更合理的做法是：继续保留最近一次权威 `Thread` 摘要里的 `name`、`mode`、
  `resident`、`preview` 等身份字段，只把当前 lifecycle / status 事实应用到
  这份 retained summary 上
- 这样 reconnect-aware 客户端在 close / archive 之后，仍能继续把该线程解释成
  resident assistant，而不是错误退化成“丢了角色信息的普通历史项”

换句话说，第一阶段虽然不把 SQLite 重构混进同一个 PR，但也不应给客户端留下“以后还得自己修 resident summary 漂移”的心智空档。

## 9. 实现范围建议

如果按 PR 拆分，第一阶段比较合理的拆法是：

1. app-server 协议补线程模式字段
2. app-server 在 `thread/start` / `thread/resume` / `thread/list` / `thread/read` / `thread/loaded/read` 上统一该字段
3. 客户端基于线程模式调整列表和恢复语义
4. 补充状态一致性和断连重连测试

observer、SQLite、bridge 都不应混进这一阶段 PR。

## 10. 验收标准

第一阶段可按下面标准判断是否完成。

### 协议

- `Thread` 能明确表达是否为持久助手线程
- 所有主要线程读取接口返回一致语义

### 生命周期

- 持久助手线程在最后一个客户端断开后不会被误 shutdown
- 普通线程的既有行为不被破坏
- 重新连接后，线程状态与保活语义保持连续
- 重新连接后，如果线程仍在等待审批、等待用户输入、持有后台 terminal，
  或工作区已有外部变化，这些事实会继续通过 `activeFlags` 暴露

### 客户端

- 客户端能区分普通线程与持久助手线程
- 客户端能在稍后重新看到关键等待和后台状态

### 范围控制

- 没有顺手引入新的大状态枚举
- 没有顺手引入 daemon 架构
- 没有把 observer 和 SQLite 重构混入同一阶段

## 11. 后续文档

在这份草案之后，更合理的下一批文档是：

1. `app-server v2` 线程模式字段与兼容性说明
2. observer 事件流设计
3. SQLite 状态汇聚设计
4. 远程 bridge 如何消费长期线程状态

这样可以把产品模式、协议、事件源、状态收敛和远端消费逐层拆开。

其中第一份可落成：

- `docs/app-server-thread-mode-v2.md`

目前这份 v2 协议文档已经进一步细化了几个实现边界：

- `thread/loaded/list` 只是 id-only probe，不承担完整 loaded 恢复摘要语义
- `thread/loaded/read` 是带 `mode + status` 的 loaded 恢复面
- `thread/status/changed` 继续保持 status-only，不重复 `Thread.mode`
- metadata-only / restore 这类路径返回的 `Thread` 也不应被例外对待；后续即使补上 stored-summary repair，客户端仍应继续直接信这些返回面，而不是额外补一次 `thread/read`

再下一份更底层但仍应独立拆出的文档是：

- `docs/observer-event-flow-design.md`

之后才是状态收敛层文档：

- `docs/sqlite-state-convergence.md`

当协议、observer、SQLite 三份设计文档都就位后，下一步应补：

- `docs/persistent-runtime-implementation-plan.md`
