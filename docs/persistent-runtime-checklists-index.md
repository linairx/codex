# Persistent Runtime Checklists Index

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-runtime-implementation-plan.md`

目标不是补新的设计结论，而是给这条主线已经拆出来的执行清单提供一个统一入口。

## 1. 这组清单解决什么问题

到目前为止，resident / mode / observer / SQLite 这条主线已经不再缺总方向说明。

更实际的问题变成了：

- 先做哪一包
- 每一包的范围是什么
- 每一包做完后下一包是什么

下面这几份 checklist 就是为这个目的准备的。

## 2. 推荐执行顺序

建议按下面顺序推进：

1. `docs/resident-mode-baseline-pr-checklist.md`
2. `docs/resident-mode-baseline-file-todo.md`
3. `docs/resident-mode-baseline-pr-template.md`
4. `docs/remote-bridge-minimal-consumption-checklist.md`
5. `docs/observer-event-source-checklist.md`
6. `docs/sqlite-state-convergence-checklist.md`
7. `docs/sqlite-state-convergence-file-todo.md`

这个顺序对应的逻辑是：

- 先把 resident / mode 语义基线收稳
- 再把首个基线 PR 压成文件级待办，方便直接开工
- 再给首个基线 PR 一份可直接复制的描述模板
- 再证明现有 thread API 已经足够支撑最小远端消费者
- 再把 observer 从状态标记推进成正式状态来源
- 最后再收敛 SQLite 的状态分层与权威来源
- 并把 SQLite 这一包继续压成可直接开工的文件级待办

## 3. 每份清单各自负责什么

### 1. Resident Mode Baseline PR Checklist

文件：

- `docs/resident-mode-baseline-pr-checklist.md`

负责：

- 收口 `Thread.mode`
- 固定 `thread/status/changed` 的 status-only 边界
- 固定 `thread/loaded/list` 的 id-only probe 边界
- 固定 `thread/loaded/read` 的 loaded `mode + status` 恢复面语义
- 固定 `interactive -> resume` / `residentAssistant -> reconnect` 的动作映射

这份清单是后面所有清单的地板。

### 2. Resident Mode Baseline File TODO

文件：

- `docs/resident-mode-baseline-file-todo.md`

负责：

- 把首个 resident baseline PR 再细化成文件级待办
- 指明服务端、typed client、TUI、CLI、README 分别该先查什么
- 给真正开始改代码的人一个更直接的入口

这份 TODO 仍属于第一包，只是比 checklist 更贴近开工。

### 3. Resident Mode Baseline PR Template

文件：

- `docs/resident-mode-baseline-pr-template.md`

负责：

- 给首个 resident baseline PR 提供可直接复制的描述模板
- 固定 summary / scope / contracts / tests / docs sync 的表达方式
- 避免 PR 描述和 checklist / file todo 再次漂移

这份模板不定义新边界，只负责把前两份文档沉淀成可提交文本。

### 4. Remote Bridge Minimal Consumption Checklist

文件：

- `docs/remote-bridge-minimal-consumption-checklist.md`

负责：

- 证明最小远端消费者已经能稳定消费 thread summary
- 证明 `Thread.mode` 足以驱动 resume / reconnect 动作映射
- 证明 `thread/list`、`thread/loaded/read`、`thread/status/changed`、`thread/resume` 这几条路径已经能组成一个消费闭环

这份清单的重点不是新 transport，而是验证消费契约。

### 5. Observer Event Source Checklist

文件：

- `docs/observer-event-source-checklist.md`

负责：

- watcher 注册、迁移、清理边界
- `workspaceChanged` 的置位、保留、清理语义
- observer 状态在主要读取面和 reconnect 路径上的一致性

这份清单的重点不是更多事件类型，而是让 observer 成为稳定来源。

### 6. SQLite State Convergence Checklist

文件：

- `docs/sqlite-state-convergence-checklist.md`

负责：

- 明确稳定身份元数据与 runtime 状态的边界
- 明确 archive / unarchive / repair / reconnect 的权威来源
- 减少 rollout、SQLite、runtime snapshot 各说各话

这份清单的重点不是“把所有状态都入库”，而是分层收敛。

### 7. SQLite State Convergence File TODO

文件：

- `docs/sqlite-state-convergence-file-todo.md`

负责：

- 把 SQLite 收敛阶段继续压成文件级待办
- 指明 `rollout/state_db`、`app-server`、测试面、typed client、README 该先查什么
- 给这条线当前的实现收尾提供更直接的开工入口

这份 TODO 不定义新设计边界，只负责把 SQLite checklist 继续落成执行顺序。

## 4. 怎么判断该用哪份清单

可以用下面的简单规则：

- 如果问题是“同一个线程在不同入口说法不一致”，先看 resident baseline
- 如果问题是“已经知道第一包要做什么，但想直接按文件开工”，先看 resident baseline file todo
- 如果问题是“已经准备提交第一个 PR，但想快速写对 PR 描述”，先看 resident baseline PR template
- 如果问题是“远端消费者不知道该从哪条 thread API 取摘要和增量”，先看 remote bridge
- 如果问题是“`workspaceChanged` 的来源或生命周期不清”，先看 observer
- 如果问题是“repair / archive / reconnect 到底该信 rollout 还是 SQLite”，先看 SQLite

## 5. 当前最推荐的起点

如果现在就要开工，默认起点仍然应是：

- `docs/persistent-runtime-checklists-index.md`

按当前工作树与几份 checklist 已补的阶段快照看，更适合把默认下一站分成两层理解：

- 如果目标还是收 resident / mode 语义基线，仍先从 `docs/resident-mode-baseline-pr-checklist.md` 进入
- 如果本地改动已经覆盖 baseline、remote bridge 最小消费和 observer 读取面收口，那么默认更适合直接切到 `docs/sqlite-state-convergence-checklist.md`
- 如果已经确定当前就在补 SQLite 这一包，且需要按文件直接开工，那么下一站更适合是 `docs/sqlite-state-convergence-file-todo.md`

原因也已经和最初不同了：

- baseline 仍是整条线的地板，但当前这包在本地工作树里已经明显进入收尾阶段
- remote bridge 最小消费闭环和 observer 读取面边界也都已有本地回归与阶段快照
- 当前更值得继续收口的主问题，已经更接近 “repair / archive / reconnect 到底先信谁” 这条 SQLite 权威来源问题

如果需要一个最简默认规则，可以直接按下面执行：

1. 先用索引页判断自己是在补哪一包
2. 如果不是在补新的 baseline 漏口，优先进入 `docs/sqlite-state-convergence-checklist.md`
3. 如果已经确认是在补 SQLite 收敛实现，直接切到 `docs/sqlite-state-convergence-file-todo.md`

如果当前工作还停留在首个基线 PR 的整理阶段，再从下面这份清单开始：

- `docs/resident-mode-baseline-pr-checklist.md`

## 6. 这份索引页的维护原则

这份索引页只适合做两件事：

- 列出 checklist
- 说明顺序和边界

它不应该重新变成另一份总设计稿。

如果后续新增新的 checklist，更合理的做法是：

- 在这里补一条入口和顺序说明

而不是把新主线的完整细节重新堆回这个索引页里。
