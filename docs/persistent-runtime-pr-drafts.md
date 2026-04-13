# Persistent Runtime PR Drafts

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增任何设计结论，而是把当前这条文档链里已经补好的各包 PR template 草稿汇总成一个总入口，方便直接挑一包整理提交。

## 1. 这份汇总解决什么问题

到这个阶段，最常见的问题已经不是：

- “下一包做什么”
- “哪份 checklist 该先读”

而是：

- 当前工作树更适合先提交哪一包
- 每一包现成可用的 PR 草稿在哪里
- 如果我现在就要起草 PR，最短路径是什么
- 如果这次 worktree 已经混了两包，该先按什么边界切开

这份文档只解决这些问题。

## 2. 最短使用方式

如果你已经完成分诊，可以直接按下面执行：

1. 先确认当前改动主要落在哪一包
2. 如果当前 worktree 仍明显混着第二包，先看 `docs/persistent-runtime-current-worktree-pr-split.md`
3. 直接跳到这一包的“推荐进入条件”
4. 打开对应 PR template
5. 从这份汇总里的“当前工作树建议”判断是否已经可以起草 PR

按当前这轮本地 worktree，如果你只是想知道“现在立刻该做什么”，更短的答案已经可以固定成：

1. 选 `4. SQLite State Convergence`
2. 打开 `docs/sqlite-state-convergence-pr-template.md`
3. 复制“当前工作树可直接使用的草稿”
4. 只保留这轮 diff 实际覆盖的文件、契约和已跑测试

如果你还没完成分诊，先回：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`

## 3. 当前四包的起草入口

### 1. Resident Mode Baseline

适合在下面这些情况优先起草：

- 当前改动主要集中在 `Thread.mode`、`thread/status/changed`、`thread/loaded/read`、`thread/resume`
- TUI / exec / CLI 的最终文案和 help/hint/exit-summary 已经一起进入本地改动集
- 当前目标是把 resident/reconnect 语义基线收成第一包

推荐进入顺序：

1. `docs/resident-mode-baseline-pr-checklist.md`
2. `docs/resident-mode-baseline-file-todo.md`
3. `docs/resident-mode-baseline-pr-template.md`

当前工作树建议：

- 如果你现在的 diff 仍主要覆盖 `app-server`、`app-server-client`、`tui`、`exec`、`cli` 和相邻 README/help 文档，通常优先从这包开始整理

### 2. Remote Bridge Minimal Consumption

适合在下面这些情况优先起草：

- 当前改动主要集中在 `debug-client`、`app-server-test-client`、typed app-server client 的 thread summary 消费
- 当前目标是证明现有 thread API 已足够支撑最小远端消费者
- 这包没有混入新 transport、observer API 或 SQLite schema

推荐进入顺序：

1. `docs/remote-bridge-minimal-consumption-checklist.md`
2. `docs/remote-bridge-minimal-consumption-pr-template.md`

当前工作树建议：

- 如果你现在的 diff 主要是在最小消费者层验证 `thread/list` / `thread/read` / `thread/loaded/read` / `thread/status/changed` / `thread/resume` 闭环，这包可以单独起草

### 3. Observer Event Source

适合在下面这些情况优先起草：

- 当前改动主要集中在 `thread_status.rs`、watcher 生命周期、`workspaceChanged` 连续性
- 当前目标是把 observer 状态来源收成一包，而不是继续设计新事件系统
- 当前改动已覆盖 app-server / typed client / TUI 三层 observer 连续性

推荐进入顺序：

1. `docs/observer-event-source-checklist.md`
2. `docs/observer-event-source-pr-template.md`

当前工作树建议：

- 如果你现在的 diff 主要在 watcher 注册/迁移/清理与 `workspaceChanged` 读取面一致性上，这包可以直接起草

### 3.1 Runtime Docs Workflow

适合在下面这些情况优先起草：

- 当前改动不再新增运行时契约，而是在收 `codex-rs-source-analysis.md` 到 checklist /
  file todo / PR template / worktree split / workflow / drafts 这一整条文档链
- 当前目标是把“怎么分诊、怎么拆包、怎么起草 PR”写成可执行流程，而不是继续补实现级边界
- 当前 worktree 已经需要把 `PR 2` 和 `PR 3` 的 docs 范围切开

推荐进入顺序：

1. `docs/persistent-runtime-current-worktree-pr-split.md`
2. `docs/persistent-runtime-pr-workflow.md`
3. `docs/persistent-runtime-pr-drafts.md`

当前工作树建议：

- 如果你现在的 diff 主要在 `docs/codex-rs-source-analysis.md`、`docs/persistent-runtime-checklists-index.md`、`docs/persistent-runtime-current-worktree-pr-split.md`、`docs/persistent-runtime-pr-workflow.md` 和 `docs/persistent-runtime-pr-drafts.md` 上，这包可以直接起草
- 按当前这轮本地改动看，这包也已经可以直接按下面这份最小草稿起稿，而不需要再回去补新的模板
- 如果当前 diff 还带着 `docs/sqlite-state-convergence-checklist.md`、`docs/sqlite-state-convergence-file-todo.md` 或 `docs/sqlite-state-convergence-pr-template.md`，应先把这些留回 PR 2，再起这份 docs workflow 草稿

当前这轮 worktree 可直接使用的最小 PR 草稿可以先写成：

```md
## Summary

