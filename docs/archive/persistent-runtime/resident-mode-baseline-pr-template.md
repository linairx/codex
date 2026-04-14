# Resident Mode Baseline PR Template

本文承接：

- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/resident-mode-baseline-file-todo.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增设计结论，而是给首个 resident / mode 语义基线 PR 提供一份可直接复制的描述模板。

## 模板

```md
## Summary

This PR tightens the resident/mode/status/source-kinds semantic baseline across the main thread surfaces.

It does not introduce a new bridge transport, observer API, SQLite schema, or higher-level agent feature.

## Scope

This PR is intentionally limited to:

- resident `Thread.mode` continuity on main read/reconnect paths
- `thread/status/changed` remaining status-only
- `thread/loaded/list` remaining id-only
- `thread/loaded/read` remaining the loaded `mode + status` summary surface
- `interactive -> resume` / `residentAssistant -> reconnect` wording consistency
- top-level `codex`, `codex exec`, `resume_picker`, and `chatwidget` continuing to preserve that same wording at the final help/hint/exit-summary layer
- source-kinds omit/empty remaining interactive-only

Out of scope:

- remote bridge transport/product work
- observer event model expansion
- SQLite model/schema expansion
- multi-agent/planner behavior

## Contract Checks

This PR preserves the following contracts:

- `Thread.mode` remains the primary resident reconnect signal
- `thread/status/changed` does not repeat `mode`
- `thread/loaded/list` does not become a full summary surface
- `thread/loaded/read` remains the loaded `mode + status` summary surface
- resident `thread/resume` remains productized as reconnect
- metadata-only / restore paths keep returned `Thread` authoritative instead of requiring an extra `thread/read`
- websocket remote facade continues to preserve the server-returned `Thread` on `thread/resume`, `thread/read`, `thread/list`, and `thread/loaded/read`
- source-kinds omit/`[]` still means interactive-only

## Files / Areas

- `codex-rs/app-server/...`
- `codex-rs/app-server-client/...`
- `codex-rs/tui/...`
- `codex-rs/cli/...`
- `codex-rs/exec/...`
- relevant README/help/docs sync

If the concrete diff for this baseline pass is already mostly in high-level consumers, it is still acceptable to keep the PR narrowly scoped to:

- `codex-rs/tui/src/app_server_session.rs`
- `codex-rs/tui/src/app.rs`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/exec/...`
- `codex-rs/cli/src/main.rs`
- directly-adjacent docs/checklists

## Tests

Ran:

- `cargo test -p codex-app-server --test all thread_`
- `cargo test -p codex-app-server --test all get_conversation_summary_by_thread_id_uses_loaded_external_rollout_path`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-tui`
- `cargo test -p codex-exec`
- `cargo test -p codex-cli`

Prefer those targeted `codex-app-server` thread/conversation-summary commands over a blanket
`cargo test -p codex-app-server --test all` when the PR is only tightening resident/thread
semantics. That larger suite can fail on unrelated surfaces such as `app/list` force-refetch
behavior, which does not validate this baseline contract.

Key negative-boundary coverage:

- `thread/status/changed` stays status-only
- `thread/loaded/list` stays id-only
- unknown thread notifications do not guess resident mode
- resident read/resume/repair paths do not regress to interactive
- resident / interactive / unknown-mode final wording does not drift across TUI hints, chat follow-up hints, exec help, and top-level CLI help/exit summaries
- observer state is not reactivated by stale watcher events after shutdown

## Docs

Updated as needed:

- app-server README
- app-server-client README
- user-facing help/summary wording
- any touched metadata-only / restore docs continue to say clients can trust the returned `Thread`
- only the directly-adjacent design/protocol docs needed to stay accurate

## Why This PR First

This PR is meant to stabilize the local thread semantics baseline before moving on to:

1. remote bridge minimal consumption
2. observer event-source tightening
3. SQLite authority/source convergence
```

## 使用建议

这份模板最适合配合下面两份文档一起使用：

- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/resident-mode-baseline-file-todo.md`

如果当前 worktree 已经明显混入 SQLite repaired-summary 收口或纯文档流程整理，再额外配合：

- `docs/persistent-runtime-current-worktree-pr-split.md`

更具体地说：

- checklist 用来定义边界
- file todo 用来指导改哪些文件
- 这份模板用来确保 PR 描述和前两者保持同一口径
- 如果当前 diff 已经装不进单一 baseline 包，就先回 worktree split 拆 PR，而不是继续硬扩这份模板

## 当前工作树可直接使用的草稿

如果当前提交主要覆盖：

- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/src/lib.rs`
- `codex-rs/app-server-client/README.md`
- `codex-rs/tui/src/app_server_session.rs`
- `codex-rs/tui/src/app.rs`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/exec/`
- `codex-rs/cli/src/main.rs`
- 直接相邻的 runtime / bridge / baseline 文档

那么下面这份草稿可以直接作为首个 resident baseline PR 的起点：

```md
## Summary

This PR tightens the resident/mode/status/source-kinds semantic baseline across app-server,
typed clients, TUI session mapping, and top-level resume/reconnect entry points.

It keeps `Thread.mode` as the primary reconnect signal, keeps `thread/status/changed`
status-only, and keeps `thread/loaded/read` as the authoritative loaded summary surface
instead of reintroducing client-side guesswork or extra repair reads.

## Scope

This PR is intentionally limited to:

- resident `Thread.mode` continuity across `thread/read`, `thread/list`, `thread/resume`,
  and `thread/loaded/read`
- `thread/loaded/list` remaining id-only
- websocket remote facade continuing to preserve the server-returned repaired `Thread`
- TUI session/read fallback and final picker/chat wording continuing to map
  `interactive -> resume` and `residentAssistant -> reconnect`
- `codex exec`, top-level `codex`, and adjacent README/help text keeping that same wording

Out of scope:

- new remote bridge transport/product work
- observer API expansion
- SQLite schema/model expansion
- unrelated app-server surfaces such as app listing behavior

## Contract Checks

This PR preserves the following contracts:

- `Thread.mode` remains the primary resident reconnect signal
- `thread/status/changed` does not repeat `mode`
- `thread/loaded/list` remains an id-only probe
- `thread/loaded/read` remains the loaded `mode + status` summary surface
- resident `thread/resume` remains productized as reconnect
- metadata-only / restore / rollback paths keep returned `Thread` authoritative instead of
  requiring an extra `thread/read`
- websocket remote facade preserves the server-returned `Thread` on `thread/resume`,
  `thread/read`, `thread/list`, and `thread/loaded/read`
- source-kinds omit/`[]` still means interactive-only

## Tests

Ran:

- `cargo test -p codex-app-server --test all thread_`
- `cargo test -p codex-app-server --test all get_conversation_summary_by_thread_id_uses_loaded_external_rollout_path`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-tui`
- `cargo test -p codex-exec`
- `cargo test -p codex-cli`

Notes:

- For this baseline PR, the targeted `codex-app-server` thread/conversation-summary commands are
  the primary signal.
- A blanket `cargo test -p codex-app-server --test all` may still be useful as a broader smoke,
  but it can fail on unrelated surfaces such as `app/list` force-refetch behavior and should not
  block resident/thread baseline review by itself.

## Docs

Updated as needed:

- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/README.md`
- `codex-rs/README.md`
- `docs/exec.md`
- directly-adjacent baseline / remote-bridge / implementation-plan docs

## Why This PR First

This PR stabilizes the local resident/thread semantics baseline before moving on to:

1. remote bridge minimal consumption
2. observer event-source tightening
3. SQLite authority/source convergence
```

如果后续 diff 又开始混入：

- 新 bridge transport
- 新 observer API
- 新 SQLite schema
- 与 resident/thread baseline 无关的大范围 app-server 行为改动

那就不该继续把它当成这份草稿的同一个 PR。

## 不适合把什么塞进这个模板

如果 PR 已经开始涉及下面这些内容，就不该继续用这份模板原样提交：

- 新 bridge transport
- 新 observer API
- 新 SQLite schema
- 多 agent / planner 新能力

这意味着 PR 范围已经超出“resident baseline”本身，应重新拆分或改用新的模板。
