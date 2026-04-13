# SQLite 状态收敛设计草案

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`

目标不是证明 SQLite 值不值得用，而是回答一个更现实的问题：

- 在 `codex-rs` 已经有 rollout、内存状态、app-server 运行态和现成 SQLite 基础的前提下，哪些状态值得继续收敛进 SQLite，哪些不该过早固化。

## 1. 当前现实

从仓库现状看，SQLite 并不是未来才会引入的新东西，而是已经存在的重要状态层。

当前已经能看到的基础包括：

- `codex-rs/state/src/runtime.rs` 的 `StateRuntime`
- thread metadata 相关模型与查询
- memories 相关状态与作业
- logs SQLite
- backfill 状态
- `rollout` 与 `state_db` 的桥接
- app-server 的 `thread/metadata/update`

而且这条线已经不只是“基础设施存在”，还开始形成了第一批实现闭环：

- `threads.mode` 已经作为 resident assistant 的稳定线程模式元数据进入 SQLite
- `thread/read`、`thread/list`、`thread/resume` 在服务重启后已经会消费这层持久化模式，而不是只依赖 rollout 或内存态推断
- `thread/start`、`thread/fork`、`turn/start` 已经开始在 rollout 物化后修补或持久化线程模式，同时避免给未物化线程提前创建垃圾行
- `thread/unarchive`、archived `thread/read` / `thread/list`、以及 `thread/metadata/update` 的 stored / loaded-repair / archived 路径，都已经补上 resident mode 连续性回归
- `codex-state` 也已有边界测试保证：metadata patch 不会顺手覆盖 resident thread 的 `threads.mode`
- `rollout` / SQLite 的摘要 repair 边界也已继续补齐：当 SQLite 已经有 thread 行、但 rollout-derived `first_user_message` 这类 stored summary 字段仍缺失时，read-repair 不会再停在只修 `rollout_path` 的半修复状态，而会继续 reconcile rollout，把 DB-backed `thread/list` 需要的摘要字段补完整
- `thread/rollback` 现在也已继续对齐这条 stored-summary repair 约束：如果 rollback 前 SQLite row 已存在、但 rollout-derived preview 仍缺失，rollback 返回面、后续 `thread/read` / `thread/list`，以及 SQLite row 都会一起恢复同一份 summary，而不是只保 resident mode、继续留下空 preview
- `app-server` 的 loaded-thread 读取面现在也已开始共用同一条 helper：`thread/list`、`thread/loaded/read` 和 `thread/read` 的 loaded fallback 不再各自重复一份 rollout summary 拼装逻辑，而是共同走 resident-aware 的 loaded summary helper；因此 loaded thread 的 provider override 也不会在 rollout metadata 缺失时悄悄退回默认 provider
- `app-server` / `app-server-client` README 也已开始把这条契约写实：`thread/list`、`thread/resume`、`thread/read`、`thread/metadata/update`、`thread/unarchive` 这些读取/恢复面返回的 `Thread` 本身应被直接视为权威 repaired summary，而不是“先拿 mode，再额外补一次 `thread/read`”的半恢复响应
- `codex-app-server-client` 的 remote facade 也已继续补到同一层：websocket 远端直接返回 repaired `thread/resume`、`thread/read`、`thread/list` 或 `thread/loaded/read` 摘要时，typed remote client 现在都会继续原样保留 `thread.mode + preview + status + path + name`，而不是把 reconnect / stored lookup / list polling / loaded polling 的摘要修补职责重新推回客户端

这意味着当前讨论的不是“要不要上 SQLite”，而是：

- 如何让 SQLite 从部分事实来源，逐步演进成更稳定的状态收敛层

## 2. 为什么还不能立刻全量收敛

虽然 SQLite 已经存在，但前面的文档已经说明一个关键前提：

- 产品模式还没完全稳定

尤其是：

- 持久助手模式还在定义阶段
- app-server 线程模式字段还未落地
- observer 事件流还只是第一阶段设计

如果这时就把大量运行态和派生状态一口气压进 SQLite，很容易出现两个问题：

- 把尚未稳定的产品语义过早固化成数据库模型
- 为了迁就数据库结构，反过来拖累产品和协议设计

所以 SQLite 应该是收敛点，而不是路线起点。

## 3. 设计目标

这份设计草案只追求四件事：

1. 识别哪些状态已经足够稳定，适合继续收敛到 SQLite
2. 识别哪些状态仍然过于瞬时，不适合第一阶段落盘
3. 明确 SQLite 与 rollout、线程运行态、observer 的分工
4. 给后续实现划出渐进迁移路径，而不是一次性大重构

## 4. 非目标

第一阶段不做：

- 不重写现有 rollout 持久化
- 不把所有通知和事件都变成数据库记录
- 不把 `ThreadStatus` 直接逐字段镜像成永久表
- 不为了 SQLite 去反向塑形 app-server 协议
- 不引入复杂分布式同步或远程数据库

SQLite 在这里的定位仍然是：

- 本地嵌入式状态收敛层

## 5. 当前状态来源的大致分层

如果把当前 `codex-rs` 的状态来源粗分，可以大致分成四类。

### 1. rollout 文件

更偏：

- 历史
- 可重建对话材料
- 事件序列

### 2. 内存运行态

更偏：

- 当前 loaded 线程
- 当前 turn
- 当前等待状态
- 当前 watcher / 连接 / 运行时资源

### 3. app-server 状态面

更偏：

- 面向客户端暴露的当前线程摘要
- `ThreadStatus`
- `workspaceChanged`
- resident / loaded 等运行时语义

这里也要继续遵守协议侧已收口的边界：

- `thread/loaded/read` 是带 `mode + status` 的当前摘要读取面
- `thread/status/changed` 只负责推送后续 `status` 增量
- 不应把“status-only 通知”和“完整线程恢复摘要”混成同一层语义
- 如果客户端还要进一步把线程摘要转成动作文案，也应继续基于读取面
  拿到的 `Thread.mode` 做映射，而不是从 SQLite 元数据或 status-only
  通知里临时推断：
  - `interactive -> resume`
  - `residentAssistant -> reconnect`

### 4. SQLite

更偏：

- 稳定元数据
- 可查询状态
- 可恢复派生信息
- 不适合只存在于内存、但也不必完整保留为 rollout 事件流的事实

这四层不应该被粗暴合并。

## 6. 哪些状态值得优先收敛到 SQLite

第一阶段以后，最值得继续向 SQLite 收敛的是“恢复后仍然有价值的稳定状态”。

### 线程元数据

这是当前最成熟的一类，包括：

- thread identity
- cwd
- created / updated 时间
- git metadata
- agent nickname / role
- rollout path 修复与关联

这些状态天然适合 SQLite，因为：

- 它们查询频繁
- 生命周期长
- 恢复时有价值
- 没必要每次都从 rollout 全量重建

### 持久助手线程元数据

一旦线程模式字段落地，下一类值得收敛的是：

- 线程模式
- 驻留相关元数据
- 更稳定的恢复游标
- 线程级后台能力开关或长期属性

这类信息适合作为“线程的稳定身份信息”，而不是瞬时事件。

### observer 派生摘要

observer 第一阶段的原始事件本身未必需要直接落盘，但其摘要逐步适合进入 SQLite，例如：

- 最近一次工作区变化时间
- 是否存在尚未吸收的工作区变化
- 某些可恢复的观察派生状态

关键点是：

- 存摘要，不存原始事件洪流

### 后台任务元数据

如果长期线程开始承载更多后台任务，比较适合收敛到 SQLite 的是：

- 任务 identity
- 所属 thread
- 生命周期阶段
- 最后更新时间
- 最后结果摘要或错误摘要

而不是每个中间事件都直接永久落盘。

## 7. 哪些状态不应第一阶段直接落 SQLite

### 瞬时运行态

例如：

- 当前是否有活跃连接
- 当前 watcher 句柄
- 当前 in-memory subscription
- 当前 streaming 中的细粒度执行状态

这些状态天然属于进程内运行态，不应为了“统一”而强行落盘。
同理，`thread/status/changed` 这类 status-only 增量通知也不应被误当成
SQLite 需要持久化的完整线程摘要来源；真正稳定的恢复信息仍应优先来自
`threads.mode` 这类持久元数据和 `thread/read` / `thread/list` /
`thread/loaded/read` 这些读取面。

### 高频原始文件变化

observer 的原始文件变化事件不适合第一阶段直接全量写 SQLite，原因包括：

- 频率高
- 噪声多
- 真实长期价值不稳定
- 很容易把数据库变成日志水位系统

第一阶段更合理的是只保留派生摘要。

### 客户端展示层拼接结果

例如：

- 某个客户端当前如何组合状态标签
- 某个列表当前如何排序和过滤

这类内容属于消费层，不应被反向存进 SQLite。

## 8. 与 rollout 的关系

SQLite 不是 rollout 的替代品，两者职责不同。

更合理的分工是：

- rollout 保留历史和可重放材料
- SQLite 保留稳定元数据与可查询派生状态

尤其在第一阶段，不应尝试把 rollout 的全部语义投影成数据库表。  
这样做成本高，而且很容易造成双重真相来源。

## 9. 与 observer 的关系

observer 文档里已经强调：

- observer 是事件源

在这份文档里要再强调一次：

- SQLite 不是 observer 本身
- SQLite 更适合存 observer 的稳定派生摘要

一个更合理的顺序是：

1. 先定义 observer 事件流
2. 再看哪些 observer 派生状态值得持久化
3. 最后才决定 SQLite 模型

不要反过来先建表，再逼 observer 去适配表结构。

## 10. 与 app-server 的关系

app-server 当前已经在读写 SQLite-backed thread metadata。  
这说明 SQLite 继续成为 app-server 的摘要与恢复后端是合理方向。

但仍应保持一个边界：

- app-server 返回的是面向客户端的线程语义
- SQLite 保存的是其中值得长期持有的稳定底座

不要让 app-server 的每个临时状态都自动等于数据库字段。

这里还值得把当前更贴近真实实现的“读取面先信谁”顺序直接写短一点，
避免后续 repair / restore 改动重新把 fallback 逻辑打散。

### 当前更贴近实现的优先级

- `thread/read`
  先组出可读的 stored summary；优先是 state-db summary，其次才回退到 rollout summary，再不行才回退到 loaded snapshot
- `thread/read`
  基础摘要组好后，再补 SQLite 持久化的 resident metadata（尤其是 `threads.mode`），最后附着 runtime metadata 与 loaded status
- `thread/list`
  先以 stored summaries 为主，再叠加当前 loaded status；resident live set 命中时优先保留 runtime resident 语义，否则再回退到 SQLite 持久化 mode
- `thread/list`
  这里的 “stored summaries” 不能被理解成“SQLite 行存在即可”；如果 SQLite 线程行只具备 identity / `mode`，但还缺 rollout-derived `first_user_message` / preview 这类列表面必需字段，那么 read-repair 仍应继续回到 rollout reconcile，把摘要补完整后再让 DB-backed listing 成为权威来源
- `thread/loaded/read`
  先从当前 loaded runtime thread 组运行态摘要，再附着持久化 resident metadata，最后叠加 live status；不要把它反过来理解成“从 SQLite 读一行再顺手补 loaded 标记”
- `thread/loaded/read`
  这条路径现在也应继续和 loaded `thread/read` / `thread/list` 共享同一条 loaded-summary helper：如果 rollout metadata 缺失但当前 loaded thread 仍持有 provider override 或外部 rollout path，loaded polling 返回面本身就应继续保留这组摘要，而不是只在某一条读取面里单独正确
- `thread/resume`
  如果线程当前仍在运行，优先信 live runtime thread；如果需要从历史恢复，则先按 rollout/history 回建，再补持久化 resident metadata，最后附着 runtime metadata
- `thread/unarchive` / `thread/metadata/update`
  这类 metadata-first 恢复路径应直接信更新后的持久化 thread metadata 返回值，而不是要求消费侧再补一次 `thread/read` 去恢复 resident 语义
- `thread/resume`
  这条路径的返回值本身也应继续被消费侧当成权威 repaired summary；如果 persisted row 仍缺 rollout-derived preview，服务端应先 reconcile rollout，而不是让客户端在 reconnect 后再自行补读

换句话说，当前更接近真实实现的心智不是“SQLite、rollout、runtime 三选一”，而是：

- stored summary 提供基础线程骨架
- SQLite metadata 提供稳定 resident identity
- runtime metadata 提供当前 loaded / observer 状态

这里还需要再加一条更贴近实现的约束：

- “stored summary” 不是抽象占位，而是一组会直接影响读取面/列表面的具体字段
- 如果 SQLite 已经有 thread 行，但这组 rollout-derived 摘要字段仍残缺，那么 `thread/read` 与 `thread/list` 很容易重新分叉：前者还能从 rollout fallback 看见线程，后者却可能因为 DB 过滤条件把同一线程直接漏掉
- 因此更合理的 read-repair 心智不是“先把路径修到 SQLite 就算完成”，而是“只要摘要字段还不完整，就继续 reconcile rollout，直到 SQLite 能独立支撑 stored summary”

如果后续还要继续压缩成 review 问句，最值得先问的是：

- 这个路径的基础摘要先来自哪里
- resident identity 是不是仍由 SQLite metadata 兜底
- loaded / observer 状态是不是仍只由 runtime metadata 附着
- 返回值是否已经足够让消费侧直接信 `thread.mode`，而不是再补一次读取或自己脑补

如果需要把它进一步压成提交前的最短检查，可以直接用下面这五句：

1. 这个路径的基础摘要先信哪里
2. resident identity 还是不是 SQLite metadata 兜底
3. loaded / observer 状态有没有被错误持久化
4. SQLite 行残缺时会不会继续 reconcile rollout
5. 消费侧是否还能直接信返回的 `thread.mode`
6. README / typed client 契约是否仍把这些返回面写成权威 repaired summary，而不是重新暗示“再补一次 `thread/read`”

## 11. 渐进实施建议

如果按阶段推进，我会建议下面这个顺序。

### 阶段 1：巩固现有 thread metadata 路径

重点是：

- 继续让 thread metadata 成为查询首选
- 保持与 rollout 的修复和回填路径稳定
- 不引入大规模新表

这一步实际上已经开始落地，而且边界比草案阶段更清楚：

- SQLite 已经是 resident thread 模式恢复的主干来源之一，而不再只是 rollout 的附属缓存
- rollout -> SQLite 的修补路径已经覆盖 resident thread 的 start / resume / fork / archive / unarchive / metadata update 等主入口
- 当前更需要做的是继续补齐边缘恢复面和消费面，而不是重新争论 thread metadata 要不要作为主干

### 阶段 2：线程模式与长期线程元数据入库

在 `Thread.mode` 语义稳定后，再考虑把相关稳定元数据进入 SQLite。

这一阶段也已经部分开始实现：

- `Thread.mode` 已经进入协议、app-server 返回面和 SQLite 稳定元数据
- in-process / 手工联调消费面也已开始把这条 SQLite 边界钉实：`codex-app-server-client` 已补上 metadata-only update 保留 resident `mode` 的 typed request 回归，`codex-app-server-test-client` 也已补上 `thread-read` / `thread-metadata-update` 的 resident-aware 摘要入口
- 当前剩余工作更偏向“继续补 resident mode 在边缘恢复路径与消费侧的连续性”，而不是重新设计 mode 字段本身

### 阶段 3：observer 派生摘要入库

等 observer 事件流稳定后，再收敛：

- 最近变化时间
- 是否有未吸收变化
- 其他真正被恢复逻辑消费的派生状态

### 阶段 4：后台任务元数据入库

当长期线程和后台任务语义稳定后，再把任务元数据正式收敛进去。

这一步应该晚于产品模式落地，而不是先行。

## 12. 第一阶段验收标准

围绕 SQLite 收敛这条线，第一阶段不应追求“更多表”，而应追求“更清楚的分工”。

至少应满足：

- 文档上明确哪些状态适合入 SQLite，哪些不适合
- 不为瞬时运行态强行建表
- thread metadata 路径继续保持主干地位
- observer 和后台任务在语义未稳前不被过早固化

## 13. 下一步文档

在这份文档之后，更合理的下一份文档不一定再是状态层，而可能是：

- 远程 bridge 如何消费长期线程、线程模式、observer 摘要与 SQLite 元数据

也就是说，SQLite 收敛文档更像状态基础设施阶段的收束点，而不是下一轮继续膨胀的起点。

如果要先把前面几份设计文档收敛成工程任务，优先补的不是新设计，而是：

- `docs/persistent-runtime-implementation-plan.md`

如果要继续补消费层设计，则下一份应是：

- `docs/remote-bridge-consumption.md`
