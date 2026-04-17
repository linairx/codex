# observer 事件流设计草案

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`

目标是把文档里提到的 `observer` 从一句基础设施建议，收敛成一个明确的事件流设计方向。

这里的 `observer` 不指新的全局产品名，而是：

- 对工作区变化进行持续观察
- 将变化转化为线程级运行时事实
- 为状态、提示、通知和后续持久化提供统一事件源

## 1. 为什么需要单独一份文档

在前面的分析里，`observer` 很容易被误解成下面几种完全不同的东西：

- 文件系统 watcher
- 长期记忆系统
- 仓库索引系统
- 远程同步系统
- SQLite 状态层

这些东西彼此有关，但不是一回事。

对当前 `codex-rs` 来说，第一阶段最重要的是把 `observer` 限定为：

- “线程可消费的工作区变化事件源”

而不是一个包办所有状态问题的大系统。

## 2. 当前基础

仓库里已经有几块很关键的基础：

- `codex-core/src/file_watcher.rs`
- `codex-rs/core/src/skills_watcher.rs`
- `codex-rs/app-server/src/thread_status.rs`
- `codex-rs/app-server/src/fs_watch.rs`

这些实现说明：

- 仓库已经有通用 watcher 基础设施
- app-server 已经能把工作区变化映射为 `workspaceChanged`
- `fs/watch` 已经证明 watcher 也可以作为外部 API 事件源
- `skills_watcher` 已经证明在通用 watcher 之上做特化 watcher 是一条现成路径

因此，observer 第一阶段不应另造一套并行 watcher 系统。

## 3. 设计目标

第一阶段 observer 事件流只追求四个目标：

1. 把工作区变化稳定转成线程级事实
2. 让长期线程和普通线程都能消费同一种变化信号
3. 让客户端能通过线程状态面感知这些变化
4. 为后续提示刷新、异步通知和状态持久化保留统一入口

## 4. 非目标

第一阶段明确不做：

- 不做代码索引
- 不做语义分析
- 不做自动 prompt 重建策略
- 不做长期记忆抽取
- 不做 SQLite 事件落盘
- 不做跨设备同步
- 不做用户级 `observer/*` 新 API 面

这些能力以后可能会基于 observer 事件流生长，但都不属于第一阶段。

## 5. 当前实现能说明什么

### `ThreadWatchManager` 已经是雏形

`app-server/src/thread_status.rs` 里当前已有这些事实：

- resident thread 会建立工作区 watch
- 变化会被汇总到 `workspaceChanged`
- 下一次 turn 开始会清掉该标志

这已经是一个很有价值的最小闭环，只是还比较偏“状态补丁”，而不是一个正式命名的事件流层。

### `fs/watch` 不是线程 observer 的替代

`fs/watch` 的定位是：

- 面向某个连接的通用文件系统订阅 API

它不直接等价于线程 observer，因为线程 observer 需要的是：

- 与线程生命周期绑定
- 能映射到线程状态
- 能被线程恢复与驻留语义消费

所以 observer 可以复用 watcher 基础设施，但不应直接复用 `fs/watch` 的产品语义。

### `skills_watcher` 说明了推荐实现方向

`skills_watcher` 本质上就是：

- 在通用 `FileWatcher` 之上构造特定领域 watcher

这说明 observer 最合理的工程路径是：

- 继续复用 `FileWatcher`
- 在其上建立线程相关的事件聚合层

而不是重新引入另一套底层监听机制。

## 6. 建议的事件流分层

第一阶段建议把 observer 理解成四层。

### 第 1 层：原始文件变化

来源是现有 `FileWatcher`。

这一层只回答：

- 哪些路径发生了变化
- 变化是否命中了某线程的观察范围

它不负责产品语义。

### 第 2 层：线程观察事件

这是 observer 第一阶段真正应该新增或明确命名的层。

这一层负责把原始文件变化归约成线程可消费事件，例如：

- 某线程工作区发生了外部变化
- 某线程观察范围内的关键路径发生了变化

第一阶段不必暴露很复杂的事件种类，只要先稳定表达“线程工作区发生变化”即可。

### 第 3 层：线程状态映射

这一层把观察事件映射到线程状态面，例如：

- `workspaceChanged`

这一步是让客户端和远端控制面可消费的关键。
但这里的线程状态面仍应保持当前协议边界：

- `thread/status/changed` 只负责推送增量 `status`
- 不负责重复 `Thread.mode`
- `thread/closed`、`thread/archived`、`thread/unarchived` 仍应保持 lifecycle
  边事件语义，而不是被 observer 强化后重新抬成线程摘要返回面
- `thread/name/updated` 也仍只是 name 增量，不应因为 observer 或后续提示构建
  需要而开始偷偷承担完整线程快照职责
- 客户端和远端控制面应继续从 `thread/started`、`thread/read`、
  `thread/list`、`thread/loaded/read` 这些恢复面保留线程角色
- 如果后续消费者要在收到 `thread/closed`、`thread/archived`、
  `thread/unarchived`、`thread/name/updated` 后更新本地视图，也应把它们应用到
  最近一次权威 `Thread` 摘要上，而不是期待这些通知本身重复 `mode`、`name`、
  `preview` 或其他 repaired summary 字段
- 如果客户端需要直接展示动作文案，也应继续从这些恢复面拿到的
  `Thread.mode` 做稳定映射：
  - `interactive -> resume`
  - `residentAssistant -> reconnect`

第一阶段最重要的是：

- 让 observer 事件优先通过线程状态面可见

而不是马上引入新的并行通知体系。

### 第 4 层：后续消费者

后续可能消费 observer 事件流的东西包括：

- 提示构建
- 异步通知
- 长期状态持久化
- 多 agent 状态同步
- 远程 bridge

但它们都应位于 observer 事件流之后，而不是和 watcher 层直接耦合。

## 7. 第一阶段建议事件模型

第一阶段建议只定义一个足够小的线程级事件语义：

- `workspaceChanged`

如果需要更明确一点，可以在内部设计上把它理解为：

- “线程观察范围内出现了新外部变化，需要客户端或下一次 turn 关注”

这里故意不建议一开始就引入大量事件类型，例如：

- `gitStatusChanged`
- `dependencyLockChanged`
- `buildOutputChanged`
- `configChanged`
- `taskSignalObserved`

因为这些都属于第二阶段以后，等真正有消费方时再细分更稳。

## 8. 与线程模式和状态的关系

observer 文档必须和前两份文档保持一致：

- 线程模式回答“这是什么线程”
- 线程状态回答“它现在在做什么”
- observer 事件流回答“外部发生了什么，值得线程知道”

三者不要混用。

具体来说：

- `residentAssistant` 决定线程是否更需要持续观察
- `ThreadStatus` 决定客户端当前怎么呈现线程状态
- observer 事件让 `workspaceChanged` 这样的事实进入状态面

## 9. 生命周期建议

第一阶段 observer 建议遵循以下生命周期。

### 注册

- 线程进入需要观察的模式后，注册工作区观察
- 第一阶段最自然的起点仍是 resident thread

### 运行

- 原始文件变化经过节流后映射为线程观察事件
- 线程观察事件再映射到线程状态面

### 清理

- 线程 unload / shutdown 时清理观察注册
- 非 resident 线程不应长期占用观察资源

### 清空

- 当新 turn 开始时，可以继续沿用当前行为清理 `workspaceChanged`
- 但文档上要明确：这不是说变化被“处理完了”，而是说该变化提醒已被当前 turn 吸收

## 10. 第一阶段推荐实现策略

如果按工程改动来拆，第一阶段更合理的方向是：

1. 保持 `FileWatcher` 作为底层实现
2. 继续以 `ThreadWatchManager` 作为线程状态汇聚入口
3. 把“线程工作区变化”从隐含逻辑提升为明确的 observer 事件概念
4. 先稳定 `workspaceChanged` 的触发、保留和清理语义

换句话说，第一阶段更像是：

- “命名并收敛现有能力”

而不是：

- “实现一个全新的 observer 子系统”

## 11. 验收标准

第一阶段 observer 事件流完成后，至少应满足：

- resident thread 的工作区变化能够稳定映射到线程状态
- 客户端无需自己做文件 watch 也能知道线程工作区发生了变化
- 线程 unload / shutdown 时观察资源会被正确清理
- observer 事件流没有顺手演变成索引系统或状态数据库

## 12. 下一步文档

在这份文档之后，更合理的下一份文档是：

- `docs/sqlite-state-convergence.md`

它应只处理：

- 哪些 observer / 线程 / 后台状态值得落 SQLite
- 哪些状态只是瞬时事件，不值得持久化
- 如何避免在产品模式未稳定前过早固化状态模型

如果要继续往实现推进，则应再补：

- `docs/persistent-runtime-implementation-plan.md`
