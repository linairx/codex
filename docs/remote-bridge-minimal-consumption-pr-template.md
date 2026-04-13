# Remote Bridge Minimal Consumption PR Template

本文承接：

- `docs/remote-bridge-minimal-consumption-checklist.md`
- `docs/remote-bridge-consumption.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增 bridge 设计结论，而是给“最小线程消费闭环”提供一份可直接复制的 PR 描述模板。

## 模板

```md
## Summary

This PR tightens the minimal remote thread-consumption contract across existing bridge-facing
consumers.

It keeps thread summaries sourced from `thread/list` / `thread/read` / `thread/loaded/read`,
keeps `thread/status/changed` status-only, and keeps resident reconnect semantics driven by
`Thread.mode` plus `thread/resume` instead of client-side guesswork.

## Scope

This PR is intentionally limited to:

- list summaries continuing to come from `thread/list`
- unknown-thread or missing-summary paths continuing to recover via `thread/read`
- loaded runtime summaries continuing to come from `thread/loaded/read`
- `thread/status/changed` remaining status-only
- resident reconnect continuing to go through `thread/resume`
- existing minimal consumers (`debug-client`, `app-server-test-client`, typed app-server client)
  continuing to map `interactive -> resume` and `residentAssistant -> reconnect`

Out of scope:

- new transport or protocol design
- observer API expansion
- SQLite schema/model expansion
- full desktop/web bridge product work
- unrelated TUI or CLI UX rewrites

## Contract Checks

This PR preserves the following contracts:

- `thread/list` remains the primary list summary surface
- `thread/read` remains the authority-first fallback for unknown-thread or missing-summary paths
- `thread/loaded/read` remains the loaded `mode + status` summary surface
- `thread/loaded/list` remains id-only
- `thread/status/changed` remains status-only and does not repeat `mode`
- resident reconnect remains productized through `Thread.mode + thread/resume`
- consumers do not guess resident semantics from source, status, or local UI state

## Files / Areas

- `codex-rs/debug-client/...`
- `codex-rs/app-server-test-client/...`
- `codex-rs/app-server-client/...`
- directly-adjacent bridge / runtime / README docs

## Tests

Ran:

- `cargo test -p codex-debug-client`
- `cargo test -p codex-app-server-test-client`
- `cargo test -p codex-app-server-test-client unknown_thread_status_change_refresh_round_trip_restores_known_thread_summary`
- `cargo test -p codex-app-server --test all thread_status`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_list_preserves_id_only_probe`

Key negative-boundary coverage:

- unknown-thread notifications do not guess resident mode
- `thread/status/changed` stays status-only
- `thread/loaded/list` stays id-only
- consumers recover missing summary through `thread/read` / `thread/loaded/read`
- reconnect wording does not drift back to generic resume/reopen language

## Docs

Updated as needed:

- `docs/remote-bridge-consumption.md`
- `docs/remote-bridge-minimal-consumption-checklist.md`
- `codex-rs/app-server-client/README.md`
- directly-adjacent bridge/debug/test-client help or README text

## Why This PR First

This PR is meant to prove that the existing thread APIs already form a stable minimal remote
consumption loop before moving on to:

1. observer event-source tightening
2. SQLite authority/source convergence
3. higher-level bridge shells or UI
```

## 当前工作树可直接使用的草稿

如果当前提交主要覆盖：

- `codex-rs/debug-client/`
- `codex-rs/app-server-test-client/`
- `codex-rs/app-server-client/`
- `codex-rs/app-server/tests/suite/v2/thread_status.rs`
- 直接相邻的 bridge / runtime / README 文档

那么下面这份草稿可以直接作为 remote bridge 最小消费 PR 的起点：

```md
## Summary

This PR tightens the minimal remote thread-consumption loop across the existing debug, test, and
typed app-server client surfaces.

It keeps unknown-thread notifications status-only, keeps summary recovery on the read surfaces,
and keeps resident reconnect semantics driven by `Thread.mode` plus `thread/resume`.

## Scope

This PR is intentionally limited to:

- `debug-client` keeping unknown-thread refresh on the read surfaces instead of guessing resident
  reconnect semantics
- `app-server-test-client` keeping the same unknown-thread self-healing loop
- typed app-server client continuing to preserve server-returned repaired thread summaries on
  `thread/resume`, `thread/read`, `thread/list`, and `thread/loaded/read`
- `thread/loaded/list` remaining an id-only probe
- consumer-facing wording continuing to map `interactive -> resume` and
  `residentAssistant -> reconnect`

Out of scope:

- new bridge transport
- observer API expansion
- SQLite schema changes
- full bridge product UI

## Contract Checks

- `thread/list` remains the list summary surface
- unknown-thread or missing-summary paths recover through `thread/read`
- loaded runtime state recovers through `thread/loaded/read`
- `thread/status/changed` remains status-only
- resident reconnect continues to go through `thread/resume`
- consumers do not infer reconnect semantics from local state when `Thread.mode` is missing

## Tests

Ran:

- `cargo test -p codex-debug-client`
- `cargo test -p codex-app-server-test-client`
- `cargo test -p codex-app-server-test-client unknown_thread_status_change_refresh_round_trip_restores_known_thread_summary`
- `cargo test -p codex-app-server --test all thread_status`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_list_preserves_id_only_probe`

## Docs

Updated as needed:

- `docs/remote-bridge-consumption.md`
- `docs/remote-bridge-minimal-consumption-checklist.md`
- `codex-rs/app-server-client/README.md`

## Why This PR First

This PR proves that the existing thread APIs already support a minimal remote consumption loop
before any new transport, observer, or SQLite work is layered on top.
```

## 使用建议

这份模板最适合配合下面两份文档一起使用：

- `docs/remote-bridge-minimal-consumption-checklist.md`
- `docs/remote-bridge-consumption.md`

如果当前 worktree 已经明显混入 observer、SQLite 收口或纯文档流程整理，再额外配合：

- `docs/persistent-runtime-current-worktree-pr-split.md`

更具体地说：

- checklist 用来定义最小消费闭环边界
- consumption 文档用来定义接口分工
- 这份模板用来把它们收成可提交文本
- 如果当前 diff 已经装不进单一 remote bridge 包，就先回 worktree split 拆 PR，而不是继续硬扩这份模板

## 不适合把什么塞进这个模板

如果 PR 已经开始涉及下面这些内容，就不该继续用这份模板原样提交：

- 新 bridge transport / websocket 协议设计
- 新 observer API
- 新 SQLite schema
- 完整网页 / 桌面 bridge 产品壳层

这意味着 PR 范围已经超出“最小线程消费闭环”本身，应重新拆分或改用新的模板。
