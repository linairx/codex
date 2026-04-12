# SQLite State Convergence File TODO

本文承接：

- `docs/sqlite-state-convergence-checklist.md`
- `docs/persistent-runtime-checklists-index.md`

目标不是再补一层 SQLite 总设计，而是把当前这包“状态来源收敛”继续压成文件级待办，方便直接开工、review 和拆 PR。

## 0. 当前工作树对应关系（2026-04-13）

按当前本地改动集看，这一包已经不再是“还没落代码”的阶段，而是进入了“继续收口 SQLite / rollout / runtime 之间剩余分叉”的阶段。

当前已经明显命中本地改动的主要文件或目录：

- `codex-rs/rollout/src/state_db.rs`
- `codex-rs/app-server/src/codex_message_processor.rs`
- `codex-rs/app-server/tests/suite/v2/thread_read.rs`
- `codex-rs/app-server/tests/suite/v2/thread_resume.rs`
- `codex-rs/app-server/tests/suite/v2/thread_metadata_update.rs`
- `codex-rs/app-server/tests/suite/v2/thread_unarchive.rs`
- `codex-rs/app-server/tests/suite/v2/thread_rollback.rs`
- `codex-rs/app-server-client/src/lib.rs`
- `codex-rs/app-server/README.md`
- `codex-rs/app-server-client/README.md`
- `docs/sqlite-state-convergence.md`
- `docs/sqlite-state-convergence-checklist.md`

如果目标是继续沿这条线推进，更合理的顺序是：

1. 先收 `rollout/state_db` 的 repair / reconcile helper
2. 再收 `app-server` 主要读取面和边缘恢复面
3. 再补服务端与 typed client 回归
4. 最后只做必要的 README / checklist 同步

补充说明：

- 这份 TODO 默认假设 resident continuity 基线、remote bridge 最小消费和 observer 读取面都已经基本站稳
- 因此这里的主问题不再是“加更多 mode 字段”，而是“继续消灭半修复状态和分叉的权威来源”

## 1. 这个 TODO 要服务哪个阶段

它只服务同一个目标：

- SQLite 稳定元数据与状态来源收敛

如果后续工作已经进入：

- 新的 remote bridge transport
- 新的 observer API
- 后台任务元数据或多 agent 持久化

那么应回到对应设计稿或 checklist，而不是继续往这份 TODO 里塞新任务。

## 2. rollout / state-db 入口

### `codex-rs/rollout/src/state_db.rs`

待检查：

- `read_repair_rollout_path()` 是否继续把“SQLite 行已存在但 stored summary 残缺”当成需要完整 reconcile 的情形，而不是停在只修路径
- `reconcile_rollout()` 是否继续把 rollout-derived summary 补回 SQLite，而不是只写 thread identity
- archived / unarchived 路径是否继续正确传递 archived 语义，而不是让 repair 写回错误状态

最值得补的负向边界：

- SQLite row 已存在但 `first_user_message` 为空时，不会继续把这条线程当成“已可稳定 listing”的完整行
- fast path 不会把 rollout summary 缺口悄悄留给上层读取面自己兜底

完成标志：

- state-db repair helper 已经明确区分“只修路径”与“必须重新 reconcile rollout”

## 3. app-server 读取面与恢复面

### `codex-rs/app-server/src/codex_message_processor.rs`

待检查：

- `thread/read` 是否继续按“stored summary -> persisted mode -> runtime metadata”组装线程摘要
- `thread/list` 是否继续把 stored summary 当基础骨架，而不是让 SQLite 残缺行和 rollout fallback 各说各话
- `thread/resume` 是否继续在恢复前修补 persisted summary，而不是只让响应面临时正确
- `thread/unarchive` / `thread/metadata/update` / `thread/rollback` 是否继续共用同一类 persisted metadata / runtime metadata helper，而不是各自维护一份 repair 逻辑
- 任何从 state-db 读取线程摘要的 helper，是否都继续走同一条 summary-repair 边界，而不是局部直接 `get_thread()`

当前这条线里已经可以视为收口的点：

- `thread/rollback` 返回面已经继续走 `load_thread_summary_for_rollout(...)` 这条合并 persisted metadata 的路径，而不是单独重建另一套 rollback-only summary
- rollback 前如果 SQLite row 已存在、但 rollout-derived preview 仍缺失，服务端与 typed client 回归都已经锁住：rollback 响应、后续 `thread/read` / `thread/list`，以及 SQLite row 会一起恢复同一份 preview，而不是只保 resident mode

最值得补的负向边界：

- 不会出现 `thread/read` 已修好 preview、但 `thread/list` / summary helper 仍拿到空 preview
- 不会出现 resident mode 仍然正确，但 preview / name / git metadata 只在部分恢复面被修补
- 不会把 loaded status、observer 状态或其他 runtime-only 字段写回 SQLite 当成稳定元数据

