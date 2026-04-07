# 持久运行时实现计划

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/persistent-assistant-mode-design.md`
- `docs/app-server-thread-mode-v2.md`
- `docs/observer-event-flow-design.md`
- `docs/sqlite-state-convergence.md`

目标是把前面几份设计稿收敛成一个可以直接指导实现和拆 PR 的任务清单。

它回答的问题不是：

- “为什么值得做”

而是：

- “按什么顺序做”
- “每个阶段的输出是什么”
- “哪些内容必须一起落，哪些不应该混在一个 PR”

## 1. 总体原则

这条线的实现应遵循四个原则：

1. 先稳定线程模式，再扩大状态来源
2. 先让读取和展示语义变清楚，再做更多持久化
3. 先收敛 observer 的状态入口，再考虑更复杂消费方
4. SQLite 作为收敛层后置推进，不抢在产品模式前面

如果违反这个顺序，最容易出现的问题是：

- 协议、状态、数据库互相牵制
- 做了很多基础设施，但用户感知提升很弱
- 早期数据库模型反向绑死后续产品迭代

## 2. 推荐阶段划分

### 当前进度（2026-04-07）

这条线已经不再是纯设计状态，阶段 1 到阶段 4 都已有第一批代码落地：

- `app-server-protocol` 已新增 `Thread.mode`
- `app-server` 已在主要 `Thread` 返回路径上填充 `mode`
- schema / TypeScript 生成物已同步更新
- `app-server/README.md` 已补充 `Thread.mode` 与 `thread.status` 的语义区分
- `tui` 已开始消费 `Thread.mode`，至少在 resume picker 中区分长期线程与普通线程
- `thread_status.rs` 已开始收敛 resident thread 的 watcher 生命周期，shutdown 会主动清理工作区 watch
- `workspaceChanged` 已限制为只作用于已加载线程，避免 shutdown 之后被陈旧 watcher 事件重新激活状态
- `state` 已新增 `threads.mode` 稳定元数据列，`thread/read` / `thread/list` / `thread/resume` 可在服务重启后回补 resident 模式

这意味着接下来的工作重点不再是“从零开始补字段”，而是把已经形成闭环的协议、observer 和 SQLite 最小改动收敛成可 review 的 PR，并继续检查消费侧是否还存在遗漏。

### 阶段 1：线程模式字段落地

核心目标：

- 在 `app-server v2` 中正式表达线程模式

对应文档：

- `docs/app-server-thread-mode-v2.md`

建议输出：

- `Thread.mode`
- 服务端统一在主要 `Thread` 返回面填充该字段
- README 和客户端兼容说明

这一阶段先不要求：

- 请求侧改成显式 `mode`
- observer 变化
- SQLite 新表

### 阶段 2：客户端消费线程模式

核心目标：

- 让 TUI 或其他 app-server 客户端真正按线程模式做展示和恢复语义区分

建议输出：

- 列表中区分普通线程与长期线程
- `thread/resume` 在长期线程上的产品语义更新
- 基于 `mode` 的回退兼容逻辑

这一阶段先不要求：

- 新状态枚举
- 新 watcher 架构
- 后台任务元数据入库

### 阶段 3：observer 事件流收敛

核心目标：

- 把现有 `FileWatcher` / `ThreadWatchManager` 路径上升成明确的 observer 事件流

对应文档：

- `docs/observer-event-flow-design.md`

建议输出：

- 清楚的线程观察事件概念
- `workspaceChanged` 触发、保留、清理语义稳定
- resident thread 生命周期与 watcher 生命周期对齐

这一阶段先不要求：

- 新对外 `observer/*` API
- 原始事件入 SQLite
- 全量索引

### 阶段 4：SQLite 收敛稳定元数据

核心目标：

- 在模式和 observer 语义稳定后，把真正值得长期保存的状态继续收敛到 SQLite

对应文档：

- `docs/sqlite-state-convergence.md`

建议输出：

- 线程模式相关稳定元数据入 SQLite
- observer 派生摘要的最小持久化
- 后台任务元数据的最小持久化起点

这一阶段先不要求：

- 全量运行态镜像
- 原始 watcher 事件日志化
- 完整 bridge 消费链

### 阶段 5：远程消费与更高层模式

核心目标：

- 让远程 bridge、多 agent、长时规划等更高层能力消费前面已经稳定的线程模式、状态面和 SQLite 摘要

这一阶段应该晚于前四个阶段。

## 3. 推荐 PR 拆分

下面这个拆分更符合当前仓库实际，也更容易 review。

### PR 1：协议字段

范围：

- `app-server-protocol`
- `app-server/README.md`
- 必要的 schema / TS 生成物

目标：

- 增加 `Thread.mode`
- 保持 `resident` 兼容

当前状态：

- 已开始实现，包含协议字段、README 和 schema / TS 生成物更新
- 剩余重点是确认返回面覆盖是否完整，以及 review 兼容性细节

不要混入：

- TUI 展示改动
- observer 改动
- SQLite 模式变更

### PR 2：服务端返回面和行为对齐

范围：

- `app-server`

目标：

- 在 `thread/start`、`thread/resume`、`thread/fork`、`thread/read`、`thread/list`、`thread/loaded/read` 等路径上统一返回 `mode`

当前状态：

- 已在当前本地改动中收敛主要返回面
- `thread/start`、`thread/resume`、`thread/fork`、`thread/read`、`thread/list`、`thread/loaded/read` 已补齐 resident / mode 对齐路径
- 目前剩余重点已从“补字段”转为“按 PR 边界整理实现与测试”

不要混入：

- 客户端产品展示
- 新持久化字段

### PR 3：客户端消费 `Thread.mode`

范围：

- `tui`
- 其他需要消费 app-server v2 线程摘要的客户端

目标：

- 列表与恢复体验按线程模式区分

当前状态：

- 已开始第一步消费，resume picker 会对长期线程显示独立模式标记
- `Thread.mode` 已透传到 TUI 会话状态，并在 `/status` 中显示线程模式
- `codex-tui` 也已补上会话映射层回归：`thread/start`、`thread/resume`、`thread/fork` 返回的 resident `Thread.mode` 都会稳定落进 `ThreadSessionState`
- `ThreadStartedNotification` 的通知路径也已补上 resident 模式回归，确保后台线程事件推断出的 session 不会把 resident assistant 降回 interactive
- TUI 启动前的 session lookup 路径也已补上 resident 模式回归：按名称恢复历史线程时，`SessionTarget.mode` 会稳定保留 `ResidentAssistant`
- latest-session / `--resume --last` 路径也已补上 resident 模式回归，确保最近会话选择不会把 resident assistant 降回普通 resume 目标
- 按线程 ID 的 lookup / `thread/read` 路径也已补上 resident 模式回归，确保直接按会话 ID 恢复时同样稳定保留 `ResidentAssistant`
- resident assistant 在线程恢复后会追加“重新连接”提示，不再只表现成普通历史恢复
- TUI 已开始补齐更多恢复入口的语义一致性，例如新线程切换、fork 后摘要提示以及线程改名确认提示，都已按 `Thread.mode` 区分“continue”与“reconnect”
- `/resume` 的命令入口说明也已开始从“历史恢复”语义扩成同时覆盖 resident assistant 的 reconnect 语义
- resume picker 的固定入口标题也已开始从纯 “Resume” 语义扩成同时覆盖 reconnect
- agent thread 在 live attach 失败后退回 transcript replay 时，resident thread 也已改成使用 reconnect 语义，不再统一提示成 resumed live
- resume 路径在 attach 失败时的错误提示，也已开始按线程模式区分 “after resume” 与 “after reconnect”
- resume 目标切 cwd 后如果配置重建失败，错误提示也已开始按线程模式区分 “for resume” 与 “for reconnect”
- session 级恢复失败提示也已继续收口，resident thread 不再只显示通用 resume/session 文案，而会明确提示 reconnect resident assistant
- CLI 退出后的 resume hint 也已开始消费 `Thread.mode`，resident assistant 不再统一显示成 “continue this session”
- `exec` 的启动配置摘要也已开始消费 `Thread.mode`，resident assistant 在 bootstrap 阶段不再被展示成普通 interactive session
- `exec --json` 的 `thread.started` 事件也已开始透出 bootstrap `thread_mode`，方便脚本和其他 JSON 消费方在首事件就区分 reconnect 与普通 resume
- `debug-client` 也已开始消费 `Thread.mode`，连接提示、线程列表和 `:resume` 帮助文案都已按 resident assistant 区分 reconnect 语义
- `app-server-test-client` 的 thread 响应/通知输出也已补上 resident-aware 摘要，方便手工联调时直接识别 reconnect 场景
- `app-server-client` README、MCP 接口文档和 typed request 回归也已开始把 `Thread.mode` 固定为 bootstrap 阶段区分 reconnect 的主信号，减少外围集成对旧 resume 语义的误读
- `app-server/README.md` 也已同步把 `Thread.mode` 固定为 resident reconnect 的主语义，不再只把 `resident: true` 当作实现细节提及
- `app-server/README.md` 的 lifecycle overview 总览入口也已补上 reconnect 口径，首屏文案不再落后于细节章节
- 仍有剩余消费侧入口需要继续检查，但主路径上的恢复/重连文案已经开始收口

不要混入：

- watcher 逻辑改造
- SQLite 收敛

### PR 4：observer 语义收敛

范围：

- `app-server/src/thread_status.rs`
- 如有必要，少量 `core/src/file_watcher.rs`

目标：

- 明确 observer 事件到 `workspaceChanged` 的映射
- 梳理 resident thread 的观察注册和清理语义

当前状态：

- `thread_status.rs` 已开始收敛 shutdown 与 observer 的边界
- resident thread 在 shutdown 时会主动清理工作区 watch，避免 watcher 生命周期长于线程生命周期
- `workspaceChanged` 已限制为只更新已加载线程，避免 shutdown 之后被陈旧 watcher 事件重新激活线程状态
- resident thread 在最后一个订阅者断开后的 reconnect / `thread/resume` 会保留既有 `workspaceChanged` 与 resident 模式，不再回退成普通 interactive 线程
- resident thread 在最后一个订阅者断开后也已补上负向与读取面回归：保持 loaded 的同时不会错误发出 `thread/closed`；后续 `thread/read` 仍会稳定返回 `mode = residentAssistant`，`thread/loaded/read` 里该线程也会继续保持 `mode = residentAssistant` 且 `status = idle`
- 下一次 turn 完成后会按既有状态机清理 `workspaceChanged`，避免脏标记长期滞留
- resident workspace watch 的迁移/清理边界也已补上单测：同一线程切换 `cwd` 后旧目录变化不会再触发 `workspaceChanged`，切回非 resident 后会移除 watch，避免 observer 状态持续受陈旧工作区干扰

不要混入：

- 新索引系统
- 新数据库表

### PR 5：SQLite 稳定元数据收敛

范围：

- `state`
- `rollout`
- `app-server`
- 必要时 `core`

目标：

- 仅让稳定元数据和派生摘要入库

不要混入：

- 全量运行态持久化
- 远程 bridge

当前状态：

- `state` 已新增 `threads.mode` 稳定元数据列，并在 rollout -> SQLite 汇总时优先保留既有线程模式
- `thread/start`、`thread/resume`、`thread/fork`、`turn/start` 会在 rollout 已物化后补齐或修复模式元数据，而不会为未物化线程预先写入持久化垃圾行
- resident `thread/start` 的边界也已补上回归覆盖：返回 `mode = residentAssistant` 时，在线程真正物化前仍不会提前写入 SQLite
- 服务重启后，未加载线程的 `thread/read` / `thread/list` 以及后续 `thread/resume` 都会消费 SQLite 中持久化的 resident 模式
- 从 resident thread 派生出的 fork 线程也已补上回归覆盖：默认仍保持 `interactive`，并且重启后不会从 SQLite 误恢复成 `residentAssistant`
- `codex-state` 也已补上状态层边界测试：`set_thread_mode` 对缺失线程只返回 false，不会顺手创建 SQLite 垃圾行
- 相关最小行为闭环已可由 `codex-state`、`codex-rollout`、`codex-app-server` 的定向测试覆盖
- 仍需保持边界，只把稳定元数据和派生摘要入库，不把 loaded 状态、watcher、连接关系一类瞬时运行态塞进 SQLite

## 4. 测试建议

这条线最容易出问题的不是编译，而是行为回归和语义漂移。

因此测试应该优先覆盖四类东西。

### 协议测试

- `Thread.mode` 在主要返回面是否一致
- 旧客户端忽略该字段时是否仍能正常工作

### 生命周期测试

- resident 线程在最后一个订阅者断开后是否仍保持 loaded
- 非 resident 线程是否维持现有关闭行为
- 重新连接后线程状态是否连续

### observer 测试

- 工作区变化是否稳定映射为 `workspaceChanged`
- 下一次 turn 是否按预期清理该标志
- unload / shutdown 后 watcher 是否清理

### SQLite 测试

- 只验证稳定元数据和派生摘要
- 不把瞬时运行态也写成持久化断言

## 5. 风险控制

这条实现线有几个明显风险，需要提前约束。

### 风险 1：把 `resident` 和 `mode` 混成一回事

控制方式：

- 文档、协议、客户端都保持 `mode` 和 `resident` 的语义分离

### 风险 2：把 `Active` 错当“线程正在运行”

控制方式：

- 客户端展示和测试里明确 `activeFlags` 的不同语义

### 风险 3：observer 范围膨胀

控制方式：

- 第一阶段只围绕 `workspaceChanged`
- 不顺手引入索引、语义分析、自动 prompt 重建

### 风险 4：SQLite 提前吞掉瞬时状态

控制方式：

- 只持久化稳定元数据与派生摘要
- 不为连接、watcher、流式细节建永久表

## 6. 完成标志

如果按这个计划推进，比较合理的“第一波完成”标志是：

- `Thread.mode` 在协议与服务端返回面稳定存在
- 客户端已开始按线程模式工作
- `workspaceChanged` 的 observer 语义明确且行为稳定
- SQLite 只新增确实需要长期保存的稳定元数据或派生摘要

做到这里，才适合进入下一轮：

- 远程 bridge 消费
- 多 agent 更强编排体验
- 长时规划模式

而不是在基础还不稳时提前并行摊大饼。

如果要继续补消费侧设计文档，优先应增加：

- `docs/remote-bridge-consumption.md`
