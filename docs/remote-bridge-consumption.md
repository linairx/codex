# 远程 bridge 状态消费设计草案

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/sqlite-state-convergence.md`
- `docs/persistent-runtime-implementation-plan.md`

目标不是设计一个完整远程产品，而是回答一个更具体的问题：

- 当线程模式、线程状态、observer 摘要和 SQLite 元数据逐步稳定后，远程 bridge 应该如何消费这些信息，才能把 `codex-rs` 从本地终端工具推进到“本地运行时 + 远端控制面”。

## 1. 范围

这份文档只讨论：

- 远端控制面最小需要读到哪些线程信息
- 这些信息分别来自哪里
- 客户端应该优先消费哪些现有接口
- 哪些事情应该延后，不要在第一阶段绑死

这份文档不讨论：

- websocket / http / 认证 transport 细节
- UI 视觉设计
- 多 agent 仪表盘细节
- 数据同步协议的完整形式
- 具体商业产品形态

## 2. 为什么远程消费要单独设计

如果不把“远程消费”单独拿出来，很容易出现两个错误方向：

- 直接把本地 TUI 心智搬到远端，导致远端仍依赖大量 item 流拼装
- 直接把数据库当 API，绕过 app-server 的线程语义层

这两条路都不理想。

更合理的方式是：

- app-server 继续作为远端消费的主入口
- 线程模式、线程状态、observer 派生摘要和 SQLite 元数据通过清晰分层汇总到 app-server
- 远端 bridge 优先消费“线程摘要”，而不是自己复原底层运行时

## 3. 远端控制面最小需要知道什么

如果只考虑第一阶段以后最小可用的远端控制面，它至少需要知道下面几类信息。

### 线程身份

例如：

- thread id
- 线程模式
- 是否 resident
- cwd
- preview / name
- source

这类信息主要来自：

- `Thread`
- SQLite thread metadata

### 当前状态

例如：

- `NotLoaded` / `Idle` / `SystemError` / `Active`
- `activeFlags`
- 是否正在等待审批
- 是否正在等待用户输入
- 是否存在后台 terminal
- 是否存在工作区变化

这类信息主要来自：

- app-server 的 `ThreadStatus`

### 可恢复摘要

例如：

- 最近更新时间
- 最近一次需要关注的变化
- 线程是否仍有长期语义上的“未处理状态”

这类信息来自：

- app-server 摘要
- SQLite 中真正稳定的派生状态

### 深入读取入口

远端控制面不应默认一次拉全量历史，但应保留按需入口：

- `thread/read`
- `thread/resume`
- 必要时 turn/item 通知流

## 4. 推荐消费顺序

远端 bridge 不应该从最细粒度事件开始消费，而应该按下面顺序。

### 第 1 层：线程列表摘要

优先消费：

- `thread/list`
- `thread/loaded/read`

目的：

- 先知道有哪些线程
- 哪些线程当前 loaded
- 哪些线程是长期线程
- 哪些线程当前需要关注

这一层解决“总览”问题。

### 第 2 层：线程状态通知

优先消费：

- `thread/status/changed`
- `thread/started`
- `thread/closed`

目的：

- 增量更新线程摘要
- 避免远端频繁全量轮询

这一层解决“变化追踪”问题。

### 第 3 层：按需深读

按需调用：

- `thread/read`
- `thread/resume`

目的：

- 当用户真正进入某线程时再读取更重的上下文

这一层解决“钻取详情”问题。

### 第 4 层：必要时消费 turn/item 流

只有在真正进入线程实时交互时，才应消费：

- `turn/*`
- `item/*`

不要把这层当作远端首页的默认数据源。

## 5. 线程模式如何改变远端消费方式

`Thread.mode` 一旦落地，远端控制面对线程的默认处理逻辑就应该发生变化。

### `interactive`

更适合被视为：

- 普通会话
- 历史恢复对象
- 按需打开的详情页对象

### `residentAssistant`

更适合被视为：

- 长期存在的运行时对象
- 需要持续观察状态变化的对象
- 断开后可重新连接的对象

也就是说，远端 bridge 不应把所有线程都当成“历史对话列表项”，而应对 `residentAssistant` 使用更接近运行时控制面的心智。

## 6. observer 摘要在远端的意义

observer 文档里已经强调，第一阶段最重要的是：

- `workspaceChanged`

对远端控制面来说，这个信号的价值不在于告诉它“哪个文件变了”，而在于告诉它：

- 这个长期线程的本地工作区发生了新变化，值得用户关注或重新进入线程

因此远端控制面第一阶段应优先消费：

- “线程有无新的外部变化”

而不是立即要求：

- 全量 changed path 列表
- 文件 diff
- 自动上下文刷新结果

这些更细内容应延后。

## 7. SQLite 元数据在远端的意义

远端 bridge 不应直接查询 SQLite，但 SQLite 收敛后的元数据会通过 app-server 间接影响远端消费质量。

对远端最有价值的是：

- 更稳定的线程摘要
- 更可靠的最近活动时间
- 更一致的线程模式和长期属性
- observer 派生摘要
- 后台任务摘要

换句话说，SQLite 在远端视角下的价值是：

- 提升摘要和恢复的一致性

而不是：

- 让远端直接消费数据库细节

## 8. 第一阶段远端 bridge 应避免什么

### 不要默认依赖完整 item 流重建首页

这样成本高，而且和 app-server 状态面重复。

### 不要把 SQLite 当远端 API

这会绕过协议演进空间，也会把内部实现细节暴露出去。

### 不要把 `Active` 简化成“正在运行”

远端展示时必须考虑：

- `workspaceChanged`
- `waitingOnApproval`
- `waitingOnUserInput`
- `backgroundTerminalRunning`

这些事实的语义不同。

### 不要在第一阶段强求多 agent 统一面板

多 agent 当然重要，但这应建立在长期线程与远端摘要已经稳定之后。

## 9. 第一阶段推荐能力面

如果只做一个最小远端控制面，我会建议它至少有下面这些能力：

1. 查看线程列表，并区分长期线程与普通线程
2. 查看每个线程当前状态和活动标签
3. 感知工作区变化、审批等待和后台 terminal
4. 重新连接长期线程
5. 进入线程后再按需消费更细的 turn/item 流

做到这里，已经能明显提升“本地运行时 + 远端控制面”的闭环体验。

## 10. 与实现计划的关系

这份文档不应打乱实现顺序。  
它依赖的前提仍然是：

1. `Thread.mode` 已稳定
2. observer 事件流已稳定映射到状态面
3. SQLite 已开始收敛稳定摘要

因此远端 bridge 应该是这些工作的消费者，而不是它们的前置阻塞项。

## 11. 后续实现建议

如果未来要把这份文档继续转成工程计划，更合理的拆法是：

1. 先做远端线程摘要读取
2. 再做状态通知消费
3. 再做长期线程重新连接体验
4. 最后再接更细的 observer 摘要和后台任务摘要

这比一开始就做完整远程终端镜像更稳。
