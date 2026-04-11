# Resident Mode Baseline PR Template

本文承接：

- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/resident-mode-baseline-file-todo.md`

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
- source-kinds omit/`[]` still means interactive-only

## Files / Areas

- `codex-rs/app-server/...`
- `codex-rs/app-server-client/...`
- `codex-rs/tui/...`
- `codex-rs/cli/...`
- `codex-rs/exec/...`
- relevant README/help/docs sync

## Tests

Ran:

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-tui`
- `cargo test -p codex-exec`

Key negative-boundary coverage:

- `thread/status/changed` stays status-only
- `thread/loaded/list` stays id-only
- unknown thread notifications do not guess resident mode
- resident read/resume/repair paths do not regress to interactive
- observer state is not reactivated by stale watcher events after shutdown

## Docs

Updated as needed:

- app-server README
- app-server-client README
- user-facing help/summary wording
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

更具体地说：

- checklist 用来定义边界
- file todo 用来指导改哪些文件
- 这份模板用来确保 PR 描述和前两者保持同一口径

## 不适合把什么塞进这个模板

如果 PR 已经开始涉及下面这些内容，就不该继续用这份模板原样提交：

- 新 bridge transport
- 新 observer API
- 新 SQLite schema
- 多 agent / planner 新能力

这意味着 PR 范围已经超出“resident baseline”本身，应重新拆分或改用新的模板。
