# Resident Mode Baseline PR Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-implementation-plan.md`

目标不是再解释 resident / mode 主线为什么重要，而是给首个“语义基线 PR”提供一份可直接执行、review 和提交的清单。

## 0. 当前本地进度快照（2026-04-11）

按当前工作树看，这个 baseline PR 已经明显进入“收尾而不是起步”阶段。

当前已落在本地改动集里的主要入口是：

- `codex-rs/app-server/`
- `codex-rs/app-server-client/`
- `codex-rs/app-server-test-client/`
- `codex-rs/debug-client/`
- `codex-rs/tui/src/app_server_session.rs`
- `docs/`

当前尚未进入本地改动集、但仍值得作为收尾检查项的入口是：

- `codex-rs/cli/src/`
- `codex-rs/exec/`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget.rs`

这意味着这份清单当前更适合被用作：

- 检查已经落地的语义是否真正闭环
- 快速识别还有哪些用户入口尚未完成同口径收尾

补充说明：

- 这些未进入当前改动集的入口不应自动视为“尚未支持 resident-aware 语义”
- 按当前仓库内容看，其中不少入口已经具备 `resume or reconnect`、resident reconnect 或 `session mode/action` 相关口径
- 因此更合理的做法是先验证这些入口是否仍与当前实现一致，再决定是否真的需要补改

## 1. 本 PR 要解决什么

这个 PR 的唯一主问题应该是：

- 收口 resident / mode / status / source-kinds 的语义基线

它适合覆盖的内容包括：

- `Thread.mode` 的读取面一致性
- `thread/status/changed` 的 status-only 边界
- `thread/loaded/list` 的 id-only probe 边界
- `thread/loaded/read` 的 loaded `mode + status` 恢复面语义
- `interactive -> resume` / `residentAssistant -> reconnect` 的动作映射
- source-kinds omit / `[]` 代表 interactive-only 的默认值

它不应混入：

- 新的 remote bridge transport
- 新的 observer 对外 API
- 新的 SQLite schema 设计
- 多 agent / planner 新能力
- 与线程语义基线无关的大 UI 重排

## 2. 先读哪些文件

建议按下面顺序进入代码和文档：

1. `codex-rs/app-server/README.md`
2. `codex-rs/app-server/src/codex_message_processor.rs`
3. `codex-rs/app-server/src/thread_status.rs`
4. `codex-rs/app-server/tests/suite/v2/`
5. `codex-rs/app-server-client/README.md`
6. `codex-rs/tui/src/app_server_session.rs`
7. `codex-rs/tui/src/resume_picker.rs`
8. `codex-rs/tui/src/chatwidget.rs`
9. `codex-rs/cli/src/`
10. `codex-rs/exec/`

这些位置分别对应：

- 服务端协议消费
- 服务端读取面与状态面实现
- typed client
- TUI 会话层
- 最终用户文案

## 3. 开发时逐条检查什么

### 读取面

- `thread/read` 仍把 `Thread.mode` 作为线程角色主信号
- `thread/list` 和 `thread/read` 的 resident / interactive 语义一致
- `thread/loaded/read` 仍承担 loaded `mode + status` 恢复面
- `thread/loaded/list` 仍只是 id-only probe
- `thread/resume` 在 resident thread 上继续表达 reconnect 语义

### 通知面

- `thread/status/changed` 仍只承载 status-only 增量
- `thread/status/changed` 不重复 `mode`
- `thread/status/changed` 不承担完整 loaded summary 恢复职责

### source-kinds 默认值

- omit / `[]` 仍表示 interactive-only
- 没有入口偷偷变成“空值 = 全量 source kinds”
- CLI / TUI / debug-client / test-client 的帮助文案与真实过滤逻辑一致

### 用户可见动作语义

- `interactive -> resume`
- `residentAssistant -> reconnect`

这条映射应在下面入口保持一致：

- CLI help
- CLI exit summary
- TUI picker / 状态文案
- app-server README
- app-server-client README
- test-client / debug-client 输出

## 4. 最值得补的负向边界测试

这类 PR 最需要的不是更多 happy path，而是防边界漂移：

- `thread/status/changed` 不重复 `mode`
- `thread/loaded/list` 不补完整摘要
- unknown thread id 的状态通知不猜 resident mode
- resident thread 不会在 read / resume / repair 路径上退回 interactive
- observer 不会在 shutdown 后被陈旧 watcher 重新激活

如果改动碰到了这些边界，却没有对应测试，这个 PR 通常还不够稳。

## 5. 最先跑哪些测试

优先跑这几个 crate：

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-tui`
- `cargo test -p codex-exec`

如果只改了局部路径，可以先跑更聚焦子集；但这四个 crate 是最容易暴露语义漂移的地方。

## 6. Review 时该怎么问

建议按下面顺序 review：

1. 这个 PR 是否只解决一个主问题
2. 读取面是否仍一致
3. 通知面是否仍保持分层
4. source-kinds 默认值是否仍一致
5. 用户动作语义是否仍一致
6. 负向边界测试是否充分
7. 文档同步是否完成

如果前四项里任一项回答不清，这个 PR 通常不应直接合并。

## 7. 什么情况下这个 PR 算完成

至少要满足：

- `Thread.mode` 在主要读取面上稳定一致
- `thread/status/changed` 仍严格保持 status-only
- `thread/loaded/list` 仍严格保持 id-only probe
- `thread/loaded/read` 仍承担 loaded `mode + status` 恢复面
- `thread/resume` 在 resident thread 上仍明确表示 reconnect
- `app-server-client`、`tui`、`cli` / `exec`、调试入口没有继续残留旧语义
- 相关 crate 的测试已覆盖
- 主要 README / help 已同步
- 合并后 remote bridge / observer / SQLite 收敛更容易独立推进

## 8. 这个 PR 合并后做什么

最佳下一步是：

- remote bridge 的最小线程消费闭环

原因很简单：

- 只有 resident / mode 语义基线先稳下来，后续 bridge 才能更容易分辨问题来自消费侧还是来自线程主线本身。
