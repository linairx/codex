# Resident Mode Baseline File TODO

本文承接：

- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是再定义 resident / mode 主线的边界，而是把首个“语义基线 PR”进一步压成文件级待办，方便直接开工。

## 0. 当前工作树对应关系（2026-04-14）

按当前本地改动集看，这个 TODO 里已经有一部分条目进入实际实现，另一部分仍更像提交前补扫项。

当前已经命中本地改动的主要文件或目录：

- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/src/lib.rs`
- `codex-rs/app-server-client/README.md`
- `codex-rs/app-server-test-client/src/lib.rs`
- `codex-rs/app-server-test-client/README.md`
- `codex-rs/debug-client/src/`
- `codex-rs/debug-client/README.md`
- `codex-rs/tui/src/app_server_session.rs`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/exec/`
- `codex-rs/cli/src/`

当前还没有进入本地改动、因此更适合视为剩余补扫项的入口：

- `codex-rs/README.md`
- 根 `README.md`

如果目标是尽快把这个 baseline PR 收成可提交状态，更合理的顺序是：

1. 先确认已改文件没有漏掉负向边界测试或 README/help 同步
2. 再补扫当前尚未进入改动集、但按清单仍应统一口径的入口

补充说明：

- 这份 TODO 当前已经不再只覆盖“服务端 + typed client + TUI 会话层”这一半条链；`resume_picker`、`chatwidget`、`exec` 与顶层 `codex` 包装层的 resident-aware help / hint / exit-summary 回归也都已进入本地改动集
- 因此更合理的默认动作已不是继续把这些入口当成待分诊项，而是：
  - 检查它们之间是否还残留语义分叉
  - 只把 README 与提交边界整理留作最后收尾项

## 1. 这个 TODO 要服务哪个 PR

它只服务同一个目标：

- resident / mode / status / source-kinds 语义基线 PR

如果后续工作已经进入：

- remote bridge 最小消费闭环
- observer 事件源收敛
- SQLite 权威来源收敛

那么应分别回到对应 checklist，而不是继续往这份 TODO 里加新任务。

## 2. 服务端读取面

### `codex-rs/app-server/src/codex_message_processor.rs`

待检查：

- `thread/read` 是否继续稳定合并 resident `mode`
- `thread/list` 是否继续稳定合并 resident `mode`
- `thread/resume` 是否继续返回 reconnect 所需的权威 `Thread`
- `thread/loaded/read` 是否继续承担 loaded `mode + status` 恢复面
- `thread/loaded/list` 是否没有被误扩成完整摘要
- repair / rollback / metadata update / unarchive 等边缘路径是否没有重新分叉出另一套 mode overlay

完成标志：

- 主要读取面和边缘恢复面在 resident continuity 上使用同一套语义

### `codex-rs/app-server/src/thread_status.rs`

待检查：

- `thread/status/changed` 是否继续只产生 status-only 增量
- observer 相关状态是否没有顺手把 `mode` 或完整摘要塞回通知
- resident thread 的 loaded / idle / active 状态是否继续与读取面一致

完成标志：

- 通知面仍保持增量职责，不重新承担恢复摘要职责

## 3. 服务端测试面

### `codex-rs/app-server/tests/suite/v2/`

待检查：

- `thread/read`
- `thread/list`
- `thread/resume`
- `thread/loaded/read`
- `thread/loaded/list`
- `thread/status/changed`
- `thread/metadata/update`
- `thread/unarchive`
- rollback 相关路径

需要优先补或确认的负向边界：

- `thread/status/changed` 不重复 `mode`
- `thread/loaded/list` 不补完整摘要
- resident thread 在 read / resume / repair 路径上不退回 interactive
- unknown thread id 不猜 resident mode

完成标志：

- 主要正向路径和最容易漂移的负向边界都已有回归保护

提交前更合适的最小验证命令：

- `cargo test -p codex-app-server --test all thread_`
- `cargo test -p codex-app-server --test all get_conversation_summary_by_thread_id_uses_loaded_external_rollout_path`

说明：

- 对 resident baseline，这两条命令比默认整份 `cargo test -p codex-app-server --test all` 更贴近真实主线
- 整份 `--test all` 仍可作为更大范围 smoke，但不应让与 `app/list` 等无关路径的波动重新打断这包提交级收口

## 4. typed client 面

### `codex-rs/app-server-client/`

待检查：

- typed 请求结果里 resident `mode` 是否继续稳定透传
- websocket remote facade 在 `thread/resume`、`thread/read`、`thread/list`、`thread/loaded/read` 上是否继续原样保留服务端返回的 `thread.mode + preview + status + path + name`
- `thread/loaded/read` 的返回值是否继续被消费侧当成当前 loaded thread 的权威摘要，而不是退回成还要额外补一次 `thread/read` 的半恢复接口
- `thread/loaded/list` 的 id-only probe 语义是否仍被明确保留
- README 是否继续把 `thread/resume` 写成 `resume or reconnect`
- 分页与 archived / repaired 路径是否没有重新退回通用 thread summary 心智

完成标志：

- typed client 不需要自己猜 resident reconnect 语义，也不会把服务端已修补的 `Thread` 降级成 probe + 二次补读

## 5. TUI 会话层和最终展示

### `codex-rs/tui/src/app_server_session.rs`

