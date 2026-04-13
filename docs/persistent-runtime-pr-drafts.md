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
