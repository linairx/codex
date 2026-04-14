# SQLite State Convergence PR Template

本文承接：

- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增 SQLite 设计结论，而是给“状态来源收敛”提供一份可直接复制的 PR 描述模板。

## 模板

```md
## Summary

This PR tightens SQLite/state-db authority boundaries across stored summaries, persisted resident
metadata, and runtime overlays.

It keeps stored summaries repaired before they are treated as authoritative, keeps resident
identity anchored in persisted metadata, and keeps runtime-only status/observer state attached at
read time instead of reintroducing per-surface fallback drift.

## Scope

This PR is intentionally limited to:

- stored-summary repair / reconcile behavior in `rollout/state_db`
- app-server read/list/resume/unarchive/metadata-update/rollback authority ordering
- compatibility summary reads such as `getConversationSummary` staying on the same repaired-summary /
  loaded-provider-override path
- typed client continuity for repaired summary surfaces
- authority-first README / checklist wording for repaired thread summaries

Out of scope:

- new bridge transport
- observer API expansion
- full runtime snapshot persistence
- multi-agent persistence redesign
- unrelated CLI/TUI wording work

## Contract Checks

This PR preserves the following contracts:

- stored summary is not treated as authoritative until required rollout-derived fields are present
- resident identity continues to come from persisted metadata such as `threads.mode`
- runtime loaded/observer state continues to be attached as runtime metadata instead of being
  persisted as stable SQLite identity
- `thread/read`, `thread/list`, `thread/resume`, `thread/unarchive`,
  `thread/metadata/update`, and `thread/rollback` return repaired summaries directly rather than
  requiring a follow-up `thread/read`
- compatibility summary reads such as `getConversationSummary` continue to preserve loaded-thread
  provider overrides and repaired stored summaries instead of drifting back to default-provider
  fallback behavior
- remote typed facades continue to preserve repaired server-returned `Thread` values directly
- remote/in-process typed facades continue to preserve repaired `ConversationSummary` values
  directly on compatibility summary reads

## Files / Areas

- `codex-rs/rollout/src/state_db.rs`
- `codex-rs/state/...`
- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/...`
- `codex-rs/app-server-client/...`
- directly-adjacent SQLite/runtime README and checklist docs

## Tests

Ran:

- `cargo test -p codex-rollout read_repair_rollout_path_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-rollout read_repair_rollout_path_preserves_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_clears_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_archived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_unarchived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-state`
- `cargo test -p codex-app-server --test all stored_rollout_thread_read_and_list_preserve_rollout_summary_and_sqlite_mode`
- `cargo test -p codex-app-server --test all thread_metadata_update_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_unarchive_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all thread_resume_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_rollback_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server-client --lib read_and_list_reconcile_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib metadata_update_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib unarchive_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib resume_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib rollback_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_list_preserves_id_only_probe`
- `cargo test -p codex-app-server-client --lib get_conversation_summary_reconciles_missing_summary_with_loaded_provider_override_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_get_conversation_summary_preserves_repaired_summary`

Key negative-boundary coverage:

- existing SQLite rows with missing stored summary do not remain half-repaired
- archived/unarchived semantics do not drift during reconcile
- resident threads do not regress to interactive during repair or restore
- one surface does not return repaired preview while another still returns an empty summary
- client layers do not reintroduce a follow-up-read requirement after repaired responses

## Docs

Updated as needed:

- `docs/sqlite-state-convergence.md`
- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/README.md`

## Why This PR First

This PR is meant to stabilize authority ordering between SQLite, rollout, and runtime metadata
before higher-level consumers depend on that repaired summary contract.
```

## 当前工作树可直接使用的草稿

如果当前提交主要覆盖：

