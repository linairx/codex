# Remote Bridge Minimal Consumption Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/remote-bridge-consumption.md`
- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是设计完整 bridge 产品，而是给“最小线程消费闭环”提供一份可直接执行的清单。

## 0. 当前本地进度快照（2026-04-12）

按当前工作树与已通过的本地测试看，这一阶段已经不再停留在“远端消费者理论上应该怎么做”，而是已有一条最小自愈闭环落地到现有调试/联调入口。

当前已落在本地改动与回归里的主要入口是：

- `codex-rs/debug-client/`
- `codex-rs/app-server-test-client/`
- `codex-rs/app-server/tests/suite/v2/thread_status.rs`
- `codex-rs/app-server-client/`
- `docs/`

当前已经可以明确视为本阶段已落地的点包括：

- `debug-client` 在收到未知 thread 的 `thread/status/changed` 时，仍会先保持 status-only 输出，不猜 `mode`；但随后会自动补发一次 `thread/loaded/read`，把后续本地摘要恢复到权威 `mode + status + action`
- `debug-client` 的这条补读链路现在也已补成真正的本地状态回收闭环：`thread/loaded/read` 回来后，resident `mode + status + name` 会重新落回 `known_threads`，而不只是打印一次刷新日志
- `app-server-test-client` 在同类 unknown-thread 状态通知路径上，也会先保持 status-only，再立即补一次 `thread/read`，避免长时间停留在“只知道 status、不知道动作语义”的半恢复状态
- `app-server-test-client` 的这条补读链路现在也已有单测锁住：未知 thread 的 `thread/status/changed` 触发后，会发出一次真实 `thread/read`，并把 resident `mode + status + name` 回收到本地 `known_threads`
- `app-server-client` 的 typed 请求/README 现在也已与这条消费契约对齐：调用方应直接信任 `thread/start` / `thread/resume` / `thread/metadata/update` / `thread/unarchive` / `thread/rollback` 返回里的 `thread.mode`，并继续把 `thread/status/changed` 当成 status-only 增量，而不是二次脑补 resident reconnect 语义
- `app-server-client` 这层 typed 契约现在也已继续补到两条先前更薄的边界：一是 in-process `thread/resume` 的 stored-summary repair 回归，二是 remote websocket facade 对 resident repaired `thread/resume`、`thread/read`、`thread/list` 与 `thread/loaded/read` 摘要的 typed 透传回归
- 这层 remote typed 契约现在也已继续补到请求侧：websocket facade 上 `thread/start` / `thread/resume` / `thread/fork` 在显式 `mode = residentAssistant` 时，不再回退去发送 legacy `resident` flag
- 这两条路径都已补上本地回归，并通过最小消费者层的单测锁住 unknown-thread 不猜 resident reconnect、但随后会回到读取面补 summary 这条契约

当前已完成并通过的本地验证包括：

- `cargo test -p codex-debug-client`
- `cargo test -p codex-app-server-test-client`
- `cargo test -p codex-app-server-test-client unknown_thread_status_change_refresh_round_trip_restores_known_thread_summary`
- `cargo test -p codex-app-server --test all thread_status`
- `cargo test -p codex-app-server-client --lib loaded_read_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib resident_workspace_changed_preserves_status_across_typed_reads_list_and_resume`
- `cargo test -p codex-app-server-client --lib resident_unsubscribe_preserves_mode_on_followup_reads_and_resume`
- `cargo test -p codex-app-server-client --lib archived_metadata_update_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib archived_read_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib unarchive_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib rollback_preserves_resident_thread_mode_through_typed_response_and_follow_up_reads`
- `cargo test -p codex-app-server-client --lib resume_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_serializes_mode_without_legacy_resident_flag`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_start_serializes_mode_without_legacy_resident_flag`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_fork_serializes_mode_without_legacy_resident_flag`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`

这意味着这份清单当前更适合继续承担：

- 检查剩余远端消费者是否仍沿同一条“status-only 增量 + 读取面恢复摘要”主线
- 确认后续 bridge 壳层不再重新发明另一套 unknown-thread 恢复逻辑

### 当前阶段判断

按第 8 节的完成定义看，这一包现在已经基本满足“阶段完成”条件。

当前已经可以相对明确地下这个判断，是因为：

- 最小远端消费者不再只停留在一个入口，而是已经覆盖 `debug-client`、`app-server-test-client`、`app-server-client`
- `thread/list` / `thread/read` / `thread/loaded/read` / `thread/status/changed` / `thread/resume` 这五条主消费路径都已有本地验证落点
- resident reconnect 的动作语义已经在 README、调试输出和 typed 返回面上保持一致

因此这份清单后续更适合作为：

- 后续 bridge 壳层开发时的回归对照表

而不是继续作为需要优先扩写的新任务包。

如果当前要做的已经不是“继续补最小消费闭环”，而是“把这包整理成单独 PR，或判断它是否已经和 observer / SQLite 混包”，更适合继续看：

- `docs/remote-bridge-minimal-consumption-pr-template.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

