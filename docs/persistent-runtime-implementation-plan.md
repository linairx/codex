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

这条线已经不再是纯设计状态，阶段 1 的第一批代码已开始落地：

- `app-server-protocol` 已新增 `Thread.mode`
- `app-server` 已在主要 `Thread` 返回路径上填充 `mode`
- schema / TypeScript 生成物已同步更新
- `app-server/README.md` 已补充 `Thread.mode` 与 `thread.status` 的语义区分
- `tui` 已开始消费 `Thread.mode`，至少在 resume picker 中区分长期线程与普通线程

这意味着接下来的“阶段 1”不再是从零开始，而是要把第一批协议与返回面改动收敛成可提交的 PR，并继续检查消费侧是否还存在遗漏。

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

- 已在当前本地改动中开始收敛
- 需要继续以测试和 diff 为准，确认不存在漏填 `mode` 的 `Thread` 构造路径

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
- resident assistant 在线程恢复后会追加“重新连接”提示，不再只表现成普通历史恢复
- 仍未完成更广义的恢复语义整理，例如更多入口/提示文案与长期线程语义的统一

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
