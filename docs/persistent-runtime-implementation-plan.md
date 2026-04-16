# 持久运行时实现计划

本文是当前 persistent-runtime 这条线的长期实现计划。

它只回答三个问题：

- 现在已经做到哪一步
- 接下来应该先做什么
- 哪些能力不该混在同一阶段

它不再回答：

- 当前 worktree 怎么拆 PR
- checklist / template / draft 怎么串
- 某一轮提交正文怎么起草

这些内容已经归档到：

- `docs/archive/persistent-runtime/`

## 1. 当前阶段判断

这条线已经不是纯设计状态。

目前可以视为“已进入实现阶段”的基础包括：

- `Thread.mode` 已进入 app-server 协议
- resident / reconnect 的基础语义已经进入主要读取面
- observer 的最小事件源已经存在
- SQLite repaired-summary 收口已经进入服务端与 typed client 的实现层
- subagent 与 planning 已有基础事件和局部消费面

但当前还不能说“persistent runtime 已经完成”。

真正还没完成的是高层产品模式：

- 持久助手模式
- 远程 bridge / 远程审批
- 更完整的多 agent 编排
- 长时规划模式
- 后台任务与异步结果体验

## 2. 当前阶段目标

下一阶段最合理的总目标是：

- 把现有 resident / thread mode / status / SQLite / observer 基础，推进成正式的持久助手模式

更具体地说，就是把“已经存在的一批机制”收成一个清晰、稳定、可被客户端直接消费的产品语义。

## 3. 分阶段计划

### 阶段 A：持久助手模式产品化

核心目标：

- 把 `residentAssistant` 从基础语义推进成正式产品模式

需要完成的事情：

- 明确线程生命周期：创建、断开、继续运行、reconnect、关闭
- 明确哪些状态是长期线程的正式状态面
- 明确 `waitingOnApproval`、`waitingOnUserInput`、
  `backgroundTerminalRunning`、`workspaceChanged` 这四类 active flag
  属于 reconnect 后仍应直接保留的线程级运行时事实，而不是临时 UI 状态
- 保持 `thread/read` / `thread/list` / `thread/resume` / `thread/loaded/read` 的线程摘要心智一致
- 让客户端不再需要自行脑补 resident 语义

不应混入：

- 完整远程 bridge 产品
- 多 agent 仪表盘
- 长时规划模式产品化

主要参考：

- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`

### 阶段 B：observer 状态面继续收敛

核心目标：

- 把 observer 从“已有事件源”推进成稳定状态来源

需要完成的事情：

- 稳定 `workspaceChanged` 的置位、保留、清理语义
- 保持 `workspaceChanged` 与其他 resident active flag 在 reconnect / typed
  读取面上的职责边界清晰：`workspaceChanged` 表示外部变化，
  `waitingOnApproval` / `waitingOnUserInput` / `backgroundTerminalRunning`
  表示仍然存在的运行时事实
- 明确 watcher 生命周期与 resident thread 生命周期的边界
- 保持读取面与状态通知的职责分离

不应混入：

- 原始文件变化全量持久化
- 新的复杂 observer API 面

主要参考：

- `docs/observer-event-flow-design.md`

### 阶段 C：SQLite 继续作为收敛层推进

核心目标：

- 继续把稳定元数据和 repaired-summary 收敛进 SQLite

需要完成的事情：

- 保持 stored summary / persisted metadata / runtime overlay 的分层清晰
- 继续消除读取面之间的 repaired-summary 分叉
- 只把稳定元数据和长期派生摘要收进 SQLite

不应混入：

- 全量运行态镜像
- 原始 watcher 事件日志化
- 客户端展示层拼接结果入库

主要参考：

- `docs/sqlite-state-convergence.md`

### 阶段 D：remote bridge / remote approvals

前提：

- A 基本稳定
- B、C 至少已经站住主干边界

核心目标：

- 让远端控制面真正建立在 thread summary / status / reconnect / approvals 之上

需要完成的事情：

- 正式远端控制面
- 远程审批回传
- 会话状态同步
- 后台状态和通知回流远端

不应混入：

- 一上来就做完整产品壳与复杂 UI
- 绕过 app-server 直接消费底层状态层

主要参考：

- `docs/remote-bridge-consumption.md`

### 阶段 E：多 agent 与长时规划产品面

前提：

- A、D 已稳定

核心目标：

- 把已有 subagent / plan 基底推进成真正的高层产品体验

需要完成的事情：

- coordinator / worker 产品模型
- 子代理状态可视化
- 结果汇总体验
- 规划与执行切换点

## 4. 当前最值得优先做的事

如果现在只能选一件事继续推进，优先做：

- 持久助手模式第一阶段

原因：

- remote bridge、多 agent、planning 都建立在“线程可长期存在且可恢复”这个前提上
- 这层不稳，后面几项都会停在半成品
- 这层里的关键完成标准之一，就是 reconnect 后仍能稳定保留关键 active
  flags，而不是把 resident 线程重新扁平化成普通历史恢复对象

## 5. 当前活文档入口

这条线现在更适合作为主入口保留的文档只有下面这些：

- `docs/codex-rs-source-analysis.md`
- `docs/codex-rs-roadmap.md`
- `docs/persistent-runtime-implementation-plan.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/remote-bridge-consumption.md`
- `docs/sqlite-state-convergence.md`

## 6. 什么时候该改这份实现计划

只有下面两种情况，才应该继续改这份文档：

1. 阶段边界真的变了
2. 某个高层能力已经从“设计/基础”进入“正式实现”，需要更新阶段判断

如果只是：

- 当前 PR 怎么拆
- 当前草稿怎么写
- 当前 worktree 哪几份 checklist 先看

不该继续改这份实现计划。
