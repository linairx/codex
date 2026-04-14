# Persistent Runtime Current Worktree PR Split

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-pr-drafts.md`
- `docs/persistent-runtime-pr-workflow.md`

目标不是新增新的任务包，而是把当前这次工作树里的实际改动按更合理的 PR 边界拆出来，方便直接整理提交。

## 1. 当前工作树的主判断

按当前 `git status` 与 diff 形态看，这批改动已经明显不是单一主线，而是至少同时覆盖了下面两包：

1. resident / mode 语义基线
2. SQLite repaired summary / authority ordering 收口

同时还带有一层更高阶的文档整理：

3. runtime 文档链执行 / 提交流程补全

这意味着如果现在直接把整个工作树压成一份 PR，风险会很高：

- scope 很难说清
- 测试面会混在一起
- reviewer 很难判断某个失败是 baseline、SQLite，还是纯文档整理问题

因此当前更合理的默认动作不是再补新文档，而是开始按下面顺序拆包。

## 2. 建议拆成哪几包

### PR 1. Resident Mode Baseline

这包优先收：

- `codex-rs/app-server-client/src/lib.rs`
- `codex-rs/tui/src/app.rs`
- `codex-rs/tui/src/app_server_session.rs`
- `codex-rs/tui/src/resume_picker.rs`
- `codex-rs/tui/src/chatwidget/tests/slash_commands.rs`
- `codex-rs/tui/src/chatwidget/tests/status_and_layout.rs`
- `codex-rs/cli/src/main.rs`
- `codex-rs/exec/src/cli_tests.rs`
- `codex-rs/README.md`
- `docs/exec.md`
- `docs/resident-mode-baseline-pr-checklist.md`
- `docs/resident-mode-baseline-file-todo.md`
- `docs/resident-mode-baseline-pr-template.md`

必要时可顺带带上：

- `codex-rs/app-server-client/README.md`
  仅当这份 README 的改动主要还是 resident/reconnect 语义，而不是 repaired summary 契约

这包的主问题应保持为：

- `Thread.mode`
- `interactive -> resume`
- `residentAssistant -> reconnect`
- `thread/loaded/read` / `thread/status/changed` / `thread/resume` 的语义基线

不应混入：

- `rollout/state_db` repair helper
- `codex-rs/app-server/src/codex_message_processor.rs` 里明显属于 repaired summary / provider override / authority ordering 的部分
- SQLite convergence 文档

### PR 2. SQLite State Convergence

这包优先收：

- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/conversation_summary.rs`
- `codex-rs/app-server/tests/suite/v2/thread_list.rs`
- `codex-rs/app-server/tests/suite/v2/thread_loaded_read.rs`
- `codex-rs/app-server/tests/suite/v2/thread_read.rs`
- `codex-rs/rollout/src/recorder_tests.rs`
- `codex-rs/app-server-client/src/lib.rs`
  仅保留 repaired summary / remote facade / typed repaired-thread 透传相关改动
  - 包括 `getConversationSummary` 的 loaded provider override repair 与 typed facade 透传回归
- `codex-rs/app-server-client/README.md`
- `codex-rs/docs/codex_mcp_interface.md`
- `docs/sqlite-state-convergence.md`
- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence-pr-template.md`

必要时可顺带带上：

- `docs/app-server-thread-mode-v2.md`
- `docs/persistent-assistant-mode-design.md`
  仅当这些文档改动确实是在说明 repaired summary / authority ordering 的新结论

这包的主问题应保持为：

- stored summary 完整度
- repaired summary authority-first
- SQLite / rollout / runtime overlay 的优先级
- remote facade 不重新降级服务端已修补的 `Thread`
- compatibility summary 读取面也继续直接信任服务端已修补的 `ConversationSummary`

不应混入：

- TUI / exec / CLI 的 resume/reconnect 文案
- observer watcher 生命周期边界
- 纯文档流程补全

### PR 3. Runtime Docs Workflow

这包只收文档链整理：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-implementation-plan.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`
- `docs/remote-bridge-consumption.md`
- `docs/remote-bridge-minimal-consumption-checklist.md`
- `docs/remote-bridge-minimal-consumption-pr-template.md`
- `docs/observer-event-source-checklist.md`
- `docs/observer-event-source-pr-template.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-pr-drafts.md`

这包的主问题应保持为：

