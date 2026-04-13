# SQLite State Convergence Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/sqlite-state-convergence.md`
- `docs/observer-event-source-checklist.md`
- `docs/sqlite-state-convergence-file-todo.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

目标不是再讨论 SQLite 为什么重要，而是把“哪些线程状态应该收敛进 SQLite、哪些不该进”收成一份可直接执行的清单。

## 0. 当前本地进度快照（2026-04-13）

按当前文档链和已落地的 resident continuity 回归看，SQLite 这一阶段也不再是纯前瞻话题，而是已经有一批“稳定元数据谁说了算”的边界进入真实实现。

当前已经可以明确视为这条线已落地的点包括：

- `threads.mode` 已经作为 resident assistant 的稳定线程模式元数据进入 SQLite，并被 `thread/read`、`thread/list`、`thread/resume` 这些主要读取面消费
- archive / unarchive / metadata update / rollback 相关路径已经开始围绕同一条 resident continuity 主线收敛，而不是分别维护多套 overlay 逻辑
- `metadata update` / repair 的边界已经补上 resident mode 连续性保护：缺失或修补 SQLite 行时，不会把 resident thread 静默降回 interactive
- `app-server-client` README 和 typed 回归也已开始把这条边界写实：metadata-only update、unarchive、post-unsubscribe reconnect、rollback 这些路径都应直接信任返回的 `thread.mode`，而不是要求消费侧再补一次 `thread/read` 去恢复 resident 语义
- rollout/SQLite 的摘要收敛现在也已补上一个更细的 repair 边界：如果 SQLite 里已经有 thread 行，但 rollout-derived summary 字段还残缺，`read_repair_rollout_path()` 不会再只修路径而保留空摘要；它会继续回到 rollout reconcile，把 `first_user_message` 这类 stored summary 字段补齐，避免 `thread/read` 能从 rollout 看见线程、但 `thread/list` 因 DB fallback 过滤条件而把同一线程静默丢掉
- `codex-rollout` 现在也已把这条 helper 级快路径边界单独锁住：当 SQLite row 已存在、路径本身也已经正确、但 rollout-derived summary 仍缺失时，`read_repair_rollout_path()` 仍会继续走 rollout reconcile，把 `first_user_message` 补回，而不是把半修复状态留给 `thread/read` / `thread/list`
- `codex-rollout` 现在也已把同一 helper 的 archived 对称面锁住：即使 `read_repair_rollout_path()` 在 `archived_only = true` 下遇到的是“已有 SQLite row + 缺 summary”的半修复状态，它也会继续 reconcile rollout，并同时把 archived 语义正确写回，而不是只补摘要或把 archived 标记写散
- `codex-rollout` 现在也已把同一 helper 的 unarchive 对称面锁住：如果已有 SQLite row 仍带着旧的 archived 标记、但又需要靠 rollout reconcile 补回 summary，`read_repair_rollout_path()` 在 `archived_only = false` 下也会一起清掉 `archived_at`，避免修完摘要后仍把线程留在错误的 archived 状态
- `codex-rollout` 现在也已把同一 helper 的 slow-path archived 入口锁住：当 SQLite row 已缺失、只能靠 rollout 重建 metadata 时，`read_repair_rollout_path()` 在 `archived_only = true` 下也会直接重建带 archived 语义的完整线程行，而不是只补 path/summary 却把 archived 状态漏掉
- `codex-rollout` 现在也已把同一 helper 的 slow-path unarchive 入口锁住：当 SQLite row 已缺失、只能靠 rollout 重建 metadata 时，`read_repair_rollout_path()` 在 `archived_only = false` 下也会直接重建未归档的完整线程行，而不是把 archived 语义留成不确定状态
- `thread/metadata/update` 的 repair 入口现在也已继续对齐同一条摘要完整度约束：如果 SQLite row 已存在、但 `first_user_message` 这类 stored summary 字段仍缺失，服务端不会再直接拿残缺 row 组 update 响应，而会先 reconcile rollout，再返回更新后的线程摘要，避免 metadata patch 响应继续比 `thread/read` / `thread/list` 少一层 preview
- `thread/unarchive` 现在也已补上同样的对称修复：服务端不会再只调用 `mark_unarchived()` 去移动路径与 archived 标记，而会先对恢复后的 rollout 做一次 reconcile，再读取持久化 metadata；这样 unarchive 响应、后续 `thread/list`，以及 SQLite 里的 stored summary 会一起恢复，而不是只让响应面从 rollout 临时看见 preview、DB-backed list 仍保留空摘要
- `thread/resume` 的 persisted-metadata 入口现在也已继续对齐：如果 resume 前 SQLite row 已存在、但 rollout-derived summary 字段仍缺失，服务端不会再直接拿这条残缺 row 做 overrides merge 并继续返回响应，而会先 reconcile rollout，再读取修补后的 persisted metadata；这样 resume 响应与 resume 后的 SQLite row 都会一起恢复 preview，不再只让响应面看起来正确
- `thread/rollback` 现在也已补到同一条 stored-summary repair 边界：如果 rollback 前 SQLite row 已存在、但 rollout-derived preview 仍缺失，响应面不会再只保 resident mode 而继续返回空摘要；rollback 返回值、后续 `thread/read` / `thread/list`，以及 SQLite row 都会一起恢复同一份 preview
- `app-server` 的 loaded-thread 读取面现在也已开始共用同一条 helper：`thread/list`、`thread/loaded/read` 和 `thread/read` 的 loaded fallback 不再各自重复一份 rollout-summary 拼装逻辑，而是共同走 resident-aware 的 loaded summary helper；对应 provider-override 回归也已分别锁住这三条路径不会在 rollout metadata 缺失时退回默认 provider
- `app-server` 的 persisted-summary repair 入口现在也已继续朝同一条 state-db 选择边界收口：`thread/read` 与 metadata repair 入口会优先复用 loaded thread 自己持有的 state-db，再回退到全局 state-db，避免同一类 stored-summary repair 逻辑继续各自复制一份 SQLite 句柄选择分叉
- `app-server-client` 的 typed 读取面现在也已把这条缺摘要 repair 路径补成对称回归：当 SQLite row 仍在、但 rollout-derived summary 字段被清空后，typed `thread/read` 与 `thread/list` 都会继续命中同一条 reconcile 路径，恢复 resident thread 的 preview，而不是只在服务端集成测试里证明这层 stored-summary 修复
- `app-server-client` 的 typed repair 现在也已把 `thread/resume` 补齐到同一层：如果 SQLite row 已存在、但 rollout-derived preview 缺失，in-process `thread/resume` 也会继续直接返回修补后的 resident summary，而不是把 reconnect 之后的补读责任重新推回调用方
- `app-server-client` 的 typed rollback 现在也已补到同一层：如果 resident thread 在 rollback 前已存在残缺 SQLite row，typed `thread/rollback` 也会继续直接返回修补后的 summary，而不是只在 follow-up `thread/read` / `thread/list` 上才补回 preview
- `app-server-client` 的 remote facade 现在也已补上这条契约的读取面对称回归：当 websocket 远端直接返回 resident repaired `thread/resume`、`thread/read`、`thread/list` 或 `thread/loaded/read` 摘要时，typed remote client 都会继续原样保留 `thread.mode + preview + status + path + name`，避免“直接信服务端返回的 Thread”只在 in-process 路径上成立；同一层 remote typed 回归现在也已把 `thread/loaded/list` 锁成 id-only probe，确保远端 facade 不会把 loaded ids 漂成完整摘要读取面
- `app-server/README.md` 与 `app-server-client/README.md` 现在也已继续收口到同一口径：`thread/list`、`thread/resume` 等返回面已被明确写成权威 repaired summary，remote facade 也被明确要求原样保留服务端返回的 repaired `Thread`，而不是重新引入“resume 后再补一次 `thread/read`”的旧心智
- `debug-client` 的 unknown-thread 自愈链路现在也已继续补到本地缓存层：reader 不再只验证“会补发一次 `thread/loaded/read`”，而会在 loaded summary 返回后确认 resident `mode + status + name` 已重新写回 `known_threads`，让这条“status-only 增量之后回到读取面恢复权威摘要”的消费契约真正形成闭环

按当前本地改动集看，`rollout/state_db` 这层最关键的 helper 分叉现在已经基本被直接锁住：

- `read_repair_rollout_path()` 在 existing-row / missing-row 两类入口上都已覆盖
- archived / unarchived 两类语义也都已覆盖
- “缺 summary 时不能停在半修复状态” 这条约束已经不再只靠 `app-server` 间接证明

因此，这份 checklist 当前更合理的默认下一步已经不是继续给 `read_repair_rollout_path()` 叠更多同类个案，而是：

- 把后续注意力转到 `app-server` 读取面 helper 是否也已经完全共用同一条 repaired-summary 边界
- 开始整理这一包 SQLite 收敛改动的提交边界，而不是继续扩写同层总文档

当前更适合作为下一步继续核对的点是：

- 哪些 `app-server` 恢复路径已经真正以 SQLite 稳定元数据为主干来源，哪些仍在 fallback 到 rollout 或 runtime overlay
- typed client 与 app-server README 是否已经把这些“直接信返回的 `thread.mode` 与 repaired summary，不要再二次脑补”语义完全写实

当前已明确通过、可直接支撑这条判断的 typed client 验证包括：

- `cargo test -p codex-app-server-client --lib archived_metadata_update_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib archived_read_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib unarchive_preserves_resident_thread_mode_through_typed_requests`
- `cargo test -p codex-app-server-client --lib resident_unsubscribe_preserves_mode_on_followup_reads_and_resume`
- `cargo test -p codex-app-server-client --lib rollback_preserves_resident_thread_mode_through_typed_response_and_follow_up_reads`
- `cargo test -p codex-app-server-client rollback_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client read_and_list_reconcile_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib metadata_update_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib unarchive_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib resume_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_resume_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_list_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_list_preserves_id_only_probe`
- `cargo test -p codex-app-server-client --lib remote_typed_thread_loaded_read_preserves_repaired_thread_summary`
- `cargo test -p codex-app-server-client --lib rollback_reconciles_missing_summary_for_existing_sqlite_row_through_typed_requests -- --nocapture`

当前已明确通过、可直接支撑这条判断的服务端验证包括：

- `cargo test -p codex-app-server --test all resident_thread_unarchive_preserves_resident_mode`
- `cargo test -p codex-app-server --test all thread_metadata_update_repairs_loaded_resident_thread_without_losing_mode`
- `cargo test -p codex-app-server --test all thread_metadata_update_repairs_missing_sqlite_row_for_archived_thread`
- `cargo test -p codex-app-server --test all stored_rollout_thread_read_and_list_preserve_rollout_summary_and_sqlite_mode`
- `cargo test -p codex-app-server --test all thread_metadata_update_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_unarchive_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all thread_resume_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-app-server --test all resident_thread_rollback_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-rollout list_threads_db_enabled_reconciles_missing_rollout_summary_fields`
- `cargo test -p codex-rollout read_repair_rollout_path_reconciles_missing_summary_for_existing_sqlite_row`
- `cargo test -p codex-rollout read_repair_rollout_path_preserves_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_clears_archived_semantics_while_reconciling_missing_summary`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_archived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-rollout read_repair_rollout_path_recreates_unarchived_row_from_rollout_when_sqlite_row_is_missing`
- `cargo test -p codex-state`
- `cargo test -p codex-app-server --test all thread_loaded_read_uses_loaded_thread_model_provider_override_when_rollout_metadata_is_missing`
- `cargo test -p codex-app-server --test all thread_list_uses_loaded_thread_model_provider_override_when_rollout_metadata_is_missing`
- `cargo test -p codex-app-server --test all get_conversation_summary_by_thread_id_repairs_missing_summary_for_existing_sqlite_row`

