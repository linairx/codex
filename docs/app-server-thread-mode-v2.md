# app-server v2 线程模式字段设计

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-assistant-mode-design.md`

目标是把“持久助手模式”在 `app-server v2` 协议上的最小改动单独说明清楚，避免把产品模式、状态模型、observer、SQLite 和远程 bridge 混在同一份文档里。

## 1. 背景

当前 `app-server v2` 已经有以下线程相关语义：

- `Thread.resident: bool`
- `ThreadStatus`
- `ThreadStatus::Active { activeFlags }`

这套结构已经足以表达：

- 线程是否保活
- 线程是否 loaded
- 当前是否存在审批、用户输入、后台 terminal、workspace 变化等活动事实

但它还不能稳定回答一个更基础的问题：

- “这个线程是什么类型的线程”

对于普通交互线程，这个缺口问题不大。  
对于长期驻留助手线程，这会直接影响：

- 列表展示
- 恢复语义
- 客户端文案
- 远端控制平面如何解释 `thread/resume`

所以第一阶段建议在 `Thread` 上补一个显式模式字段。

## 2. 设计目标

这次协议改动只追求三件事：

1. 让客户端可以直接知道线程类型，而不是靠 `resident + status + source` 猜
2. 保持与现有 `resident` 和 `ThreadStatus` 兼容
3. 给后续模式扩展留位置，但不强行一次性设计完整矩阵

## 3. 非目标

这份协议说明明确不处理：

- 线程状态大重构
- `thread/status/changed` 新事件模型
- observer 事件定义
- SQLite 状态持久化方案
- 远程 bridge transport
- 多 agent 线程编排协议

如果这些内容需要推进，应另写文档。

## 4. 当前问题

### `resident` 不是线程模式

`resident: bool` 的现实意义是：

- 最后一个客户端断开后，该线程是否继续保持 loaded

它不适合直接承担“线程类型”的语义，原因有三点：

- 它描述的是生命周期策略，不是产品类型
- 它无法区分“长期驻留助手线程”和未来其他同样需要保活的线程
- 客户端仍需要结合其他字段做不稳定推断

### `source` 也不适合作为替代

`source` 更偏“线程来自哪里”，例如 CLI、TUI、app-server 或其他入口。  
它不等于线程模式。

同一个 `source` 既可能启动普通交互线程，也可能启动长期驻留线程。

### `status` 更不应该承担模式语义

`status` 回答的是：

- 线程现在处于什么运行态

它不应该被滥用来表达：

- 线程属于哪一类产品模式

否则模式和状态会缠在一起，后面很难维护。

## 5. 建议字段

建议在 `Thread` 上增加一个新字段：

```text
mode: ThreadMode
```

建议的第一阶段枚举值：

- `interactive`
- `residentAssistant`

后续可预留但先不实现的潜在值：

- `planner`
- `coordinator`
- `backgroundWorker`

第一阶段只要求前两个值可用。

## 6. 为什么第一阶段只放在 `Thread` 上

第一阶段建议只在返回的 `Thread` 结构里新增 `mode`，原因是：

- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/read`
- `thread/list`
- `thread/loaded/read`
- `thread/started` 通知

这些返回面本来就以 `Thread` 为中心。  
只要 `Thread` 统一带 `mode`，大多数客户端就已经能消费该语义。

这比同时修改多套独立响应字段更稳，也更容易兼容。

与之相对，`thread/loaded/list` 这类只返回线程 id 的接口不应被当成完整 loaded
恢复摘要面。
如果客户端既需要 loaded 集合，又需要区分 reconnect 语义、线程角色或当前
runtime status，应该继续读取 `thread/loaded/read`，而不是期待 id-only
接口重复 `Thread.mode` 或补充当前 `status`。

## 7. 请求侧建议

第一阶段有两种可接受路径。

### 路径 A：暂时保留 `resident` 请求参数，服务端推导 `mode`

这是最保守的方案：

- `thread/start` 继续接受 `resident: bool`
- `thread/resume` 继续接受 `resident: bool`
- 服务端在返回 `Thread` 时填充 `mode`

这种方式的优点是：

- 改动最小
- 不破坏现有客户端
- 可以先让读取侧和展示侧稳定下来

缺点是：

- 请求侧语义仍不够直观

### 路径 B：新增请求侧模式字段，但保留 `resident`

这是更完整但稍重一点的方案：

- 新增可选 `mode`
- 第一阶段要求 `mode` 与 `resident` 保持兼容
- 当两者同时出现冲突时，服务端明确报错或定义优先级

如果追求范围可控，我更倾向于先做路径 A，再在下一步把请求侧也切到模式字段。

## 8. 第一阶段推荐兼容策略

第一阶段推荐如下策略：

- 返回侧：新增 `Thread.mode`
- 请求侧：保留现有 `resident`
- 服务端：
  - 普通线程返回 `mode = interactive`
  - 长期驻留助手线程返回 `mode = residentAssistant`
  - `resident = false` 通常对应 `interactive`
  - `resident = true` 在第一阶段可映射到 `residentAssistant`