- 把源码分析文档链真正收成分诊 / 执行 / 提交流程

不应混入：

- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence-pr-template.md`
- 新代码语义
- 新测试
- 新实现级边界

## 3. 当前最需要人工再分一下的文件

下面几个文件当前最可能同时含有两包内容，提交前应再按 diff 手工切：

- `codex-rs/app-server-client/src/lib.rs`
  一部分明显属于 baseline typed resident/reconnect continuity；另一部分属于 SQLite repaired summary / remote facade 透传
- `codex-rs/app-server-client/README.md`
  同时可能含 resident reconnect 语义和 repaired summary authority-first 语义
- `docs/persistent-runtime-checklists-index.md`
  既有主线分诊入口补充，也有 PR/template/workflow 汇总

更短的规则可以直接记成：

- 只要某段改动主要在讲 `mode/resume/reconnect`，更偏 PR 1
- 只要某段改动主要在讲 repaired summary / preview / authority ordering / remote facade direct trust， 更偏 PR 2
- 只要某段改动主要在讲“该看哪份文档、怎么起草 PR”，更偏 PR 3

如果当前是按文件直接拆分，更适合再多记一条：

- `docs/sqlite-state-convergence-*` 留在 PR 2
- `docs/persistent-runtime-*` 和 `docs/codex-rs-source-analysis.md` 留在 PR 3

## 4. 建议提交顺序

更合理的顺序通常是：

1. PR 1: Resident Mode Baseline
2. PR 2: SQLite State Convergence
3. PR 3: Runtime Docs Workflow

原因：

- baseline 是其余几包的语义地板
- SQLite 收口依赖 baseline 已经稳定，但又比纯文档 workflow 更贴近当前代码主改动
- docs workflow 最适合最后单独整理，避免在前两包 review 时引入额外阅读噪声

按当前这轮本地 `git status`，还可以再记住一个更贴近现实的结论：

- 当前已修改文件实际上主要落在 `PR 2 + PR 3`
- `PR 1` 在这轮 worktree 里更像背景边界，而不是当前需要先拆出来的一包

如果只按当前已改文件直接拆，最短顺序更适合是：

1. 先整理 `PR 2: SQLite State Convergence`
2. 再整理 `PR 3: Runtime Docs Workflow`

## 5. 每包起草时最短入口

如果现在就要开始起草：

- PR 1 从 `docs/resident-mode-baseline-pr-template.md` 开始
- PR 2 从 `docs/sqlite-state-convergence-pr-template.md` 开始
- PR 3 当前可以直接从 `docs/persistent-runtime-pr-drafts.md` 里的
  `Runtime Docs Workflow` 最小草稿起稿

如果后续不想重新手写结构，PR 3 现在更适合直接复用那份最小草稿；只在 scope
明显变化时，再回到下面这组结构重新展开：

1. Summary:
   把 persistent runtime 文档链从总分析收成分诊 / 选包 / worktree 拆包 / 流程 / drafts 体系
2. Scope:
   只包含 docs 下的索引、template、workflow、drafts 与当前 worktree 拆包收口
3. Out of scope:
   不新增代码、不新增实现级边界、不修改运行时契约

## 5.1 当前 worktree 的最短拆分顺序

如果当前只是想把这轮已改文件直接拆成可提交的两包，更适合按下面顺序操作：

### 先拆 PR 2

先只收下面这些文件：

- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/conversation_summary.rs`
- `codex-rs/app-server-client/src/lib.rs`
- `docs/sqlite-state-convergence-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/sqlite-state-convergence-pr-template.md`

这一步的目标是：

- 先把 repaired-summary / provider-override / typed facade 透传这条代码主线从当前 worktree 里拿出来

### 再拆 PR 3

