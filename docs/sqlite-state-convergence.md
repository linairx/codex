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

## 11. 渐进实施建议

如果按阶段推进，我会建议下面这个顺序。

### 阶段 1：巩固现有 thread metadata 路径

重点是：

- 继续让 thread metadata 成为查询首选
- 保持与 rollout 的修复和回填路径稳定
- 不引入大规模新表

### 阶段 2：线程模式与长期线程元数据入库

在 `Thread.mode` 语义稳定后，再考虑把相关稳定元数据进入 SQLite。

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
