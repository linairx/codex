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
- TUI 的随机启动 tooltip 也已同步收口到同一口径：`codex resume` 不再只提示“恢复历史会话”，而会明确写成 `resume or reconnect`
- agent thread 在 live attach 失败后退回 transcript replay 时，resident thread 也已改成使用 reconnect 语义，不再统一提示成 resumed live
- resume 路径在 attach 失败时的错误提示，也已开始按线程模式区分 “after resume” 与 “after reconnect”
- resume 目标切 cwd 后如果配置重建失败，错误提示也已开始按线程模式区分 “for resume” 与 “for reconnect”
- session 级恢复失败提示也已继续收口，resident thread 不再只显示通用 resume/session 文案，而会明确提示 reconnect resident assistant
- CLI 退出后的 resume hint 也已开始消费 `Thread.mode`，resident assistant 不再统一显示成 “continue this session”
- `codex resume` 与 `codex exec resume` 的帮助摘要也已继续收口到同一口径：入口说明明确写成 `resume or reconnect`，避免外围调用者继续把 resident thread 理解成纯历史恢复
- `app-server/README.md` 的 Events 总览和 `docs/codex_mcp_interface.md` 的 thread 概览也已继续对齐到首屏语义：外围集成现在可以直接从这些入口读到 `resume/reconnect` 与 `thread.mode` 的关系，而不用先翻到深处章节再自行拼装 resident reconnect 心智
- `app-server/README.md` 的 `thread/resume` 方法摘要和 `app-server-client` README 的 bootstrap 说明也已继续精确化：文案现在明确写成“ordinary interactive resume target” 对比 resident reconnect，避免 typed/in-process 集成继续把 resident thread 混读成泛化的普通 resume
- `app-server/README.md` 顶部 lifecycle overview 的首屏总结也已同步改成同一口径：外围集成在最先读到的 overview 里就能看到 ordinary interactive resume target 与 resident reconnect 的区分，不需要再靠后文兜底
- `docs/codex_mcp_interface.md` 的 thread 概览和 `app-server/README.md` 的 `thread/list` 摘要也已继续精确化：MCP 入口现在直接写明 `thread/resume` 覆盖 resume/reconnect，而列表消费面也明确把 resident assistant 对照项写成 ordinary interactive resume target，避免历史 UI/selector 继续按泛化 interactive session 心智解读
- `app-server-client` README 的 bootstrap 说明和 `app-server/README.md` 的历史列表消费段也已继续收紧术语：前者改成显式 resident reconnect，后者则把 resident assistant 的对照项固定成 ordinary interactive resume targets，减少 in-process 调用侧和列表 UI 的语义漂移
- `app-server-test-client` README 的摘要说明也已继续对齐实现现状：文档现在明确写出 compact summary 同时包含 wire `thread.mode` 和派生的 `resume/reconnect` action label，并把它们解释成 ordinary interactive resume target 对照 resident reconnect target，减少手工联调时的术语猜测
- `app-server-test-client` 的 CLI help 也已同步收口：`resume-message-v2` / `thread-resume` 这两个最常用恢复命令的子命令说明和参数 help 现在都明确写成 `resume or reconnect`，并补上 help 回归测试，避免实现 README 已经更新但 `--help` 输出继续滞后
- `docs/remote-bridge-consumption.md` 也已继续精确化远端首页语义：文档现在明确要求把 `Thread.mode` 直接映射成列表动作，`interactive -> resume`、`residentAssistant -> reconnect`，避免远端控制面只消费 mode 却不把产品动作语义定死
- `docs/persistent-assistant-mode-design.md` 与 `docs/app-server-thread-mode-v2.md` 也已继续补齐这条设计侧边界：`thread/resume` 虽然保留统一 API，但客户端动作文案必须按 `Thread.mode` 稳定映射成 `interactive -> resume`、`residentAssistant -> reconnect`
- `docs/observer-event-flow-design.md` 与 `docs/sqlite-state-convergence.md` 也已继续补齐同一消费边界：observer/status-only 通知和 SQLite 元数据负责状态事实，但产品动作文案仍必须从读取面提供的 `Thread.mode` 稳定映射成 `interactive -> resume`、`residentAssistant -> reconnect`
- `exec` 的启动配置摘要也已开始消费 `Thread.mode`，resident assistant 在 bootstrap 阶段不再被展示成普通 interactive session
- `exec` 的人类可读 bootstrap 摘要也已进一步补齐 `session action`：除了 `session mode` 外，现在会直接显示 `interactive -> resume`、`resident assistant -> reconnect`，并补上两侧回归测试
- `exec` 的这组人类可读 bootstrap 摘要回归也已补上负向覆盖：当 `thread_mode` 未知时，不会错误打印 `session mode` / `session action`
- `exec resume` 的人类可读 stderr 摘要也已补上端到端回归：真实命令输出会稳定包含 `session mode: interactive` 与 `session action: resume`
- `exec resume <thread-id>` 的人类可读 stderr 摘要也已补上对应端到端回归：按显式 thread id 恢复普通 interactive 会话时，真实命令输出同样会稳定包含 `session mode: interactive` 与 `session action: resume`
- 普通 `codex exec` 的人类可读 stderr 摘要也已补上对应的端到端回归：fresh start 场景下，真实命令输出同样会稳定包含 `session mode: interactive` 与 `session action: resume`
- 普通 `codex exec --json` 的端到端回归也已把这层输出边界写死：fresh start 场景下，`thread_mode = interactive` 只通过首个 `thread.started` JSON 事件暴露，不会再额外把 `session mode/session action` 打到 stderr
- `exec --json` 的 `thread.started` 事件也已开始透出 bootstrap `thread_mode`，方便脚本和其他 JSON 消费方在首事件就区分 reconnect 与普通 resume
- `exec --json` 的 bootstrap `thread_mode` 也已补上序列化回归测试：resident assistant 会稳定输出 `thread_mode = residentAssistant`，而未知模式时继续省略该字段，避免 JSON 契约无意漂移
- `exec --json` 的 `thread.started` wire 形状也已补到集成测试层：除了内部序列化单测外，公开 JSON 输出回归现在也会直接断言 `type/thread_id/thread_mode` 的实际字段形状
- `exec --json` 的这层集成回归也已补上负向覆盖：当 bootstrap 里拿不到 `thread_mode` 时，公开 JSON 输出会继续省略该字段
- `exec resume --json` 的真实命令输出也已补上端到端回归：恢复普通 interactive 会话时，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = interactive`
- `exec resume --json` 的这条端到端回归也已把 stderr 边界写死：模式语义只通过首个 `thread.started` JSON 事件暴露，不会再额外把 `session mode/session action` 打到 stderr
- 普通 `codex exec --json` 的真实命令输出也已补上对应的端到端回归：fresh start 场景下，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = interactive`
- `exec resume --json` 的 resident 路径也已补到真实命令输出层：当线程模式先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，stdout 首个 `thread.started` 事件会稳定带出 `thread_mode = residentAssistant`
- `exec resume --last --json` 的 resident 路径也已补上同层端到端回归：当最近会话先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，stdout 首个 `thread.started` 事件同样会稳定带出 `thread_mode = residentAssistant`
- `exec resume --last` 的 resident 路径也已补上人类可读端到端回归：当最近会话先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，真实 stderr 摘要同样会稳定包含 `session mode: resident assistant` 与 `session action: reconnect`
- `exec resume <thread-id>` 的 resident 路径也已补上人类可读端到端回归：当线程模式先通过 SQLite 稳定元数据恢复成 `residentAssistant` 后，真实 stderr 摘要会稳定包含 `session mode: resident assistant` 与 `session action: reconnect`
- `exec` 这两条真实 `--json` 首事件回归现在也已收敛到同一个 stdout 首行解析 helper，减少端到端测试各自手写 JSONL 首事件提取逻辑的漂移
- `exec` 的 `thread.started` 事件注释也已同步收口：公开类型说明现在明确这是 bootstrap 后的首事件，覆盖 fresh start 与 resume/reconnect，而不再只写成“新线程 started”
- `exec` 的公开 JSON event 注释也已同步收口：`ThreadStartedEvent.thread_id` 现在明确写成后续可按 `thread_mode` 用于 `resume or reconnect`，不再把这条 API 注释停留在旧的纯 resume 心智
- `exec` 的 `ThreadStartedEvent.thread_mode` 字段注释也已同步补齐：公开类型说明现在直接把它定义成下游区分 ordinary interactive resume target 与 resident reconnect target 的主信号
- `codex-rs/README.md` 的顶层 `codex exec` 说明也已同步补上脚本消费边界：README 现在明确写出 `codex exec --json` 的首个 `thread.started` 事件会带 bootstrap `thread_id`，fresh start 时输出 `interactive`、resident reconnect 时输出 `residentAssistant`，未知模式时则省略该字段；并明确这层元数据只走 JSON 事件面
- `codex-rs/README.md` 的顶层 `codex exec` 说明现在也已把默认 human-readable 输出补齐：README 会直接写出非 `--json` 模式下的 bootstrap stderr 摘要包含 `session mode` / `session action`
- `debug-client` 也已开始消费 `Thread.mode`，连接提示、线程列表和 `:resume` 帮助文案都已按 resident assistant 区分 reconnect 语义
- `debug-client` 的 `:refresh-thread` 摘要也已开始同时展示线程模式与推荐动作，resident thread 不再只显示成模糊的 reconnect/resume 动词，而能直接看出这是 `resident assistant`
- `debug-client` 的 `:use <thread-id>` 提示也已继续按已知线程模式收口：resident thread 不再显示成通用 thread 切换提示，而会明确提示切到 `resident assistant thread`
- `debug-client` 的 resident 消费面也已重新补回完整工作态：被截断的事件处理和帮助输出已经恢复，并补上 reconnect 文案回归，避免这块最小客户端入口停留在半成品状态
- `debug-client` 的内置 `:help` 也已同步收口到 resident-aware 语义，避免帮助面继续把 `:use` / `:refresh-thread` 描述成泛化 thread 操作
- `debug-client` 的 clap 顶层 `--help` 也已继续对齐到同一口径：除了交互内置 `:help` 外，`--thread-id` 和相关 override 的命令行帮助说明现在同样稳定写成 `resume or reconnect`，并补上顶层 help 回归测试
- `app-server-test-client` 的 thread 响应/通知输出也已补上 resident-aware 摘要，方便手工联调时直接识别 reconnect 场景；`thread-read` 与 `thread-metadata-update` 的联调命令也已补齐，同样会把 resident `mode` 打进摘要输出
- `app-server-test-client` 的其余主要 thread 命令也已继续补齐 resident-aware 摘要：`thread-fork`、`thread-loaded-read` 和 `thread-unarchive` 现在同样会把 `mode` 与 `resume/reconnect` 语义直接打到联调输出里
- `app-server-test-client` 的 README 也已同步把这批边缘恢复命令写成显式 `mode` + `resume/reconnect` 摘要输出，不再只停留在模糊的 resident-aware 描述
- `app-server-test-client` 的 clap 顶层 `--help` 也已同步收口：`resume-message-v2` 和 `thread-resume` 这两个最常用恢复命令的子命令说明现在同样稳定写成 `resume or reconnect`，并补上 help 回归测试
- `app-server-test-client` 的 compact summary 输出也已补上整串回归断言：`thread/read` 与列表摘要现在都会稳定覆盖 `mode + resident + status + action` 的完整文本，不再只靠单独的 mode/action 映射测试侧面兜底
- `app-server-test-client` README 里的这段说明也已进一步对齐到真实输出：联调摘要打印的是 wire `thread.mode` 值（如 `interactive` / `residentAssistant`）再附带 `resume/reconnect` 动作
- `app-server-client` README、MCP 接口文档和 typed request 回归也已开始把 `Thread.mode` 固定为 bootstrap 阶段区分 reconnect 的主信号，减少外围集成对旧 resume 语义的误读；除了 metadata-only update 的 resident `mode` 保留覆盖外，`thread-loaded-read` 的 typed request 边界也已补齐 resident 模式回归
- `app-server/README.md` 也已同步把 `Thread.mode` 固定为 resident reconnect 的主语义，不再只把 `resident: true` 当作实现细节提及
- `app-server/README.md` 的 lifecycle overview 总览入口也已补上 reconnect 口径，首屏文案不再落后于细节章节
- `app-server/README.md` 的 `thread/list`、`thread/loaded/read` 和 `thread/unarchive` 示例结果也已补成显式带 `mode` 的版本，避免示例继续落后于正文契约
- `app-server/README.md` 的 `thread/start`、`thread/resume` 和 `thread/fork` 顶层示例现在也已显式带上 `mode`，最前面的线程生命周期示例不再把 resident / interactive 语义压在省略号后面
- `app-server/README.md` 的 lifecycle overview 现在也已把 `mode` / `status` 的职责分工直接写进首屏总览，不再要求读者下钻到详细章节才知道 reconnect 语义来自 `mode`
- `app-server/README.md` 的 `thread/started` 通知示例现在也已显式带上 `mode`，response / notification 两个消费面都不再靠读者自行脑补 resident 语义
- `app-server/README.md` 的 Events 总览现在也已明确：生命周期通知里只要附带 `thread` snapshot，外围消费者就应直接信任其中的 `thread.mode`
- `app-server/README.md` 的 detached review 文案也已同步要求消费 `thread/started` snapshot 里的 `mode`，避免 review fork 这类边缘通知路径重新退回通用 resumed session 心智
- `app-server/README.md` 的详细 `thread/status/changed` 说明现在也已明确这只是 status-only 通知；需要 reconnect 语义时，客户端必须保留先前恢复面返回的 `thread.mode`
- `app-server/README.md` 的 API Overview 顶层摘要现在也已同步写清：`thread/status/changed` 不会重复 `mode`，而 `Thread.mode` 与 `thread.status` 的职责边界也已被直接写成契约
- `app-server/README.md` 的 `thread/unsubscribe` 说明现在也已明确 resident 连续性：最后一个订阅者断开后，resident thread 的后续 `thread/loaded/read`、`thread/read` 与 `thread/resume` 仍会保留既有 `mode` 与 loaded/runtime 状态
- `app-server/README.md` 的顶层方法摘要也已同步到同一口径：`thread/unsubscribe` 与 `thread/rollback` 这两条返回/影响 `Thread` 的路径都不再把 resident `mode` 语义留给读者自行推断
- `app-server/README.md` 的顶层 `thread/unarchive` 摘要现在也已显式写出“保留既有 `mode`”，避免这类恢复路径只在详细示例里才体现 resident 语义
- `app-server/README.md` 的顶层 `thread/loaded/read` 摘要也已明确写出会返回当前 `mode`，loaded 恢复面不再只靠下方详细章节体现 resident 语义
- `app-server/README.md` 的顶层 `thread/fork` 摘要现在也已直接写出“默认仍保持 interactive，除非 fork 自己显式进入 resident 模式”，避免顶层说明重新引入来源线程模式继承的误读
- `app-server/README.md` 的 archive 段落也已同步写清：archived thread 虽然默认不出现在普通 `thread/list`，但在 `thread/list archived=true` 与 `thread/read` 返回时仍会保留既有 `mode`
- `app-server/README.md` 的详细 `thread/list` 说明现在也已把 archived 列表的模式消费约束写成正文，不再只靠 archive 段落和示例去侧面表达
- `app-server/README.md` 的详细 `thread/loaded/list` 说明现在也已明确这只是 id-only probe；需要 reconnect 语义时不应停在 loaded ids，而应继续读 `thread/loaded/read`
- `app-server/README.md` 的详细 `thread/loaded/read` 说明现在也已明确：loaded polling 本身就是带 `thread.mode` 的恢复面，而不只是无模式状态探针
- `app-server/README.md` 的详细 `thread/fork` 说明现在也已把这条模式边界写成正文契约，而不再只靠示例 JSON 的 `interactive` 值隐含表达
- `app-server/README.md` 的详细 `thread/unarchive` 说明现在也已明确：恢复 archived resident thread 时，应直接信任返回 `thread.mode`，不需要额外补一次 `thread/read`
- `docs/app-server-thread-mode-v2.md` 也已同步补上这两条设计侧边界：`thread/loaded/list` 不是模式恢复面，`thread/status/changed` 则继续是 status-only 通知
- `docs/persistent-assistant-mode-design.md` 也已同步补了这层承接说明，不再让更早的前导草案停留在“接口会返回 mode”但没有说明这些边界已由哪份当前文档接手
- `docs/observer-event-flow-design.md` 也已同步把这层 observer / status 边界写明：`workspaceChanged` 等 observer 事实可以通过线程状态面暴露，但 `thread/status/changed` 仍只负责 status 增量，不承担重复 `mode`
- `docs/remote-bridge-consumption.md` 也已同步补上远端消费侧的同一边界：`thread/loaded/read` 负责 loaded 线程的 `mode + status` 摘要，而 `thread/status/changed` 只做后续 status 增量
- `docs/sqlite-state-convergence.md` 也已同步承认这层协议边界：SQLite 继续收敛稳定线程摘要与元数据，但不把 `thread/status/changed` 这类 status-only 增量误当成完整恢复来源
- 当前这条 README / 设计文档链已经基本收口；下一步更适合把这批改动整理成提交，或切回新的代码闭环，而不是继续扩写同层文档

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
- resident thread 的 unarchive 路径也已补上回归：`thread/unarchive` 响应不会再因为只读 rollout 摘要而丢掉稳定 `mode`，后续 `thread/read` / `thread/list` 也会继续保持 `ResidentAssistant`
- resident thread 的 archive 读取面也已补上回归：进入 archived 状态后，`thread/read` 和 `thread/list archived=true` 仍会继续保留 `ResidentAssistant`
- `thread/metadata/update` 的 resident 覆盖也已继续扩到 stored + archived 面：纯 SQLite 稳定元数据路径不会把未加载 resident thread 的更新响应与后续读取面降成 interactive；缺失 SQLite 行修复也不会把 loaded resident thread 的稳定 `mode` 重建成普通 interactive，而 archived resident thread 更新 metadata 后的响应与后续读取面同样会继续保持 `ResidentAssistant`
- `codex-state` 也已继续补上状态层边界测试：`set_thread_mode` 对缺失线程只返回 false，不会顺手创建 SQLite 垃圾行；`update_thread_git_info` 在 resident thread 上也不会顺手覆盖 `threads.mode`
- `app-server/README.md` 的 `thread/metadata/update` 说明也已同步收口：返回的 `thread` 被明确要求保留既有 `mode`，外围客户端不需要再把 metadata-only update 额外视作一次需要 `thread/read` 补 mode 的特殊恢复路径
- `docs/sqlite-state-convergence.md` 也已同步刷新成“已落地边界 + 后续阶段”的状态文档，开始明确记录 `threads.mode` 和 resident metadata repair 这批已经进入 SQLite 主干路径的最小闭环
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