### 当前阶段判断

按第 7 节的完成定义看，这一包已经明显接近阶段完成，但相比 `remote-bridge` 和 `observer`，仍更值得保留为当前主线上最后一份需要继续收口的 checklist。

当前还不直接把它写成“已完成”，主要是因为：

- resident continuity 的关键恢复面虽然已经补得很全，但“各读取面先信谁”的优先级说明还更像散落在测试和设计稿里的事实，而不是收成一段更明确的阶段结论
- SQLite / rollout / runtime overlay 的职责分工虽然已经越来越清楚，但仍值得再补一次面向实现者的简洁总结，避免后续改动重新把 fallback 逻辑写散
- `codex-state` 这层虽然现在也已通过整 crate 本地验证，但这更像是在说明底层稳定元数据地板已基本站稳；当前真正还值得继续收口的仍是 `app-server` / typed client 这一层对“先信谁”的表达与提交边界整理
- `app-server` 里虽然已经继续清掉了一部分旧 phase-1 TODO 与 state-db 选择分叉，但这也反过来说明当前主问题更接近“把剩余旧心智清干净”，而不是继续发散到新的状态模型或存储面

因此这份清单当前更适合作为：

- 下一轮真正继续推进的默认入口
- 进入 `docs/sqlite-state-convergence-file-todo.md` 之前的阶段边界说明

