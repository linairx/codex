# Observer Event Source PR Template

本文承接：

- `docs/observer-event-source-checklist.md`
- `docs/observer-event-flow-design.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增 observer 设计结论，而是给“observer 事件源收口”提供一份可直接复制的 PR 描述模板。

## 模板

```md
## Summary

This PR tightens observer event-source semantics across watcher lifecycle, `workspaceChanged`
continuity, and the read/notification surfaces that expose observer-derived state.

It keeps `thread/status/changed` status-only, keeps `workspaceChanged` consistent across read
surfaces, and keeps resident watcher lifecycle aligned with shutdown / cwd migration / unsubscribe
semantics instead of reintroducing implicit state.

## Scope

This PR is intentionally limited to:

- watcher registration / migration / cleanup boundaries
- `workspaceChanged` set / retain / clear semantics
- observer continuity across `thread/read`, `thread/list`, `thread/loaded/read`, and
  `thread/resume`
- `thread/status/changed` remaining status-only even when observer state changes
- typed and TUI consumers continuing to read observer state from the same summary surfaces

Out of scope:

- new bridge transport or protocol work
- SQLite schema expansion
- new multi-agent product surfaces
- full event-bus redesign
- unrelated thread-mode wording work

## Contract Checks

This PR preserves the following contracts:

- `workspaceChanged` remains an observer-derived thread fact rather than a synonym for
  "still running"
- shutdown does not allow stale watcher events to reactivate `workspaceChanged`
- cwd migration does not let old workspaces continue to dirty the new thread state
- resident unsubscribe / reconnect preserves observer state when the thread stays loaded
- non-resident threads do not retain watcher state incorrectly
- `thread/status/changed` remains status-only and does not repeat `mode`

## Files / Areas

- `codex-rs/app-server/src/thread_status.rs`
- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/...`
- `codex-rs/app-server-client/...`
- `codex-rs/tui/src/app_server_session.rs`
- directly-adjacent observer/runtime docs

## Tests

Ran:

- `cargo test -p codex-app-server --test all thread_status`
- `cargo test -p codex-app-server --test all thread_metadata_update_repairs_loaded_resident_thread_without_losing_workspace_changed`
- `cargo test -p codex-app-server --test all thread_unsubscribe_keeps_resident_thread_loaded`
- `cargo test -p codex-app-server --test all resident_thread_preserves_workspace_changed_across_unsubscribe_and_resume`
- `cargo test -p codex-app-server moving_resident_watch_to_new_cwd_ignores_old_workspace_changes`
- `cargo test -p codex-app-server-client --lib resident_workspace_changed_preserves_status_across_typed_reads_list_and_resume`
- `cargo test -p codex-tui workspace_changed_status_is_consistent_across_read_list_and_loaded_read`

Key negative-boundary coverage:

- stale watcher events do not reactivate `workspaceChanged` after shutdown
- old cwd watchers do not dirty the new cwd after migration
- observer state is not dropped on resident unsubscribe/reconnect
- non-resident threads do not inherit resident watcher behavior
- `thread/status/changed` stays status-only

## Docs

Updated as needed:

- `docs/observer-event-flow-design.md`
- `docs/observer-event-source-checklist.md`
- directly-adjacent runtime / SQLite / client docs that mention `workspaceChanged`

## Why This PR First

This PR is meant to stabilize observer as a reliable state source before moving on to:

1. SQLite authority/source convergence
2. higher-level bridge shells
3. any future observer-specific product surfaces
```

## 当前工作树可直接使用的草稿

如果当前提交主要覆盖：

- `codex-rs/app-server/src/thread_status.rs`
- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/thread_status.rs`
- `codex-rs/app-server-client/`
- `codex-rs/tui/src/app_server_session.rs`
- 直接相邻的 observer / runtime 文档

那么下面这份草稿可以直接作为 observer 事件源收口 PR 的起点：

```md
## Summary

This PR tightens observer event-source semantics around `workspaceChanged`, watcher lifecycle, and
the status/read surfaces that expose observer state.

It keeps `thread/status/changed` status-only, keeps `workspaceChanged` consistent across
`thread/read` / `thread/list` / `thread/loaded/read` / `thread/resume`, and keeps resident
watchers aligned with shutdown, cwd migration, and unsubscribe semantics.

## Scope

This PR is intentionally limited to:

- watcher lifecycle boundaries in `thread_status`
- observer continuity on app-server read/reconnect surfaces
- typed and TUI consumer continuity for `workspaceChanged`
- negative boundaries around stale watcher reactivation, old cwd pollution, and resident
  unsubscribe/reconnect

Out of scope:

- new observer API
- SQLite schema changes
- bridge transport changes
- unrelated resident wording or help-text work

## Contract Checks

- `workspaceChanged` remains an observer-derived fact, not a disguised running state
- shutdown does not reactivate observer state from stale watcher events
- cwd migration removes the old watcher effect
- resident unsubscribe / reconnect preserves observer state when loaded
- `thread/status/changed` does not repeat `mode`

## Tests

Ran:

- `cargo test -p codex-app-server --test all thread_status`
- `cargo test -p codex-app-server --test all thread_metadata_update_repairs_loaded_resident_thread_without_losing_workspace_changed`
- `cargo test -p codex-app-server --test all thread_unsubscribe_keeps_resident_thread_loaded`
- `cargo test -p codex-app-server --test all resident_thread_preserves_workspace_changed_across_unsubscribe_and_resume`
- `cargo test -p codex-app-server moving_resident_watch_to_new_cwd_ignores_old_workspace_changes`
- `cargo test -p codex-app-server-client --lib resident_workspace_changed_preserves_status_across_typed_reads_list_and_resume`
- `cargo test -p codex-tui workspace_changed_status_is_consistent_across_read_list_and_loaded_read`

## Docs

Updated as needed:

- `docs/observer-event-flow-design.md`
- `docs/observer-event-source-checklist.md`

## Why This PR First

This PR proves observer state is already stable enough to act as a reliable source before SQLite
and higher-level bridge work depend on it.
```

## 使用建议

这份模板最适合配合下面两份文档一起使用：

- `docs/observer-event-source-checklist.md`
- `docs/observer-event-flow-design.md`

如果当前 worktree 已经明显混入 resident baseline、SQLite 收口或纯文档流程整理，再额外配合：

- `docs/persistent-runtime-current-worktree-pr-split.md`

更具体地说：

- checklist 用来定义 observer 边界
- flow design 文档用来定义状态来源与通知分层
- 这份模板用来把它们收成可提交文本
- 如果当前 diff 已经装不进单一 observer 包，就先回 worktree split 拆 PR，而不是继续硬扩这份模板

## 不适合把什么塞进这个模板

如果 PR 已经开始涉及下面这些内容，就不该继续用这份模板原样提交：

- 新 observer API
- 新 SQLite schema
- 新 bridge transport
- 完整事件总线重构

这意味着 PR 范围已经超出“observer 事件源收口”本身，应重新拆分或改用新的模板。
