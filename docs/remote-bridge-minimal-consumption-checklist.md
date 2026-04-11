# Remote Bridge Minimal Consumption Checklist

本文承接：

- `docs/codex-rs-source-analysis.md`
- `docs/remote-bridge-consumption.md`
- `docs/resident-mode-baseline-pr-checklist.md`

目标不是设计完整 bridge 产品，而是给“最小线程消费闭环”提供一份可直接执行的清单。

## 1. 这一阶段要解决什么

这一阶段的唯一主问题应该是：

- 让一个最小远端消费者稳定消费现有 thread summary / status 增量 / reconnect 语义

它适合覆盖的能力包括：

- 用 `thread/list` 渲染线程摘要
- 用 `thread/loaded/read` 刷新 loaded thread 的 `mode + status`
- 用 `thread/status/changed` 消费 status-only 增量
- 用 `thread/resume` 进入 resident reconnect
- 把 `interactive -> resume`、`residentAssistant -> reconnect` 做成真实动作映射

它不应混入：

- 新 transport 协议设计
- observer 新事件类型
- SQLite schema 扩展
- 多 agent UI
- 完整网页 / 桌面产品壳层

## 2. 先读哪些文档和代码

建议按下面顺序进入：

1. `docs/remote-bridge-consumption.md`
2. `docs/app-server-thread-mode-v2.md`
3. `codex-rs/app-server/README.md`
4. `codex-rs/app-server/src/codex_message_processor.rs`
5. `codex-rs/app-server-client/`
6. `codex-rs/debug-client/`
7. `codex-rs/app-server-test-client/`

这些位置分别对应：

- 远端消费约束
- 线程模式契约
- 服务端接口说明
- 服务端读取面实现
- typed/in-process client
- 最小调试消费者
- 最小联调消费者

## 3. 开发时逐条检查什么

### 摘要来源

- 首页或列表摘要来自 `thread/list`
- loaded 运行态摘要来自 `thread/loaded/read`
- 不把 `thread/loaded/list` 当成完整摘要来源

### 增量来源

- `thread/status/changed` 只负责 status-only 增量
- 不从 `thread/status/changed` 里猜 `mode`
- 如果本地缓存里没有 thread summary，就回到读取面补 summary，而不是脑补 resident 语义

### 动作映射

- `interactive -> resume`
- `residentAssistant -> reconnect`

这条映射应直接来自 `Thread.mode`，而不是：

- source
- resident 布尔值
- 历史名字
- 某个局部 UI 状态

### reconnect 路径

- resident thread 统一通过 `thread/resume` 进入 reconnect
- reconnect 后仍以返回的 `Thread` 摘要作为权威启动状态
- 不要求 bridge 自己从 item 历史重建 resident 语义

## 4. 最值得补的负向边界

- 不把 `thread/status/changed` 当成完整摘要
- 不从 unknown thread id 的通知里猜 `mode`
- 不把 `thread/loaded/list` 当成带 `mode + status` 的接口
- 不把 `workspaceChanged` 简化显示成“线程仍在执行”
- 不把 resident reconnect 重新退回泛化的 resume / reopen 文案

## 5. 最适合先验证的消费者

第一批验证最好优先选这些现有入口，而不是先做新 UI：

- `app-server-test-client`
- `debug-client`
- `app-server-client` typed 请求

原因是它们已经足够证明：

- 现有 thread API 是否足以支撑远端消费
- 问题到底出在消费契约还是出在实现缺口

## 6. 最先跑哪些测试

优先跑这些：

- `cargo test -p codex-app-server`
- `cargo test -p codex-app-server-client`
- `cargo test -p codex-debug-client`

如果改动主要落在 test client，也应补跑对应 crate 的测试。

## 7. Review 时最值得问什么

1. 这个改动是不是只做“最小线程消费闭环”
2. 摘要来源和增量来源是否仍保持分层
3. `Thread.mode` 是否仍是动作映射的唯一主信号
4. resident reconnect 是否仍统一走 `thread/resume`
5. unknown thread / empty page / 分页续页这些边界是否有测试
6. README / help / 调试输出是否与真实消费逻辑一致

## 8. 什么情况下这一阶段算完成

至少应满足：

- 一个最小远端消费者已经能稳定列出 resident 与 interactive 线程
- loaded 线程的当前 `mode + status` 能通过 `thread/loaded/read` 稳定刷新
- `thread/status/changed` 能作为后续 status-only 增量消费
- resident thread 的动作语义在消费者侧已稳定表现为 reconnect
- 调试/联调入口不再残留另一套 resume 语义
- 后续 bridge 产品壳层可以建立在这套消费闭环之上，而不需要重新解释 thread 语义

## 9. 这一阶段完成后下一步做什么

更合适的下一步是：

- observer 正式事件源收敛

因为只有当最小消费者已经成立，后续 observer 强化才更容易判断是在补状态来源，还是在替消费侧兜底。