- `codex-rs/rollout/src/state_db.rs`
- `codex-rs/state/`
- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/thread_read.rs`
- `codex-rs/app-server/tests/suite/v2/thread_resume.rs`
- `codex-rs/app-server/tests/suite/v2/thread_metadata_update.rs`
- `codex-rs/app-server/tests/suite/v2/thread_unarchive.rs`
- `codex-rs/app-server/tests/suite/v2/thread_rollback.rs`
- `codex-rs/app-server-client/src/lib.rs`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/README.md`
- 直接相邻的 SQLite / runtime 文档

那么下面这份草稿可以直接作为 SQLite 状态收敛 PR 的起点：

```md
## Summary

This PR tightens SQLite/state-db authority boundaries for repaired thread summaries across
rollout/state-db helpers, app-server read/restore paths, and typed client consumption.

It keeps stored summaries reconciled before they are treated as authoritative, keeps resident
identity anchored in persisted metadata, and keeps repaired responses directly trustworthy on
`thread/read`, `thread/list`, `thread/resume`, `thread/unarchive`, `thread/metadata/update`, and
`thread/rollback`, while keeping `getConversationSummary` on the same repaired-summary path.

## Scope

This PR is intentionally limited to:

- `read_repair_rollout_path()` and related reconcile behavior in `rollout/state_db`
- app-server authority ordering for read/list/resume/unarchive/metadata-update/rollback
- compatibility summary reads such as `getConversationSummary`
- typed client continuity for repaired summary surfaces
- authority-first README / checklist wording

Out of scope:

- new bridge transport
- new observer API
- full runtime snapshot persistence
- unrelated product-layer wording changes

## Contract Checks

- existing SQLite rows with missing rollout-derived summary are reconciled instead of treated as
  authoritative
- resident identity continues to come from persisted metadata
- runtime-only observer/loaded status stays runtime-attached
- repaired responses are directly trustworthy and do not require a follow-up `thread/read`
- remote typed facades preserve repaired `Thread` values directly
- compatibility summary reads preserve loaded provider overrides and repaired
  `ConversationSummary` values directly

## Tests

Ran:

- `cargo test -p codex-rollout read_repair_rollout_path_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-rollout read_repair_rollout_path_preserves_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_clears_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_archived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_unarchived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-state`
- `cargo test -p codex-app-server --test all stored_rollout_thread_read_and_list_preserve_rollout_summary_and_sqlite_mode`
- `cargo test -p codex-app-server --test all thread_metadata_update_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_unarchive_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all thread_resume_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_rollback_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server-client --lib read_and_list_reconcile_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib metadata_update_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib unarchive_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib resume_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib rollback_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_list_preserves_id_only_probe`
- `cargo test -p codex-app-server-client --lib get_conversation_summary_reconciles_missing_summary_with_loaded_provider_override_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_get_conversation_summary_preserves_repaired_summary`

## Docs

Updated as needed:

- `docs/sqlite-state-convergence.md`
- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/README.md`

## Why This PR First

This PR proves repaired thread summaries and persisted resident identity already have a stable
authority ordering before higher-level consumers or future persistence work build on top.
```

## 使用建议

这份模板最适合配合下面三份文档一起使用：

- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence.md`

如果当前 worktree 已经明显混入 resident baseline、observer 收口或纯文档流程整理，再额外配合：

- `docs/persistent-runtime-current-worktree-pr-split.md`

更具体地说：

- checklist 用来定义阶段边界
- file todo 用来指导文件级收口
- convergence 文档用来定义状态分层与权威来源
- 这份模板用来把它们收成可提交文本
- 如果当前 diff 已经装不进单一 SQLite 包，就先回 worktree split 拆 PR，而不是继续硬扩这份模板

## 不适合把什么塞进这个模板

如果 PR 已经开始涉及下面这些内容，就不该继续用这份模板原样提交：

- 新 bridge transport
- 新 observer API
- 完整 runtime 镜像入库
- 多 agent 持久化大改

这意味着 PR 范围已经超出“SQLite 状态来源收敛”本身，应重新拆分或改用新的模板。