完成标志：

- app-server 内部不再残留多套“缺摘要 repair”或“resident continuity overlay”实现

## 4. 服务端测试面

### `codex-rs/app-server/tests/suite/v2/`

优先检查的文件：

- `thread_read.rs`
- `thread_resume.rs`
- `thread_metadata_update.rs`
- `thread_unarchive.rs`
- `thread_rollback.rs`

需要优先补或确认的边界：

- stored thread 缺摘要时，`thread/read` 与 `thread/list` 继续一起恢复 preview
- archived thread 缺摘要时，读取面、列表面和持久化行继续保持一致
- `thread/resume` / `thread/metadata/update` / `thread/unarchive` / `thread/rollback` 返回面本身已经是修补后的权威摘要
- rollback / repair / reconnect 不会各自重新引入另一套 stored-summary overlay

完成标志：

- 关键恢复路径的断言不再只验证 `mode`，也会一起验证 preview / persisted row 已同步修补

## 5. typed client 面

### `codex-rs/app-server-client/src/lib.rs`

待检查：

- in-process typed 请求是否继续直接信服务端返回的修补后 `Thread`
- remote facade 是否继续原样透传 repaired summary，而不是自己再补一次读取
- archived / repaired / resumed 路径是否继续对称覆盖

最值得补的负向边界：

- typed client 不会只在 `thread/read` 看见 repaired preview，却在 `thread/resume` / `thread/unarchive` / `thread/rollback` 上退回半修复摘要
- remote facade 不会把“服务端已经修补好 stored summary”重新降级成客户端二次补读职责

完成标志：

- typed client 层不再残留“metadata-only 响应后再补一次 `thread/read`”的旧心智

## 6. 文档同步面

### `codex-rs/app-server/README.md`

待检查：

- `thread/read`
- `thread/list`
- `thread/resume`
- `thread/metadata/update`
- `thread/unarchive`

这些读取面是否继续明确写成：

- 返回值本身就是权威线程摘要
- 调用方应直接信 `thread.mode` 与摘要字段
- `thread/status/changed` 仍不是完整恢复来源

按 2026-04-13 当前本地文档状态，这一层已经基本对齐实现：

- `thread/list`、`thread/resume`、`thread/read`、`thread/metadata/update`、`thread/unarchive` 都已开始明确写成 authority-first 的 repaired summary 读取面
- README 不再把这些返回面描述成“先拿 mode，再额外补一次 `thread/read`”的半恢复响应

### `codex-rs/app-server-client/README.md`

待检查：

- typed 调用示例是否继续把服务端返回的 `Thread` 当成 `resume or reconnect` 的启动摘要
- 是否没有重新暗示“metadata update / resume 之后再做一次 `thread/read`”

按 2026-04-13 当前本地文档状态，这一层也已经基本对齐实现：

- in-process typed client README 已明确把 `thread/resume` 返回值写成 `resume or reconnect` 的启动摘要
- README 也已明确把 `thread/read` / `thread/list` / `thread/resume` / `thread/metadata/update` / `thread/unarchive` 这些 repaired 返回面写成可直接信任的权威摘要
- remote facade 也已补上同层口径：websocket 远端直接返回 repaired `Thread` 时，typed remote client 应继续原样保留该摘要，而不是重新引入客户端补读

### `docs/sqlite-state-convergence.md` 与 `docs/sqlite-state-convergence-checklist.md`

待检查：

- 权威来源优先级是否仍和实现一致
- “stored summary 也有完整度要求” 这条约束是否继续说清

按 2026-04-13 当前本地文档状态，这一层也已经开始收口：

- `docs/sqlite-state-convergence.md` 已继续把 `thread/read` / `thread/list` / `thread/resume` 的优先级与 repaired-summary 契约写短
- `docs/sqlite-state-convergence-checklist.md` 已明确把 README / typed client 的这层契约也纳入阶段边界，而不是只把它留在测试里

完成标志：

- 代码、README 和 checklist 不再分别讲不同的 repair 心智

## 7. 建议提交前顺序

建议按这个顺序整理：

1. 先收 `rollout/state_db` 与 `app-server` helper
2. 再收服务端回归
3. 再收 typed client 回归
4. 最后补 README / checklist / 设计稿里的阶段结论

如果某个改动只是补文档，而没有顺手确认对应 helper 和测试，这一包通常还不能算真正收口。

## 8. 这份 TODO 收完后做什么

这份文件级 TODO 收完后，不应继续在这里追加新的 remote bridge、observer 或后台任务任务包。

更合理的下一步是：

- 整理当前 SQLite 收敛 PR
- 或切到更高层消费方真正依赖这层稳定摘要的实现
