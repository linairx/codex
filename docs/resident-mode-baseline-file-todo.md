# Resident Mode Baseline File TODO

本文承接：

- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/persistent-runtime-checklists-index.md`

目标不是再定义 resident / mode 主线的边界，而是把首个“语义基线 PR”进一步压成文件级待办，方便直接开工。

## 0. 当前工作树对应关系（2026-04-11）

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

当前还没有进入本地改动、因此更适合视为剩余补扫项的入口：

- `codex-rs/cli/src/`
- `codex-rs/exec/`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/README.md`
- 根 `README.md`

如果目标是尽快把这个 baseline PR 收成可提交状态，更合理的顺序是：

1. 先确认已改文件没有漏掉负向边界测试或 README/help 同步
2. 再补扫当前尚未进入改动集、但按清单仍应统一口径的入口

补充说明：

- 这些“尚未进入本地改动集”的入口并不等于“当前一定缺语义”
- 按当前仓库内容看，其中不少文件已经包含 `resume or reconnect`、`resident assistant`、`SESSION_ID_OR_NAME` 或 `session mode/action` 相关口径
- 因此它们当前更适合作为“验证并决定是否还需补改”的入口，而不是默认必须再开一轮改动

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

## 4. typed client 面

### `codex-rs/app-server-client/`

待检查：

- typed 请求结果里 resident `mode` 是否继续稳定透传
- `thread/loaded/list` 的 id-only probe 语义是否仍被明确保留
- README 是否继续把 `thread/resume` 写成 `resume or reconnect`
- 分页与 archived / repaired 路径是否没有重新退回通用 thread summary 心智

完成标志：

- typed client 不需要自己猜 resident reconnect 语义

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

- 这一路径当前已能看到 resident reconnect 文案与对应测试
- 因此更像“确认无需继续补改”的验证项，而不是默认缺口

完成标志：

- picker 仍然是最直观、最准确的 resident 入口之一

### `codex-rs/tui/src/chatwidget.rs`

待检查：

- 会话头、thread mode label、reconnect 提示是否继续保留 resident 语义
- observer / rollback / repair 之后进入聊天视图时是否没有把 resident mode 清掉

当前状态提示：

- 源码中已能看到 resident thread mode、resume/reconnect picker 注释和 reconnect 文案
- 因此这里更值得确认的是“读取/恢复后有没有静默丢 mode”，而不是先假设文案仍未同步

完成标志：

- 最终展示层没有把上游已经保住的 resident mode 静默丢掉

## 6. CLI / Exec / 调试入口

### `codex-rs/cli/src/`

待检查：

- `resume` / `fork` help 是否仍与 resident/non-interactive 语义一致
- positional 参数是否继续使用 `SESSION_ID_OR_NAME` 一类准确命名
- exit summary 是否继续区分 continue / reconnect

当前状态提示：

- 当前源码和测试里已经能看到 `resume or reconnect`、`SESSION_ID_OR_NAME`、`--include-non-interactive` 与 resident reconnect exit hint
- 因此这一块更像“确认现有语义无需再补改”，除非本轮 baseline PR 还要继续触碰壳层 help 或退出摘要

### `codex-rs/exec/`

待检查：

- bootstrap summary 是否继续区分 interactive / resident assistant
- human-readable 输出是否继续区分 resume / reconnect
- `--json` 输出是否不泄露 human summary 特有文案

当前状态提示：

- 当前源码、README 与测试里已经能看到 `thread_mode`、`session mode`、`session action`、resident reconnect 与 `--json` 边界
- 因此这里当前更像“回归验证项”，不是默认还缺第一轮 resident-aware 收口

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
5. 最后统一扫 README / help / 设计稿里的最小同步

这样更容易在每一层都看清“是不是还在说同一套话”。

## 9. 完成后应切到哪里

这份文件级 TODO 收完后，不应继续在这里追加 observer 或 SQLite 任务。

更合理的下一步是：

- 回到 `docs/remote-bridge-minimal-consumption-checklist.md`

也就是开始第二个主包，而不是继续把第一个主包无限细化下去。