剩下这批文档更适合留给 `PR 3`：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-checklists-index.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`
- `docs/persistent-runtime-pr-workflow.md`
- `docs/persistent-runtime-pr-drafts.md`

这一步的目标是：

- 再把分诊 / 选包 / 拆包 / 起稿这条 docs workflow 链单独整理出来

如果需要更短的执行规则，可以直接记成：

1. 先把 `codex-rs/**` 代码改动和 `docs/sqlite-state-convergence-*` 一起归到 PR 2
2. 再把 `docs/persistent-runtime-*` 和 `docs/codex-rs-source-analysis.md` 归到 PR 3

## 5.2 当前 worktree 的最短暂存顺序

如果当前已经准备直接在本地拆 commit，更适合按下面顺序暂存：

### 先暂存 PR 2

```bash
git add \
  codex-rs/app-server/src/codex_message_processor.rs \
  codex-rs/app-server/tests/suite/conversation_summary.rs \
  codex-rs/app-server-client/src/lib.rs \
  docs/sqlite-state-convergence-checklist.md \
  docs/sqlite-state-convergence-file-todo.md \
  docs/sqlite-state-convergence-pr-template.md
```

然后先检查一次：

```bash
git diff --cached --stat
```

如果缓存区里还混入了 `docs/persistent-runtime-*` 或 `docs/codex-rs-source-analysis.md`，
说明这一步还没有切干净，应先把 docs-workflow 那包拿出去。

### 再暂存 PR 3

在 PR 2 提交或导出 patch 之后，再暂存：

```bash
git add \
  docs/codex-rs-source-analysis.md \
  docs/persistent-runtime-checklists-index.md \
  docs/persistent-runtime-current-worktree-pr-split.md \
  docs/persistent-runtime-pr-workflow.md \
  docs/persistent-runtime-pr-drafts.md
```

同样再检查一次：

```bash
git diff --cached --stat
```

如果缓存区里又出现 `codex-rs/app-server/**`、`codex-rs/app-server-client/src/lib.rs` 或
`docs/sqlite-state-convergence-*`，说明 repaired-summary 那包重新混回来了。

如果需要最短版，可以直接记成：

1. 先 `git add` 代码文件 + `docs/sqlite-state-convergence-*`
2. 看一次 `git diff --cached --stat`
3. 再 `git add` `docs/persistent-runtime-*` + `docs/codex-rs-source-analysis.md`
4. 再看一次 `git diff --cached --stat`

## 5.3 当前 worktree 的最短提交顺序

如果当前已经按上面的顺序把缓存区切开，更适合继续按下面顺序提交：

### 先提交 PR 2

更合适的提交说明可以直接写成：

```bash
git commit -m "Align getConversationSummary with repaired summary flow"
```

提交前最后再看一眼：

```bash
git diff --cached --stat
git diff --cached
```

这一步最该确认的是：

- 只包含 `codex-rs/app-server/**`、`codex-rs/app-server-client/src/lib.rs` 和
  `docs/sqlite-state-convergence-*`
- 没有把 `docs/persistent-runtime-*` 或 `docs/codex-rs-source-analysis.md` 混进去

### 再提交 PR 3

更合适的提交说明可以直接写成：

```bash
git commit -m "Tighten persistent-runtime PR split and drafting flow"
```

提交前同样再看一眼：

```bash
git diff --cached --stat
git diff --cached
```

这一步最该确认的是：

- 只包含 `docs/codex-rs-source-analysis.md` 和 `docs/persistent-runtime-*`
- 没有把 `codex-rs/app-server/**`、`codex-rs/app-server-client/src/lib.rs` 或
  `docs/sqlite-state-convergence-*` 带回来

如果需要最短版，可以直接记成：

1. 先提交 repaired-summary 代码包
2. 再提交 docs workflow 包
3. 每次提交前都同时看 `git diff --cached --stat` 和 `git diff --cached`

## 5.4 提交后的最短 handoff

如果这两包已经分别提交完成，下一步更适合直接按下面衔接：

### PR 2 提交后

- 直接回 `docs/sqlite-state-convergence-pr-template.md`
- 使用其中“当前工作树可直接使用的草稿”
- 只保留这次实际提交里包含的文件、契约和已跑测试

### PR 3 提交后

- 直接回 `docs/persistent-runtime-pr-drafts.md`
- 使用其中 `Runtime Docs Workflow` 的最小草稿
- 只保留这次 docs-workflow 提交里实际包含的文档范围

如果需要最短版，可以直接记成：

1. 提交完 PR 2，就用 SQLite template 起 PR 文本
2. 提交完 PR 3，就用 Runtime Docs Workflow 草稿起 PR 文本

## 6. 这份拆包草案的作用边界

这份文档只负责：

- 把当前这次工作树按更合理的 PR 边界切开

它不负责：

- 替代对应 checklist / file todo / PR template
- 新增第五个主包
- 替你决定最终 commit 粒度

如果后续当前工作树继续变化，这份拆包草案也应跟着更新，而不是继续假设它永远正确。
