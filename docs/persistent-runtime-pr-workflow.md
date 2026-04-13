# Persistent Runtime PR Workflow

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是新增设计结论，而是把这条文档链已经拆好的 checklist / file todo / PR template 串成一条可直接执行的提交流程。

## 1. 这份流程解决什么问题

到这个阶段，问题通常已经不再是：

- “主线为什么重要”
- “还缺哪些高层设计”

更常见的问题反而是：

- 我现在该从哪份文档进入
- 我怎么判断自己是在分诊、实现、review，还是整理 PR
- 我已经有一批本地改动后，下一步该看 checklist、file todo，还是 PR template
- 如果当前 worktree 已经混了两条主线，我该先怎么拆包

这份流程只解决这类执行问题。

## 2. 最短执行顺序

如果需要一个最短版本，可以直接按下面执行：

1. 先用 `docs/codex-rs-source-analysis.md` 判断自己当前在哪条主线上
2. 再用 `docs/persistent-runtime-checklists-index.md` 选具体任务包
3. 如果当前 worktree 已经明显混了两包，先看 `docs/persistent-runtime-current-worktree-pr-split.md`
4. 如果还在查缺口，先读 checklist
5. 如果已经准备改文件，读 file todo
6. 如果已经准备整理提交，直接读对应 PR template
7. 不要在已经进入第 5 或第 6 步后，又回到总分析文档扩写同层内容

## 3. 按角色怎么进入

### 实现者

如果当前目标是直接改代码，更合理的顺序是：

1. 先用索引页判断当前是在补：
   - baseline
   - remote bridge
   - observer
   - SQLite
2. 再进入对应 checklist
3. 如果这条主线有 file todo，就继续进 file todo
4. 带着 file todo 回到代码和测试里开工

实现者最不该做的事是：

- 重新回到总分析文档拆实现级待办
- 在还没选定主线前同时改两包
- 把“文档还没写完”误当成“代码不能开工”

### Reviewer

如果当前目标是 review 一包改动，更合理的顺序是：

1. 先看对应 checklist 的完成定义和负向边界
2. 再看对应 PR template 里的 contract checks
3. 最后再回到 diff 判断这包是否混入了第二个主问题

review 时最该优先拦下的通常是：

- PR 范围已经超出对应模板定义的 scope
- 测试没有覆盖模板里写明的关键负向边界
- PR 描述重新退回旧心智，例如又把 repaired summary 写成“先返回半结果，再补一次 `thread/read`”

### 提交整理者

如果当前目标已经是整理一包 PR，而不是继续补代码，更合理的顺序是：

1. 先确认当前改动属于哪一包
2. 如果当前 worktree 还混着第二条主线，先看 `docs/persistent-runtime-current-worktree-pr-split.md`
3. 再打开对应 PR template
4. 对照 checklist / file todo 把 scope、contracts、tests、docs sync 填实
5. 只在模板和现有改动不匹配时，再回到 checklist / file todo 查缺口

提交整理者最不该做的事是：

- 为了写 PR 描述重新发明新的包边界
- 在模板之外顺手补一轮新的实现设计
- 把两个主线包揉进同一份 PR 文本

## 4. 按任务包怎么进入

### 1. Resident Baseline

进入顺序：

1. `docs/resident-mode-baseline-pr-checklist.md`
2. `docs/resident-mode-baseline-file-todo.md`
3. `docs/resident-mode-baseline-pr-template.md`

更适合回答的问题：

- `Thread.mode` / `thread/status/changed` / `thread/loaded/read` / `thread/resume` 是否还在说同一套话
- TUI / exec / CLI 的最终文案是否仍和读取面一致
- 当前 baseline 改动是否已经可以收成一个 PR

### 2. Remote Bridge

进入顺序：

1. `docs/remote-bridge-minimal-consumption-checklist.md`
2. `docs/remote-bridge-minimal-consumption-pr-template.md`

更适合回答的问题：

- 现有 thread API 是否已经足以支撑最小远端消费者
- unknown-thread / loaded summary / status-only 增量 / reconnect 是否已经形成闭环
- 这包 PR 是否仍只是在验证消费契约，而不是引入新 transport

### 3. Observer

进入顺序：

1. `docs/observer-event-source-checklist.md`
2. `docs/observer-event-source-pr-template.md`

更适合回答的问题：

- watcher 生命周期边界是否已经清楚
- `workspaceChanged` 是否仍只通过正确的读取面和 status-only 通知暴露
- 当前 observer 收口是否已经可以作为单独 PR 提交

### 4. SQLite

进入顺序：

1. `docs/sqlite-state-convergence-checklist.md`
2. `docs/sqlite-state-convergence-file-todo.md`
3. `docs/sqlite-state-convergence-pr-template.md`

更适合回答的问题：

- stored summary / persisted metadata / runtime overlay 的权威顺序是否仍清楚
- repaired summary 是否已经能被调用方直接信任
- 当前 SQLite 收口改动是否已经可以收成单独 PR

## 5. 什么时候该切换文档

下面这些信号说明你该从一个层级切到下一个层级，而不是继续停留原地：

### 从总分析切到索引页

当你已经能回答：

- 当前是在补哪条主线
- 当前是在做实现、review，还是提交整理

这时就不该再停在 `codex-rs-source-analysis.md`。

### 从 checklist 切到 file todo

当你已经能回答：

- 这一包的主问题是什么
- 这包不该混入什么
- 你准备开始看具体文件

这时就该进入 file todo，而不是继续在 checklist 里补开放问题。

### 从 checklist / file todo 切到 PR template

当你已经能回答：

- 这包当前改动大致落在哪些文件
- 关键测试已经跑了哪些
- 这包的 scope / contracts / docs sync 能否写成一段完整描述

这时就该切到 PR template，而不是继续扩 checklist。

### 从 PR template 切到 worktree split

当你发现：

- 同一份模板已经装不下当前 diff
- 你必须反复写“本 PR 也顺手包含另一条主线”
- 同一份测试列表明显分成两组互不相干的边界

这时就不该继续硬写单包 PR，而应回到：

- `docs/persistent-runtime-current-worktree-pr-split.md`

## 6. 最短停手规则

如果需要一个最短停手规则，可以直接按下面记：

1. 总分析只做分诊
2. 索引页只做选包
3. checklist 只做边界
4. file todo 只做开工
5. PR template 只做提交

只要某份文档开始承担下一层的职责，就应该停手并切换到下一份文档。

按当前这轮 SQLite / repaired-summary 收口的实际状态，还可以再补一句更落地的规则：

- 如果 `sqlite-state-convergence-pr-template.md` 已经能自然装下当前 diff，且对应测试列表已经能直接列出，就不该继续扩写同层 checklist / workflow / 总分析文档；默认动作应改成直接起草 PR 2
- 如果 `persistent-runtime-pr-drafts.md` 里的 `Runtime Docs Workflow` 最小草稿已经能自然装下当前 docs diff，也不该继续回到 split / workflow 文档补同层说明；默认动作应改成直接起草 PR 3

## 7. 这份流程完成后该做什么

做到这里，这条文档链的默认下一步已经很明确：

- 不再新增同层模板
- 直接选择一个任务包
- 用对应 PR template 整理成最终提交文本

如果后续还要继续扩文档，更合理的触发条件通常只剩两类：

1. 某个任务包新增了完全不同的文件级入口
2. 默认执行顺序本身发生了变化

否则，更合理的动作通常是：

- 回到代码和测试
- 或直接整理 PR
