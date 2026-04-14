# Persistent Runtime 归档文档

这个目录存放的是 persistent-runtime 这条线在某一轮实现和提交流程中产生的过程性文档。

它们的典型特点是：

- 面向某一轮 worktree
- 面向某一组 PR
- 面向 checklist / template / draft / split / workflow

这些文档对回看历史仍有价值，但不再适合作为当前主入口。

## 为什么归档

顶层 `docs/` 更适合只保留：

- 长期有效的架构分析
- 当前仍然有效的设计稿
- 当前路线图
- 当前实现计划

如果把 checklist / file-todo / template / drafts 持续留在顶层主路径里，后果通常是：

- 旧的 PR 过程文档被误当成当前开发入口
- 总分析文档不断退化成任务堆栈
- 路线图、设计稿和一次性提交流程混在一起

## 这里包含什么

目前归档的主要是：

- resident-mode baseline 的 checklist / file todo / template
- remote-bridge minimal consumption 的 checklist / template
- observer-event-source 的 checklist / template
- SQLite state convergence 的 checklist / file todo / template
- persistent-runtime 的 workflow / split / drafts / checklist 索引

## 当前主入口

如果你现在要继续判断 `codex-rs` 真实还缺什么，更适合回到：

- `docs/codex-rs-source-analysis.md`
- `docs/codex-rs-roadmap.md`
- `docs/persistent-runtime-implementation-plan.md`

如果你现在要继续做某个长期方向的设计，更适合回到：

- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/remote-bridge-consumption.md`
- `docs/sqlite-state-convergence.md`