优先目标不再是盲目增加更多 resident continuity 测试，而是把“权威来源优先级”和“哪些恢复路径仍可能 fallback”写成更清楚的阶段结论。

如果已经确定当前就在补这条 SQLite 收敛实现，而不是继续判断阶段边界，更适合直接转到：

- `docs/sqlite-state-convergence-file-todo.md`

如果当前要做的已经不是“继续判断 SQLite 这包还缺哪些主干边界”，而是“把这包整理成单独 PR，或判断它是否已经和 baseline / 文档流程整理混包”，更适合继续看：

- `docs/sqlite-state-convergence-pr-template.md`
- `docs/persistent-runtime-current-worktree-pr-split.md`

### 当前权威来源优先级快照

按当前 `app-server` 实现，更适合面向实现者直接记住的顺序是：

- `thread/read`
  先用可读到的 stored summary 组装线程摘要；优先是 state-db summary，其次才回退到 rollout summary，再不行才回退到 loaded snapshot
- `thread/read`
  在基础摘要组好后，再补 SQLite 持久化的 resident metadata（尤其是 `threads.mode`），最后再附着 runtime metadata / loaded status
- `thread/list`
  先以 stored summaries 为主，再叠加 loaded status；resident live set 命中时优先按 runtime resident 状态标注，否则再回退到 SQLite 持久化 mode
