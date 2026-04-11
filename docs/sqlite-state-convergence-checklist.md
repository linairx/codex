# SQLite State Convergence Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/sqlite-state-convergence.md`
- `docs/observer-event-source-checklist.md`

目标不是再讨论 SQLite 为什么重要，而是把“哪些线程状态应该收敛进 SQLite、哪些不该进”收成一份可直接执行的清单。

## 1. 这一阶段要解决什么

这一阶段的唯一主问题应该是：

- 收敛线程稳定元数据与权威来源，减少 rollout、SQLite、runtime snapshot 各说各话

它适合覆盖的内容包括：

- resident / mode 相关稳定元数据
- archive / unarchive / repair / metadata update 路径上的权威来源
- 读取面在 SQLite / rollout / runtime 之间的优先级
- 哪些状态适合长期保存，哪些只适合 runtime 或 event stream

它不应混入：

- 新 bridge transport
- 新 observer API
- 完整 runtime 镜像入库
- 全量 watcher 事件日志化
- 多 agent 持久化大改

## 2. 先读哪些文档和代码

建议按下面顺序进入：

1. `docs/sqlite-state-convergence.md`
2. `docs/observer-event-flow-design.md`
3. `codex-rs/state/`
4. `codex-rs/app-server/src/codex_message_processor.rs`
5. `codex-rs/app-server/tests/suite/v2/`
6. `codex-rs/app-server-client/`

这些位置分别对应：

- SQLite 状态分层设计
- observer 与状态层的边界
- SQLite runtime / model 实现
- 服务端读取面与 repair / restore 路径
- 集成回归入口
- typed 消费侧回归

## 3. 开发时逐条检查什么

### 状态分层

- 哪些字段属于稳定身份元数据
- 哪些字段属于当前 loaded runtime snapshot
- 哪些字段只适合由 rollout 派生
- 哪些字段只适合由 event stream 暴露

### 权威来源

- `thread/read` 先信谁
- `thread/list` 先信谁
- `thread/resume` 先信谁
- `thread/unarchive` 先信谁
- `thread/metadata/update` repair 路径先信谁

### 关键恢复路径

- archive
- unarchive
- metadata repair
- rollback 后再读取
- resident reconnect

这些路径最容易暴露“SQLite、rollout、runtime 各说各话”的问题。

### 不该做的事

- 不把所有 runtime 细节一股脑塞进 SQLite
- 不把 status-only 增量误当成完整恢复来源
- 不把 observer 原始事件直接当成第一批必须持久化的数据

## 4. 最值得补的负向边界

- 缺失 SQLite 行时 repair 不会把 resident thread 降回 interactive
- metadata update 不会覆盖已持久化的 resident mode
- archived resident thread 读取面不会退回普通 interactive 摘要
- unarchive 后不会因为只看 rollout summary 而丢 resident mode
- rollback / repair / reconnect 不会各自维护不同的一套 mode / name overlay 逻辑

## 5. 最先跑哪些测试

优先跑：

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-state`

如果改动直接触及 state runtime 或 model 层，先把 `codex-state` 跑稳，再扩大到 `app-server` 读取面与消费侧。

## 6. Review 时最值得问什么

1. 这个改动是不是只在收敛状态来源，而不是顺手引入新模型
2. 每条关键读取/恢复路径的权威来源是否比以前更清楚
3. resident continuity 是否在 stored / loaded / archived 三个面都一致
4. observer 状态有没有被错误持久化成过重的 SQLite 负担
5. 有没有补足 archive / unarchive / repair / metadata update / rollback 这些边界测试

## 7. 什么情况下这一阶段算完成

至少应满足：

- 稳定身份元数据与 runtime 状态的边界更清楚
- 主要读取面在 SQLite / rollout / runtime 之间的优先级更清楚
- resident thread 在 stored / loaded / archived / repaired 路径上保持一致
- repair 与 metadata update 不再意外破坏 resident continuity
- typed client / app-server 读取面不需要再各自补一层临时解释

## 8. 这一阶段完成后下一步做什么

更合适的下一步是：

- 更完整的 remote bridge 或更上层产品模式

因为只有当状态来源真正收敛后，后续 bridge、多 agent、后台任务面板之类能力才更容易建立在稳定摘要之上，而不是继续围着 repair 和 fallback 打转。