待检查：

- `thread/start` / `thread/resume` / `thread/fork` 的 resident mode 映射是否一致
- `thread/read` / `thread/list` / `thread/loaded/read` 的 typed 读取面是否一致
- `thread/loaded/list` 是否继续只作为 id-only probe 暴露

完成标志：

- TUI 会话层没有另一套 thread mode 解释逻辑

### `codex-rs/tui/src/resume_picker.rs`

待检查：

- resident badge / action label 是否继续稳定
- source-kinds interactive-only / 全量切换是否继续对应真实过滤逻辑
- 文案是否继续保持 `resume` / `reconnect` 对照

当前状态提示：

- 这一路径当前已不只锁住 resident reconnect；interactive / unknown 的 `resume` hint 对照项也已进入本地改动集
- 因此这里当前更像“提交前确认最终提示链已完整收口”的验证项，而不是默认缺口

完成标志：

- picker 仍然是最直观、最准确的 resident 入口之一

### `codex-rs/tui/src/chatwidget.rs`

待检查：

- 会话头、thread mode label、reconnect 提示是否继续保留 resident 语义
- observer / rollback / repair 之后进入聊天视图时是否没有把 resident mode 清掉

当前状态提示：

- 当前本地改动里，这一路径已经有 resident label、resident rename reconnect hint，以及 interactive / unknown rename follow-up `resume` 文案的直接回归
- 因此这里更值得确认的是“最终展示是否还有漏面”，而不是先假设文案仍未同步

完成标志：

- 最终展示层没有把上游已经保住的 resident mode 静默丢掉

## 6. CLI / Exec / 调试入口

### `codex-rs/cli/src/`

待检查：

- `resume` / `fork` help 是否仍与 resident/non-interactive 语义一致
- positional 参数是否继续使用 `SESSION_ID_OR_NAME` 一类准确命名
- exit summary 是否继续区分 continue / reconnect

当前状态提示：

- 当前本地改动里，顶层 `codex` 包装层已把 `resume` / `exec resume` 帮助里的 `SESSION_ID_OR_NAME` 和 resident-aware exit hint 一并锁住
- 因此这一块当前更像“README 和提交边界收尾项”，除非后续还要继续改主入口帮助文本

### `codex-rs/exec/`

待检查：

- bootstrap summary 是否继续区分 interactive / resident assistant
- human-readable 输出是否继续区分 resume / reconnect
- `--json` 输出是否不泄露 human summary 特有文案

当前状态提示：

- 当前本地改动里，`exec` 这条线已把 `resume or reconnect` 帮助、`SESSION_ID_OR_NAME`、`session mode/session action` 与 `thread.started` 回归继续锁紧
- 因此这里当前更像“提交级收口项”，不是默认还缺第一轮 resident-aware 收口

### `codex-rs/debug-client/`

待检查：

- thread list / attach / watch 等路径是否继续按 `Thread.mode` 输出 resident-aware 摘要
- unknown thread 的 status-only 通知是否不猜 mode

完成标志：

- 调试入口不再各自保留另一套旧语义

## 7. 文档同步面

### `codex-rs/app-server/README.md`

待检查：

- `Thread.mode`
- `thread/status/changed`
- `thread/loaded/list`
- `thread/loaded/read`
- `thread/resume`

这些段落是否继续说同一套话。

### `codex-rs/README.md` 与根 `README.md`

待检查：

- session management 入口是否仍与 resident reconnect 语义一致
- `resume` / `fork` / `--last` / `--include-non-interactive` 是否继续对齐当前实现

当前状态提示：

- 这两份 README 当前都已经能看到 `resume or reconnect` 与 `--include-non-interactive` 相关说明
- 因此更值得检查的是它们是否仍与最新实现保持一致，而不是默认还缺第一次同步

### 相关设计稿

最少应交叉检查：

- `docs/app-server-thread-mode-v2.md`
- `docs/persistent-assistant-mode-design.md`

完成标志：

- 最常见入口文档不再保留旧的纯 resume 心智

## 8. 建议提交前顺序

建议按这个顺序整理：

1. 先收服务端读取面和测试
2. 再收 typed client
3. 再收 TUI 会话层与最终展示
4. 再收 CLI / exec / debug-client 的用户可见语义
5. 再用 `docs/resident-mode-baseline-pr-template.md` 把当前这批改动整理成提交级 scope / contracts / tests / docs sync
6. 最后统一扫 README / help / 设计稿里的最小同步

这样更容易在每一层都看清“是不是还在说同一套话”。

如果走到第 5 步时发现当前 diff 已经明显混入 SQLite repaired-summary 收口或纯文档流程整理，更合理的动作不是继续硬收成单一 baseline PR，而是先回：

- `docs/persistent-runtime-current-worktree-pr-split.md`

## 9. 完成后应切到哪里

这份文件级 TODO 收完后，不应继续在这里追加 observer 或 SQLite 任务。

更合理的下一步是：

- 先用 `docs/resident-mode-baseline-pr-template.md` 整理当前 baseline 提交边界
- 如果这包已经可以提交，再回到 `docs/remote-bridge-minimal-consumption-checklist.md`

也就是先把第一个主包真正收成可 review 的 PR，然后再开始第二个主包，而不是继续把第一个主包无限细化下去。