This PR tightens the persistent-runtime docs workflow so the current worktree
 can be split into implementation PRs without reusing the source-analysis doc as
 an implementation todo list.

It turns the current docs chain into a clearer handoff path from analysis, to
 package selection, to worktree split, to PR drafting, and makes the current
 SQLite convergence work stop at a directly-usable PR 2 draft instead of
 continuing to expand same-level docs.

## Scope

This PR is intentionally limited to:

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-pr-drafts.md`

Out of scope:

- new code changes
- new runtime protocol semantics
- new test-only boundaries
- changes that belong in SQLite / baseline / observer implementation PRs

## Contract Checks

- the source-analysis doc stops at routing and handoff instead of growing new
  implementation-level todo lists
- the current worktree split keeps repaired-summary code/docs in PR 2 and
  keeps workflow / drafts / routing docs in PR 3
- the workflow doc now points directly at the PR template layer as the stop
  point once a package is ready
- the drafts doc contains directly-usable minimal PR text for the current
  SQLite and docs-workflow splits

## Docs

Updated:

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-pr-drafts.md`

Left to PR 2 instead of this docs-workflow PR:

- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence-pr-template.md`

## Why This PR Now

This keeps the current persistent-runtime doc chain from drifting back into a
 second implementation planning layer and makes the current mixed worktree
 directly splittable into SQLite convergence and docs workflow PRs.
```

### 4. SQLite State Convergence

适合在下面这些情况优先起草：

- 当前改动主要集中在 `rollout/state_db` repair helper、`app-server` 读取/恢复面、typed repaired-summary 消费
- 当前目标是收敛 stored summary / persisted metadata / runtime overlay 的权威顺序
- 当前改动已经不再是“补一个模式字段”，而是“消灭半修复状态和 fallback 分叉”

推荐进入顺序：

1. `docs/sqlite-state-convergence-checklist.md`
2. `docs/sqlite-state-convergence-file-todo.md`
3. `docs/sqlite-state-convergence-pr-template.md`

当前工作树建议：

- 如果你现在的 diff 主要在 `rollout/state_db`、`codex_message_processor.rs`、相关服务端测试、typed repaired-summary 回归和 README 契约上，这包可以直接起草
- 如果这批改动还顺带补了 `getConversationSummary` 的 loaded provider override repair、以及 `app-server-client` 对 repaired `ConversationSummary` 的 typed 透传回归，也仍应留在这包，而不是拆去 baseline 或纯文档 PR
- 按当前这轮本地改动看，这包已经可以直接从 `docs/sqlite-state-convergence-pr-template.md` 的“当前工作树可直接使用的草稿”起草，再按实际已跑测试删减为最小提交文本

