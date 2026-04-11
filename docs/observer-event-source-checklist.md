# Observer Event Source Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/observer-event-flow-design.md`
- `docs/remote-bridge-minimal-consumption-checklist.md`

目标不是再解释 observer 为什么有价值，而是把“把 observer 从状态标记推进成正式事件源”收成一份可直接执行的清单。

## 1. 这一阶段要解决什么

这一阶段的唯一主问题应该是：

- 让 observer 成为可解释、可消费、可测试的正式状态来源，而不只是零散挂在 `ThreadStatus` 上的附着物

它适合覆盖的内容包括：

- watcher 注册、迁移、清理边界
- `workspaceChanged` 的置位、保留、清理语义
- resident thread 生命周期与 watcher 生命周期对齐
- `thread/read` / `thread/list` / `thread/loaded/read` / `thread/resume` 之间的 observer 一致性

它不应混入：

- 新 bridge transport
- 新 SQLite schema 扩展
- 新的多 agent 任务面板
- 复杂索引系统
- 完整 event bus 重构

## 2. 先读哪些文档和代码

建议按下面顺序进入：

1. `docs/observer-event-flow-design.md`
2. `docs/sqlite-state-convergence.md`
3. `codex-rs/app-server/src/thread_status.rs`
4. `codex-rs/app-server/src/codex_message_processor.rs`
5. `codex-rs/app-server/tests/suite/v2/`
6. `codex-rs/tui/src/app_server_session.rs`
7. `codex-rs/app-server-client/`

这些位置分别对应：

- observer 设计边界
- 与 SQLite 的状态分层边界
- watcher / thread 状态实现
- 读取面与 repair / resume 入口
- 集成测试入口
- TUI typed 消费层
- in-process client 消费层

## 3. 开发时逐条检查什么

### watcher 生命周期

- watcher 是在哪个时点注册的
- resident thread 切换 `cwd` 时 watcher 如何迁移
- thread unload / shutdown / unsubscribe 时 watcher 如何清理
- 非 resident thread 是否会被错误地保留 watcher

### `workspaceChanged` 语义

- 在什么条件下置位
- 在什么条件下清理
- 它何时只表示“有外部变化”
- 它何时不应该被误解为“线程仍在执行”

### 读取面一致性

- `thread/read` 是否继续保留 observer 状态
- `thread/list` 是否继续保留 observer 状态
- `thread/loaded/read` 是否继续保留 observer 状态
- `thread/resume` 是否继续保留 reconnect 时的 observer 状态

### 通知面边界

- `thread/status/changed` 是否继续只做 status-only 增量
- observer 相关通知是否没有开始承担完整摘要恢复职责
- observer 状态是否仍需要由读取面提供完整上下文

## 4. 最值得补的负向边界

- shutdown 后陈旧 watcher 事件不会重新激活 `workspaceChanged`
- 切换 `cwd` 后旧工作区变化不会污染新工作区状态
- unsubscribe 后 resident thread 的 observer 状态不会丢
- non-resident thread 不会错误保留 watcher
- `thread/status/changed` 不会因为 observer 强化而开始偷偷重复 `mode`

## 5. 最先跑哪些测试

优先跑：

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-tui`

如果改动主要集中在 watcher 状态机，也应先跑最聚焦的 `app-server` 测试子集，再扩大到相关消费侧。

## 6. Review 时最值得问什么

1. 这个改动是不是只在收敛 observer 状态来源
2. watcher 生命周期是否比以前更清楚，而不是更隐式
3. `workspaceChanged` 的语义是否更清楚，而不是更混杂
4. 读取面之间是否继续一致
5. `thread/status/changed` 是否仍保持 status-only
6. 有没有补足 shutdown / cwd 迁移 / resident unsubscribe 这类最容易破的负向测试

## 7. 什么情况下这一阶段算完成

至少应满足：

- watcher 注册、迁移、清理边界已经清楚
- `workspaceChanged` 的置位和清理语义已经清楚
- 主要读取面上的 observer 状态一致
- reconnect 路径不会丢 observer 状态
- `thread/status/changed` 仍保持 status-only
- TUI / app-server-client 不需要再各自猜 observer 状态来源

## 8. 这一阶段完成后下一步做什么

更合适的下一步是：

- SQLite 状态分层与权威来源收敛

因为只有 observer 先成为稳定状态来源，后续才更容易判断哪些状态真的值得持久化，哪些只适合停留在 runtime 或 event stream。