- `thread/list`
  这里的 “stored summary” 不能被简化成“只要 SQLite 里有行就行”；如果 SQLite 线程行还缺 rollout-derived preview/first-user-message 等列表必需字段，read-repair 仍必须继续 reconcile rollout，把 stored summary 补齐后再让 DB-backed listing 成为权威来源
- `thread/resume`
  如果线程当前仍在运行，优先信 live runtime thread；如果需要从历史恢复，则先按 rollout/history 回建，再补持久化 resident metadata，最后附着 runtime metadata
- `thread/resume`
  这条路径对消费侧的要求也已开始固定：无论是 in-process typed client，还是 remote websocket facade，只要服务端已经返回修补后的 resident `Thread` 摘要，调用方都应继续直接信这个返回值，而不是额外补一次 `thread/read` 去确认 preview / mode
- `thread/unarchive` / `thread/metadata/update`
  这类 metadata-first 恢复路径应直接信更新后的持久化 thread metadata 返回值，而不是要求消费侧再补一次 `thread/read` 去恢复 resident 语义
- `thread/metadata/update`
  这里的 “更新后的持久化 thread metadata” 也必须满足 stored summary 完整度要求；如果 SQLite row 已存在但 rollout-derived preview 仍缺失，update 前仍应先做 rollout reconcile，而不是把空 preview 当成可接受的持久化响应
- `thread/unarchive`
  这里也同样不能把“档案状态去掉了”误当成恢复完成；如果恢复后的 SQLite row 仍缺 rollout-derived preview，unarchive 仍应先 reconcile rollout，再让后续 stored list/read 把这条线程当成真正完成收敛的 restored thread
- `thread/resume`
  这里也不能把“resume 响应已经从 rollout 读到 preview”误当成收敛完成；如果 persisted metadata 仍缺 rollout-derived summary，resume 前仍应先 reconcile rollout，让后续 SQLite row、overrides merge 和响应面继续保持同一套 stored summary

换句话说，当前更接近真实实现的心智不是“SQLite、rollout、runtime 三选一”，而是：

- stored summary 提供基础线程骨架
- SQLite metadata 提供稳定 resident identity
- runtime metadata 提供当前 loaded / observer 状态

这里还需要额外记住一个更具体的实现约束：

- “stored summary” 本身也有完整度要求
- 如果 SQLite 里只有 thread identity / mode，但缺少列表读取面依赖的 rollout-derived 摘要字段，这条线程还不能算真正完成收敛
- 这类情况下更合理的动作不是让 `thread/list` 对残缺 SQLite 行做特判兜底，而是继续通过 rollout reconcile 把 SQLite 摘要补完整

后续如果还要继续收口，这里最值得补的不是更多 resident continuity 个案，而是：

- 把这套优先级再压成更短的 review checklist，用来判断新恢复路径有没有重新写散 fallback 顺序

### 当前最短 review checklist

如果后续有新恢复路径或 repair 路径进来，最适合先问的其实只剩下面几句：

1. 这个路径的基础线程摘要先来自哪里：state-db summary、rollout summary，还是 loaded snapshot
2. resident identity 是否仍由 SQLite 持久化 metadata 兜底，而不是临时从 rollout 或通知里脑补
3. loaded / observer 状态是否仍只由 runtime metadata 附着，而不是顺手写回 SQLite 当成稳定字段
4. 如果 SQLite 行已存在但 stored summary 仍残缺，这个路径会不会继续 reconcile rollout，而不是停在半修复状态
5. 返回值是否已经足够让消费侧直接信 `thread.mode` 与摘要字段，而不是还要额外补一次 `thread/read`

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
- SQLite 已有 thread 行但 stored summary 仍残缺时，`thread/list` 不会把该线程静默过滤掉、而 `thread/read` 又单独从 rollout 看见它

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