当前这轮 worktree 可直接使用的最小 PR 草稿可以先写成：

```md
## Summary

This PR tightens the repaired-summary boundary for compatibility summary reads so
`getConversationSummary` stays aligned with the same SQLite / rollout /
loaded-thread authority ordering already used by the main thread read surfaces.

It keeps loaded provider overrides and repaired stored summaries intact when a
conversation summary is looked up by thread id, and extends the typed
app-server-client coverage so both in-process and remote facades continue to
preserve the repaired `ConversationSummary` directly.

## Scope

This PR is intentionally limited to:

- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/conversation_summary.rs`
- `codex-rs/app-server-client/src/lib.rs`
- directly-adjacent SQLite convergence docs that describe this repaired-summary
  boundary and PR scope

Out of scope:

- new bridge transport
- observer lifecycle changes
- TUI / exec / CLI wording work
- broader SQLite helper refactors beyond this compatibility-summary path

## Contract Checks

- `getConversationSummary` continues to reuse the loaded thread's state-db and
  provider override when reading by thread id
- existing SQLite rows with missing stored summary are reconciled instead of
  falling back to default-provider behavior
- in-process typed clients continue to receive the repaired
  `ConversationSummary` directly without a follow-up read
- remote typed facades continue to preserve server-returned repaired
  `ConversationSummary` values directly

## Tests

Ran:

- `cargo test -p codex-app-server conversation_summary`
- `cargo test -p codex-app-server-client get_conversation_summary`

## Docs

Updated as needed:

- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence-pr-template.md`

## Why This PR Now

This keeps the compatibility summary path from drifting away from the repaired
summary contract that the main thread read/list/resume surfaces already follow,
and leaves the current worktree in a state where PR 2 can be split and
submitted without adding new implementation scope.
```

如果当前 worktree 还同时带着下面这些文档改动：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-pr-drafts.md`

更合理的默认动作不是把它们继续塞进这份最小 PR 2 草稿，而是：

1. 保持上面这份 SQLite 草稿只覆盖代码 + SQLite convergence 文档
2. 把这些流程 / 分诊 / 草稿入口文档留给 `PR 3: Runtime Docs Workflow`

## 4. 什么时候不要起草新的包

下面这些情况通常说明不该继续新开一份 PR 草稿，而应该先回去收 scope：

- 当前 diff 同时明显覆盖两条主线
- 还没法明确说出这一包“不包含什么”
- 关键测试还没法按对应 template 列出来
- 需要靠“后续再补一次 `thread/read`”之类的说法才能解释当前行为

更短的判断方式可以直接记成：

1. 一包只收一个主问题
2. 一包必须有对应 template 能承接
3. 如果现有 template 无法自然容纳当前 diff，先拆包，不要先写 PR

## 5. 从这里到最终 PR 文本

如果已经选定了一包，最短路径就是：

1. 打开对应 PR template
2. 复制“当前工作树可直接使用的草稿”
3. 用 checklist / file todo 把 scope、contracts、tests、docs sync 填实
4. 删除草稿里和当前 diff 无关的句子
5. 只保留当前这包真正完成的边界，不替下一包预写承诺

## 6. 和其他文档的分工

这份文档只负责：

- 告诉你当前工作树该从哪份 PR 草稿起步

它不负责：

- 替代 checklist
- 替代 file todo
- 新增任何设计边界

如果你需要：

- 分诊：回 `docs/codex-rs-source-analysis.md`
- 选包：回 `docs/persistent-runtime-checklists-index.md`
- 串流程：看 `docs/persistent-runtime-pr-workflow.md`
- 真正起草：进对应 PR template