这样做虽然还不完美，但可以先把最重要的读取和展示问题解决掉。

## 9. 与 `thread/fork` 的关系

`thread/fork` 也返回 `Thread`，因此也需要明确该字段。

第一阶段建议：

- 默认 fork 出来的线程仍是 `interactive`
- 不因为源线程是 `residentAssistant`，就自动让 fork 线程继承为 `residentAssistant`

原因是：

- fork 更像新线程派生，而不是原线程身份延续
- 自动继承长期驻留语义容易制造意外保活

如果未来确实需要“派生出新的长期线程”，应通过显式请求参数控制，而不是隐式继承。

## 10. 与 `ThreadStatus` 的关系

第一阶段不建议修改 `ThreadStatus` 枚举结构。

原因很直接：

- 它已经能表达当前运行时活动事实
- 当前缺的是线程类型，不是状态枚举能力
- 继续往 `ThreadStatus` 塞“resident assistant”之类值，会让模式和状态耦合

协议上应该坚持：

- `mode` 负责线程身份
- `status` 负责线程当前运行态

这也意味着 `thread/status/changed` 应继续保持 status-only：

- 它负责增量推送运行态变化
- 不负责重复 `Thread.mode`
- 客户端需要从前面的 `thread/started`、`thread/read`、`thread/list`、
  `thread/loaded/read` 等恢复面保留线程角色

## 11. 返回面影响

第一阶段新增 `Thread.mode` 后，应该统一出现在所有返回 `Thread` 的接口与通知中，包括：

- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/read`
- `thread/metadata/update`
- `thread/list`
- `thread/loaded/read`
- `thread/started`
- 其他任何嵌套返回 `Thread` 的线程相关响应

这样客户端才不需要在不同 API 上做“有的接口有 mode、有的没有”的补丁逻辑。
特别是 `thread/metadata/update` 这类 metadata-only 路径，也不应被例外对待，否则客户端很容易重新引入“更新完 metadata 还要再补一次 `thread/read` 才能恢复 resident reconnect 语义”的旧耦合。

同一条原则也应继续适用于后续的 stored-summary repair：

- 如果某条线程已经有持久化元数据，但 rollout-derived summary 仍残缺，服务端应优先在这些返回面上把摘要修补好
- 客户端应继续把 `thread/read`、`thread/resume`、`thread/metadata/update`、`thread/list`、`thread/loaded/read`，以及后续 restore 路径返回的 `Thread` 当作权威摘要，而不是把 rollout-vs-SQLite 的 reconcile 责任重新推回消费侧

这条约束虽然更完整地属于后续 SQLite 收敛文档，但这里值得先写清，因为它直接决定 `Thread.mode` 一旦进入返回面后，客户端是否还能继续相信“返回的 `Thread` 就是当前权威摘要”。

## 12. 文档与客户端迁移建议

### README

`app-server/README.md` 至少需要补清楚：

- `Thread.mode` 的语义
- `resident` 与 `mode` 的区别
- `thread/resume` 对 `residentAssistant` 更偏“重新连接”
- 客户端动作文案应直接按 `Thread.mode` 映射：`interactive -> resume`、`residentAssistant -> reconnect`
- `thread/status/changed` 只推送 runtime `status`，不重复 `mode`
- `thread/loaded/list` 只是 id-only probe；需要 reconnect 语义、线程角色或
  当前 runtime status 时应继续读 `thread/loaded/read`
- metadata-only / restore 路径返回的 `Thread` 也应继续被写成权威摘要，而不是要求客户端再补一次 `thread/read` 去修 resident 语义或 preview

### 客户端

客户端迁移建议是：

- 优先消费 `Thread.mode`
- 逐步减少对 `resident` 的直接产品展示依赖
- 保留对旧服务端缺失 `mode` 的兼容兜底

兼容兜底可以短期采用：

- 无 `mode` 时，根据 `resident` 推断是否表现为长期线程

但这应是过渡逻辑，不是长期协议约定。

## 13. 第一阶段验收标准

这份协议改动完成后，至少应满足：

- 所有主要线程读取接口都返回 `Thread.mode`
- 客户端能稳定区分 `interactive` 与 `residentAssistant`
- 不需要改动 `ThreadStatus` 就能支撑第一阶段产品语义
- 旧客户端即使忽略 `mode` 也不会立刻坏掉

## 14. 后续演进

在这一版协议稳定之后，再考虑：

1. 是否在请求侧引入显式 `mode`
2. 是否让 `resident` 逐步退化为内部兼容字段
3. 是否为多 agent / planner / background worker 扩展更多模式值
4. 是否让更多客户端完全迁移到 `mode` 驱动展示

这几个步骤都应晚于第一阶段的读取侧落地。

与这份协议文档并行但独立的下一份基础设施文档应是：

- `docs/observer-event-flow-design.md`

再下一份状态层文档应是：

- `docs/sqlite-state-convergence.md`

如果前面三份设计稿已经齐备，下一步更实际的不是继续抽象设计，而是整理：

- `docs/persistent-runtime-implementation-plan.md`