## 1. 这一阶段要解决什么

这一阶段的唯一主问题应该是：

- 让一个最小远端消费者稳定消费现有 thread summary / status 增量 / reconnect 语义

它适合覆盖的能力包括：

- 用 `thread/list` 渲染线程摘要
- unknown-thread 或缺摘要时，回到 `thread/read` 补权威当前摘要
- 用 `thread/loaded/read` 刷新 loaded thread 的 `mode + status`
- 用 `thread/status/changed` 消费 status-only 增量
- 用 `thread/resume` 进入 resident reconnect
- 把 `interactive -> resume`、`residentAssistant -> reconnect` 做成真实动作映射

它不应混入：

- 新 transport 协议设计
- observer 新事件类型
- SQLite schema 扩展
- 多 agent UI
- 完整网页 / 桌面产品壳层

## 2. 先读哪些文档和代码

建议按下面顺序进入：

1. `docs/remote-bridge-consumption.md`
2. `docs/app-server-thread-mode-v2.md`
3. `codex-rs/app-server/README.md`
4. `codex-rs/app-server/src/codex_message_processor.rs`
5. `codex-rs/app-server-client/`
6. `codex-rs/debug-client/`
7. `codex-rs/app-server-test-client/`

这些位置分别对应：

- 远端消费约束
- 线程模式契约
- 服务端接口说明
- 服务端读取面实现
- typed/in-process client
- 最小调试消费者
- 最小联调消费者

## 3. 开发时逐条检查什么

### 摘要来源

- 首页或列表摘要来自 `thread/list`
- loaded 运行态摘要来自 `thread/loaded/read`
- 不把 `thread/loaded/list` 当成完整摘要来源

### 增量来源

- `thread/status/changed` 只负责 status-only 增量
- 不从 `thread/status/changed` 里猜 `mode`
- 如果本地缓存里没有 thread summary，就回到读取面补 summary，而不是脑补 resident 语义

### 动作映射

- `interactive -> resume`
- `residentAssistant -> reconnect`

这条映射应直接来自 `Thread.mode`，而不是：

- source
- resident 布尔值
- 历史名字
- 某个局部 UI 状态

### reconnect 路径

- resident thread 统一通过 `thread/resume` 进入 reconnect
- reconnect 后仍以返回的 `Thread` 摘要作为权威启动状态
- 不要求 bridge 自己从 item 历史重建 resident 语义

## 4. 最值得补的负向边界

- 不把 `thread/status/changed` 当成完整摘要
- 不从 unknown thread id 的通知里猜 `mode`
- 不把 `thread/loaded/list` 当成带 `mode + status` 的接口
- 不把 `workspaceChanged` 简化显示成“线程仍在执行”
- 不把 resident reconnect 重新退回泛化的 resume / reopen 文案

## 5. 最适合先验证的消费者

第一批验证最好优先选这些现有入口，而不是先做新 UI：

- `app-server-test-client`
- `debug-client`
- `app-server-client` typed 请求

原因是它们已经足够证明：

- 现有 thread API 是否足以支撑远端消费
- 问题到底出在消费契约还是出在实现缺口

## 6. 最先跑哪些测试

优先跑这些：

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-debug-client`

如果改动主要落在 test client，也应补跑对应 crate 的测试。

## 7. Review 时最值得问什么

1. 这个改动是不是只做“最小线程消费闭环”
2. 摘要来源和增量来源是否仍保持分层
3. `Thread.mode` 是否仍是动作映射的唯一主信号
4. resident reconnect 是否仍统一走 `thread/resume`
5. unknown thread / empty page / 分页续页这些边界是否有测试
6. README / help / 调试输出是否与真实消费逻辑一致

## 8. 什么情况下这一阶段算完成

至少应满足：

- 一个最小远端消费者已经能稳定列出 resident 与 interactive 线程
- loaded 线程的当前 `mode + status` 能通过 `thread/loaded/read` 稳定刷新
- `thread/status/changed` 能作为后续 status-only 增量消费
- resident thread 的动作语义在消费者侧已稳定表现为 reconnect
- 调试/联调入口不再残留另一套 resume 语义
- 后续 bridge 产品壳层可以建立在这套消费闭环之上，而不需要重新解释 thread 语义

## 9. 这一阶段完成后下一步做什么

更合适的下一步是：

- observer 正式事件源收敛

因为只有当最小消费者已经成立，后续 observer 强化才更容易判断是在补状态来源，还是在替消费侧兜底。
