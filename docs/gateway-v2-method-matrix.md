# Gateway v2 Method Coverage Matrix

This matrix defines the Stage A compatibility target for `codex-gateway` when a
Codex client connects over the app-server v2 WebSocket protocol.

Source inventory:

- client request usage was derived from
  `codex-rs/tui/src/app_server_session.rs`
- client server-request handling was derived from
  `codex-rs/tui/src/app/app_server_requests.rs`
- current gateway implementation status was checked against
  `codex-rs/gateway/src/northbound/v2.rs`,
  `codex-rs/gateway/src/v2.rs`, and `codex-rs/gateway/src/embedded.rs`

Stage A scope:

- release-quality baseline topologies: embedded runtime and remote runtime with
  exactly one downstream worker
- transport model: one northbound v2 connection maps to one downstream
  app-server session
- multi-worker remote runtime now has partial Stage B support; this matrix keeps
  Stage A as the drop-in baseline and calls out the current multi-worker
  additions separately where they affect rollout guidance

Status legend:

- `transparent passthrough`: gateway proxies the v2 method without
  method-specific translation
- `policy interception`: gateway intentionally terminates or shapes part of the
  flow at the transport boundary
- `deferred`: not part of the current drop-in target, or blocked on a broader
  topology/policy gap

## Handshake and Session Lifecycle

| Method / flow | Why current clients use it | Gateway status | Notes |
| --- | --- | --- | --- |
| `initialize` | Required by every v2 client session | `policy interception` | Gateway authenticates the WebSocket upgrade, enforces an explicit initialize handshake timeout, connects a downstream app-server session, propagates client identity/capabilities downstream, and returns a gateway-owned `InitializeResponse`. |
| `initialized` | Required handshake completion notification | `transparent passthrough` | Gateway accepts the northbound notification and downstream session setup already sends `initialized` as part of the app-server client transport. |
| connection close / downstream disconnect | Required for normal session teardown | `transparent passthrough` | Downstream disconnect currently terminates the northbound connection. |

## Bootstrap and Session Setup Requests

These are the v2 requests the current TUI issues during bootstrap or early
session setup.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `account/read` | Bootstrap auth/account state | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `account/read` refresh re-adds a recovered worker into merged auth state for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `account/rateLimits/read` | Background rate-limit refresh for status surfaces | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded and single-worker remote compatibility harnesses, and by multi-worker remote aggregation coverage that preserves the primary historical view from the first worker while merging the per-limit map across workers. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `account/rateLimits/read` refresh re-adds a recovered worker into the shared per-limit view for the same session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `model/list` | Bootstrap model picker/default model discovery | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `model/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also has reconnect regression coverage showing that a later unpaginated aggregated `model/list` refresh re-adds a recovered worker into merged model inventory for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `externalAgentConfig/detect` | Import existing external agent config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `externalAgentConfig/detect` refresh still succeeds after worker recovery. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `externalAgentConfig/detect` refresh re-adds a recovered worker into merged import-discovery results for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `externalAgentConfig/import` | Import external agent config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Real multi-worker reconnect coverage now also verifies that a later `externalAgentConfig/import` request reconnects a recovered worker before the shared import fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `app/list` | Connector/app discovery and refresh | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later threadless `app/list` refresh still succeeds after worker recovery. Multi-worker remote mode now aggregates the current threadless `app/list` path across workers, including gateway-owned pagination over the merged result set, while thread-scoped `app/list` stays sticky to the owning worker and is covered by the real multi-worker compatibility harness. Dedicated reconnect regression coverage now also verifies that a later threadless `app/list` refresh re-adds a recovered worker into the shared aggregated result set, that a later thread-scoped `app/list` request still routes to that recovered worker on the same session, and dedicated northbound websocket reconnect coverage now pins the threadless recovered-worker aggregation path on the shared client session. |
| `mcpServerStatus/list` | MCP inventory/bootstrap refresh | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `mcpServerStatus/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates connection-scoped MCP server inventory across workers with gateway-owned pagination. Dedicated reconnect regression coverage now also verifies that a later `mcpServerStatus/list` refresh re-adds a recovered worker into the shared aggregated result set, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `mcpServer/oauth/login` | Start OAuth login for a configured MCP server | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded, single-worker remote, and multi-worker remote compatibility harnesses, including the follow-up `mcpServer/oauthLogin/completed` notification on the northbound client session. The real single-worker reconnect harness now also verifies that a later `mcpServer/oauth/login` still succeeds after worker recovery and still forwards the completion notification on the recovered session. Multi-worker remote mode now also uses worker-discovery fallback for this request, with dedicated reconnect regression coverage showing that a later `mcpServer/oauth/login` can reconnect a recovered worker before selecting the downstream session that owns the named MCP server, and dedicated northbound websocket reconnect coverage now also pins the follow-up `mcpServer/oauthLogin/completed` notification on that recovered session. Real multi-worker same-session recovery coverage now also verifies that `mcpServer/oauth/login` routes back to the recovered worker and still forwards the resulting completion notification on one shared client session. |
| `skills/list` | Skills picker / refresh | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `skills/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates `skills/list` across workers, suppresses duplicate `skills/changed` invalidations until the client refreshes, and has dedicated reconnect regression coverage showing that a later `skills/list` request re-adds a recovered worker into the shared session result set. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `plugin/list` | Plugin catalog discovery | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. The real single-worker reconnect harness now also verifies that a later threadless `plugin/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also has dedicated aggregation coverage for merged marketplace inventory, featured plugins, and installed-state selection across workers. Dedicated reconnect regression coverage now also verifies that a later threadless `plugin/list` refresh re-adds a recovered worker into the shared marketplace view, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can re-include the recovered worker's plugin state without reconnecting. |
| `plugin/read` | Plugin detail view | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. The real single-worker reconnect harness now also verifies that a later `plugin/read` request still succeeds after worker recovery. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the selected plugin request. Dedicated reconnect regression coverage now also verifies that a later `plugin/read` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/read` now fails closed instead of selecting from an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/read` routes back to the recovered worker on one shared client session. |
| `config/value/write` | Plugin enablement and other targeted config updates | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests; multi-worker remote mode now fans this connection-scoped mutation out across workers so plugin/config state does not drift. Real multi-worker reconnect coverage now also verifies that a later `config/value/write` request reconnects a recovered worker before the shared write fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `config/batchWrite` | Reload user config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Real multi-worker reconnect coverage now also verifies that a later `config/batchWrite` request reconnects a recovered worker before the shared write fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `account/login/start` | Onboarding sign-in start flow | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded compatibility harness for the external-auth `chatgptAuthTokens` path, by the real single-worker remote compatibility harness for both managed-auth and external-auth onboarding flows, by the real multi-worker remote compatibility harness for the current external-auth onboarding path, and by dedicated multi-worker northbound regression coverage that verifies `apiKey` and `chatgptAuthTokens` login fanout to every downstream worker session while managed ChatGPT login flows remain on the primary worker because they return worker-local `loginId` state. Dedicated northbound websocket reconnect coverage now also pins both the recovered-worker fanout path for external-auth `apiKey` login and the recovered primary-worker path for managed ChatGPT login on the shared client session. Real multi-worker same-session recovery coverage now also verifies the managed primary-worker login path after the recovered worker is re-added. |
| `account/login/cancel` | Cancel a pending onboarding sign-in flow | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded compatibility harness for post-login cancellation semantics, by the real single-worker remote compatibility harness for both managed-auth and external-auth onboarding flows, and by the real multi-worker remote compatibility harness for the current primary-worker external-auth path. Dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker path on the shared client session. Real multi-worker same-session recovery coverage now also verifies that `account/login/cancel` still reaches the recovered primary worker on one shared client session. |
| `account/logout` | Sign-out flow | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Real multi-worker reconnect coverage now also verifies that a later `account/logout` request reconnects a recovered worker before the shared logout fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `memory/reset` | Reset memory mode state | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Real multi-worker reconnect coverage now also verifies that a later `memory/reset` request reconnects a recovered worker before the shared reset fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `feedback/upload` | Submit in-product feedback from an active session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded, single-worker remote, and multi-worker remote compatibility harnesses. Dedicated northbound websocket reconnect coverage now also pins the recovered primary-worker route on the shared client session. Real multi-worker same-session recovery coverage now also verifies that `feedback/upload` still routes through the recovered primary worker without reconnecting the northbound client. |

## Supporting Configuration Requests

These requests are used by some clients and test tools outside the main TUI
bootstrap hot path, but they still need stable v2 compatibility behavior.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `config/read` | Read effective config and optional layer metadata | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded, single-worker remote, and multi-worker compatibility coverage for both threadless and cwd-aware reads. The real single-worker reconnect harness now also verifies that a later threadless `config/read` refresh still succeeds after worker recovery. Multi-worker remote mode now also has reconnect regression coverage showing that later threadless and cwd-aware `config/read` refreshes re-add a recovered worker before selecting either the recovered primary worker or the config layers that match the requested cwd. Dedicated northbound websocket reconnect coverage now also pins both recovered-worker selection paths on the shared client session. During reconnect retry backoff, threadless `config/read` now fails closed instead of silently falling through to a surviving secondary worker's config, and cwd-aware `config/read` now fails closed instead of silently falling back to a surviving worker whose config layers do not match the requested path. |
| `configRequirements/read` | Read config requirements and validation metadata | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded, single-worker remote, and primary-worker multi-worker compatibility coverage. The real single-worker reconnect harness now also verifies that a later `configRequirements/read` request still succeeds after worker recovery. Dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker path on the shared client session. Real multi-worker same-session recovery coverage now also verifies that `configRequirements/read` still routes through the recovered primary worker on one shared client session. |
| `experimentalFeature/list` | Discover experimental capability flags | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `experimentalFeature/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates capability discovery across workers. Dedicated reconnect regression coverage now also verifies that a later aggregated `experimentalFeature/list` refresh re-adds a recovered worker into the shared feature inventory. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `collaborationMode/list` | Discover supported collaboration modes | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `collaborationMode/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also deduplicates and sorts the aggregated capability set across workers. Dedicated reconnect regression coverage now also verifies that a later aggregated `collaborationMode/list` refresh re-adds a recovered worker into the shared preset inventory. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `fs/watch` | Subscribe the current connection to filesystem change notifications | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Multi-worker remote mode now fans connection-scoped watches out across workers so one shared northbound session can observe worker-local filesystem changes instead of only the primary worker's watch set. Dedicated reconnect regression coverage now also verifies that a later `fs/watch` request reconnects a recovered worker before the shared watch is applied, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker fanout on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can still fan `fs/watch` back out across the recovered worker and the surviving worker without reconnecting. |
| `fs/unwatch` | Remove a prior filesystem watch for the current connection | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Multi-worker remote mode now fans connection-scoped unwatch requests out across workers so one shared northbound session can tear down worker-local watch state consistently. Dedicated reconnect regression coverage now also verifies that a later `fs/unwatch` request reconnects a recovered worker before the shared watch teardown runs, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker fanout on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can still fan `fs/unwatch` back out across the recovered worker and the surviving worker without reconnecting. |

Additional multi-worker recovery coverage:

- Real same-session recovery tests now also verify that later aggregated
  refreshes for `account/read`, `account/rateLimits/read`, `model/list`,
  `externalAgentConfig/detect`, threadless `app/list`, threadless
  `plugin/list`, `skills/list`, `mcpServerStatus/list`,
  `thread/realtime/listVoices`, cwd-aware `config/read`,
  `experimentalFeature/list`, and `collaborationMode/list` re-add a recovered
  worker into the shared northbound session instead of leaving bootstrap and
  discovery state pinned to the surviving subset.
- The same lazy-reconnect path now also replays active `fs/watch`
  registrations onto any re-added downstream worker session before later
  filesystem watch traffic routes there, so shared watch subscriptions survive
  worker recovery on one northbound connection.

## Command Execution Requests

These requests drive the standalone `command/exec` control plane outside the
normal thread/turn flow.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `command/exec` | Start a standalone command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests, including a streaming/PTTY-shaped request body plus the final `command/exec` response. The real embedded compatibility harness now validates the in-process baseline flow for `command/exec` plus `command/exec/outputDelta`. The real single-worker remote harness now validates the broader standalone control plane through a gateway-backed remote worker session, and the real multi-worker primary-worker harness now validates the baseline flow through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec` plus `command/exec/outputDelta` still flow through the re-added primary worker without reconnecting the northbound client. |
| `command/exec/write` | Write stdin bytes to an active command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real single-worker remote harness now also validates `command/exec/write` through a gateway-backed remote worker session, and the real multi-worker primary-worker harness now validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/write` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/write` still reaches the re-added primary worker on one shared client session. |
| `command/exec/resize` | Resize an active PTY-backed command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real single-worker remote harness now also validates `command/exec/resize` through a gateway-backed remote worker session, and the real multi-worker primary-worker harness now validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/resize` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/resize` still reaches the re-added primary worker on one shared client session. |
| `command/exec/terminate` | Terminate an active command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real single-worker remote harness now also validates `command/exec/terminate` through a gateway-backed remote worker session, and the real multi-worker primary-worker harness now validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/terminate` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/terminate` still reaches the re-added primary worker on one shared client session. |

## Plugin Management Requests

These are the plugin-management requests the current TUI issues after bootstrap.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `plugin/install` | Install a selected plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. The real single-worker reconnect harness now also verifies that a later `plugin/install` request still succeeds after worker recovery and still updates follow-up `plugin/list` state. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the install request. Dedicated reconnect regression coverage now also verifies that a later `plugin/install` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/install` now fails closed instead of mutating plugin state on an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/install` can still mutate recovered-worker plugin state without reconnecting the northbound client. |
| `plugin/uninstall` | Remove an installed plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. The real single-worker reconnect harness now also verifies that a later `plugin/uninstall` request still succeeds after worker recovery and still updates follow-up `plugin/list` state. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the uninstall request. Dedicated reconnect regression coverage now also verifies that a later `plugin/uninstall` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/uninstall` now fails closed instead of mutating plugin state on an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/uninstall` can still clear recovered-worker plugin state without reconnecting the northbound client. |

## Thread and Turn Requests

These methods are used by the current TUI for normal thread lifecycle and turn
control.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/start` | Start a new session | `transparent passthrough` | Covered by embedded and single-worker remote gateway tests. Real multi-worker remote compatibility tests now also cover steady-state `thread/start` routing across one shared northbound session, and same-session recovery coverage now also verifies that a later `thread/start` can reuse a re-added worker without reconnecting the client. |
| `thread/resume` | Resume an existing session | `policy interception` | `threadId`-based resume is covered by the real embedded thread re-entry harness, by single-worker remote compatibility tests, and by real multi-worker routing and same-session recovery regressions. History-based resume is now treated as connection-scoped so it does not trip visibility checks on its placeholder `threadId`, and path-based resume is now allowed when the rollout path is already visible in the current tenant/project scope; real multi-worker compatibility coverage plus dedicated northbound JSON-RPC regressions cover the visible-path success path, and the real multi-worker same-session recovery harness now also verifies that visible-path resume stays sticky on a recovered worker after reconnect, while embedded and single-worker remote harnesses continue to pin the hidden-path rejection path. |
| `thread/fork` | Fork an existing session | `policy interception` | `threadId`-based fork is covered by single-worker remote compatibility tests plus real multi-worker routing and same-session recovery regressions, including follow-up `thread/read` route backfill for the returned forked thread. Path-based fork is now allowed when the rollout path is already visible in the current tenant/project scope; real multi-worker compatibility coverage plus dedicated northbound JSON-RPC regressions cover the visible-path success path, and the real multi-worker same-session recovery harness now also verifies that visible-path fork stays sticky on a recovered worker after reconnect, while embedded and single-worker remote harnesses continue to pin the hidden-path rejection path. |
| `thread/list` | Session picker/history | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/list` on one shared northbound session, while multi-worker routing continues to deduplicate repeated thread ids across workers, select the newest visible thread metadata, and backfill sticky worker ownership from that winner. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/list` refresh re-adds a recovered worker into the shared visible-thread set and restores sticky ownership for its threads, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation plus sticky-route backfill on the shared client session. |
| `thread/loaded/list` | Discover currently loaded subagent threads | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests, including scope filtering. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/loaded/list` on one shared northbound session. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/loaded/list` refresh re-adds a recovered worker into the shared loaded-thread set and restores sticky ownership for its threads, dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation plus sticky-route backfill on the shared client session, and the real multi-worker same-session recovery harness now also exercises that recovered `thread/loaded/list` path through `RemoteAppServerClient`. |
| `thread/read` | Read thread metadata/history | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/read` routing plus scope-filtered reads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/read` on the same session still routes to a recovered worker, including a follow-up read of a recovered-worker review thread materialized after reconnect. |
| `thread/name/set` | Rename thread | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/name/set`, including follow-up `thread/read` that reflects the worker-local rename on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/name/set` on the same session still routes to a recovered worker and is reflected by a follow-up `thread/read`, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `thread/memoryMode/set` | Update memory mode | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/memoryMode/set` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/memoryMode/set` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `thread/unsubscribe` | Drop local subscription | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/unsubscribe` through the recovered worker. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/unsubscribe` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/unsubscribe` on the same session still routes to a recovered worker. |
| `thread/compact/start` | Compact thread history | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/compact/start` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/compact/start` on the same session still routes to a recovered worker. |
| `thread/shellCommand` | Start shell command from thread context | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/shellCommand` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/shellCommand` on the same session still routes to a recovered worker. |
| `thread/backgroundTerminals/clean` | Clean background terminals | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/backgroundTerminals/clean` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/backgroundTerminals/clean` on the same session still routes to a recovered worker. |
| `thread/rollback` | Roll back thread turns | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/rollback` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/rollback` on the same session still routes to a recovered worker. |
| `review/start` | Start review flow | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `thread/read` of the returned detached review thread. Real single-worker remote reconnect coverage now also verifies that a later client session can still `review/start` plus follow-up `thread/read` of the detached review thread through the recovered worker. Real multi-worker remote compatibility tests now also cover steady-state sticky `review/start` on one shared northbound session, including follow-up `thread/read` of worker-owned review threads. Real multi-worker reconnect coverage now also verifies that a later sticky `review/start` on the same session still routes to a recovered worker and re-registers the returned review thread for follow-up `thread/read`, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `turn/start` | Start a turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests, including turn lifecycle notifications. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/start` plus shared-session turn lifecycle fan-in across worker-owned threads. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/start` on the same session still routes to a recovered worker, and that the recovered worker's turn lifecycle notifications still fan in on the shared northbound session. Dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `turn/interrupt` | Interrupt active turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/interrupt` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/interrupt` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `turn/steer` | Steer active turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/steer` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/steer` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |

## Additional Thread-Control Requests

These lower-frequency requests are not central to the Stage A bootstrap flow,
but they already have dedicated passthrough or real-client gateway coverage.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/archive` | Archive a thread from history views | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/archive` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/archive` on the same session still routes to a recovered worker. |
| `thread/unarchive` | Restore an archived thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/unarchive` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/unarchive` on the same session still routes to a recovered worker. |
| `thread/metadata/update` | Update thread metadata such as git/context info | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/metadata/update` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/metadata/update` on the same session still routes to a recovered worker. |
| `thread/turns/list` | Page thread turns for history/review surfaces | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/turns/list` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/turns/list` on the same session still routes to a recovered worker. |
| `thread/increment_elicitation` | Increase elicitation counters on a thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/increment_elicitation` on the same session still routes to a recovered worker. |
| `thread/decrement_elicitation` | Decrease elicitation counters on a thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/decrement_elicitation` on the same session still routes to a recovered worker. |
| `thread/inject_items` | Inject items into an existing thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/inject_items` on the same session still routes to a recovered worker. |

## Realtime Requests

These methods are used by the TUI realtime conversation path.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/realtime/listVoices` | Discover available realtime voices before a session starts | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/realtime/listVoices` on one shared northbound session, deduplicating and preserving the merged default-voice selection across workers. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/realtime/listVoices` refresh re-adds a recovered worker into the shared voice inventory for the same session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `thread/realtime/start` | Start realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/start` across worker-owned threads on one shared northbound session, with shared-session fan-in for the resulting realtime notifications. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/start` on the same session still routes to a recovered worker, and that the recovered worker's realtime notifications still fan in on the shared northbound session. Dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `thread/realtime/appendAudio` | Stream microphone audio | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/appendAudio` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/appendAudio` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `thread/realtime/appendText` | Stream text into realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/appendText` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/appendText` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |
| `thread/realtime/stop` | Stop realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/stop` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/stop` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. |

## Server Notifications

The current gateway forwards downstream server notifications back to the client
on the same WebSocket connection without method-specific translation.

Required notification behavior for Stage A:

- `thread/started`
- `thread/status/changed`
- `turn/started`
- `turn/completed`
- item delta/update notifications used during active turns

Current status:

- notification forwarding is `transparent passthrough`
- `thread/started`, `thread/status/changed`, `turn/started`, and
  `turn/completed` are covered by gateway compatibility tests
- `item/agentMessage/delta` is covered by a dedicated gateway compatibility
  test for active-turn streaming notifications
- `item/started` and `item/completed` are now covered by dedicated gateway
  compatibility tests for longer-running turn item lifecycle notifications
- `hookStarted` and `hookCompleted` are now covered by dedicated gateway
  compatibility tests for hook lifecycle notifications
- the real single-worker remote compatibility harness now also covers
  `item/started`, `item/completed`, `hookStarted`, and `hookCompleted`
  during a full `turn/start` workflow
- `item/guardianApprovalReviewStarted` and
  `item/guardianApprovalReviewCompleted` are now covered by dedicated gateway
  compatibility tests for approval auto-review lifecycle notifications
- `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta` are now covered
  by real single-worker remote and multi-worker remote turn-workflow
  compatibility tests
- dedicated gateway notification coverage now also includes
  `plan/delta`, `reasoning/summaryPartAdded`, `terminalInteraction`,
  `turn/diffUpdated`, `turn/planUpdated`, `thread/name/updated`,
  `thread/tokenUsage/updated`, `mcpToolCall/progress`,
  `contextCompacted`, and `model/rerouted`
- `thread/realtime/started` is covered by a dedicated gateway compatibility
  test for experimental realtime notification forwarding
- additional experimental realtime notifications now also have dedicated
  gateway compatibility coverage for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- the real embedded compatibility harness now also covers realtime
  notification forwarding for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed` during a full
  sideband-backed realtime workflow
- dedicated gateway connection-notification coverage now also includes
  `command/exec/outputDelta`, so streamed standalone command output is pinned at
  the northbound transport boundary too
- multi-worker northbound v2 regression coverage now also verifies fan-in for
  experimental realtime notifications on one shared client session, now
  covering the full current set:
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- the real multi-worker remote compatibility harness now also covers
  `item/started`, `item/completed`, `hookStarted`, and `hookCompleted`
  fan-in during worker-owned `turn/start` workflows
- lower-frequency user-visible notifications now also have dedicated gateway
  compatibility coverage for `warning`, `configWarning`,
  `deprecationNotice`, and `mcpServer/startupStatus/updated`
- the real embedded compatibility harness now also covers `configWarning`
  during startup, and the real single-worker remote compatibility harness now
  also covers `warning`, `configWarning`, `deprecationNotice`,
  `account/login/completed`, `mcpServer/oauthLogin/completed`, and
  `mcpServer/startupStatus/updated`, so those visible notification paths are
  exercised through unmodified `RemoteAppServerClient` sessions in addition to
  dedicated northbound coverage
- the real embedded and single-worker remote compatibility harnesses now also
  cover `thread/name/updated` during the `thread/name/set` rename flow, so the
  visible rename notification path is exercised through unmodified
  `RemoteAppServerClient` sessions in addition to dedicated northbound
  passthrough coverage
- dedicated gateway connection-notification coverage now also includes
  `account/login/completed` for onboarding auth completion flows
- dedicated gateway connection-notification coverage now also includes
  `mcpServer/oauthLogin/completed` for MCP OAuth completion status
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `account/login/completed`, so one shared northbound session
  does not surface duplicate onboarding-complete notifications when a fanout
  login flow completes on more than one worker
- that same exact-duplicate suppression now also covers
  `mcpServer/oauthLogin/completed`, so one shared northbound session does not
  surface duplicate MCP OAuth completion notifications when more than one
  worker emits the same payload
- the real multi-worker connection-state harnesses now also verify that
  `mcpServer/oauthLogin/completed` is emitted exactly once both in steady
  state and after worker reconnect, so that dedupe path is exercised through
  an unmodified `RemoteAppServerClient` session in addition to dedicated
  northbound regressions
- that same exact-duplicate suppression now also covers connection-scoped
  `warning`, `configWarning`, and `deprecationNotice`, so one shared
  northbound session does not surface duplicate visible notices when more than
  one worker emits the same payload
- dedicated gateway connection-notification coverage now also includes
  `externalAgentConfig/import/completed`, so fanout imports do not surface
  duplicate completion notifications on one shared multi-worker session
- the real embedded compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- the real single-worker remote compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- the real single-worker remote compatibility harness now also covers
  `account/login/completed` notification delivery during the onboarding auth
  flow
- the real multi-worker remote compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- real multi-worker same-session recovery coverage now also verifies managed
  `account/login/completed` delivery after the recovered primary worker is
  re-added to one shared client session
- dedicated northbound multi-worker websocket regression coverage now also
  verifies that a managed `account/login/start` can reconnect a missing
  primary worker and still forward that recovered worker's resulting
  `account/login/completed` notification over the shared northbound session
- broader notification hardening is still deferred to parity validation and
  rollout work, but the current TUI-facing realtime notification set now has
  dedicated gateway coverage

## Server Requests

These are the server-initiated requests the current TUI explicitly handles.

| Method | Client handling status | Gateway status | Notes |
| --- | --- | --- | --- |
| `item/commandExecution/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/fileChange/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/permissions/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/tool/requestUserInput` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests and by the real multi-worker remote `RemoteAppServerClient` harness, which verifies per-worker request-id translation on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this request/response path without leaking stale worker-local request IDs. |
| `mcpServer/elicitation/request` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this elicitation path without leaking stale worker-local request IDs. |
| `item/tool/call` | TUI marks unsupported today | `transparent passthrough` | This is a client capability gap, not a gateway transport gap; dedicated northbound passthrough coverage now verifies the request/response round trip, the real embedded and single-worker remote compatibility harnesses now also exercise that same transport path over the northbound client connection, with the embedded harness verifying `serverRequest/resolved` plus `item/started` / `item/completed` forwarding for the dynamic-tool call lifecycle in steady state, the single-worker harness verifying that same lifecycle ordering in steady state, the single-worker reconnect harness verifying the same ordering after worker recovery, and the real multi-worker remote harness verifying translated per-worker request IDs on one shared northbound session. |
| `account/chatgptAuthTokens/refresh` | TUI accepts | `transparent passthrough` | Covered by dedicated northbound server-request round-trip tests plus single-worker and multi-worker remote round-trip tests, with the real single-worker `RemoteAppServerClient` harness now covering both steady-state and post-reconnect recovery, and the multi-worker harness covering translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker can resume this connection-scoped refresh round trip without leaking stale worker-local request IDs, and real multi-worker disconnect coverage now also verifies that the shared session fails closed both while a refresh prompt is still pending and while the client response is still awaiting downstream `serverRequest/resolved`. Embedded mode currently relies on targeted northbound coverage for this path because the in-process downstream app-server client rejects ChatGPT token-refresh prompts before they can surface through the real embedded compatibility harness. |
| legacy `apply_patch` / `exec_command` approvals | TUI marks unsupported today | `transparent passthrough` | These are not part of the primary Stage A drop-in target. Dedicated northbound websocket regression coverage now verifies round trips for legacy `applyPatchApproval` and `execCommandApproval`, and scope-enforcement unit coverage now also pins that their `conversationId` is treated as thread ownership so legacy prompts do not bypass gateway visibility checks. |

Additional transport behavior:

- real multi-worker same-session recovery coverage now also verifies that after
  a dropped worker is lazily re-added on one shared client session, translated
  thread-scoped approval / elicitation requests plus the connection-scoped
  `account/chatgptAuthTokens/refresh` flow still round trip through that
  worker correctly
- when one shared northbound client session has already completed the v2
  handshake, any later lazily re-added downstream worker now also receives the
  forwarded `initialized` notification before subsequent requests route there,
  so reconnect recovery restores full post-handshake app-server session state
- dedicated northbound multi-worker websocket regression coverage now also
  verifies that a later client request can reconnect a missing worker and then
  still forward subsequent server requests from that recovered worker over the
  same shared northbound session

- if a northbound v2 connection ends while a forwarded server request is still
  pending, including client disconnects and malformed-payload protocol closes,
  plus other terminal northbound I/O failures such as client-send timeouts or
  broken pipes,
  the gateway now rejects that pending request back to the downstream
  app-server session with a gateway-owned internal error instead of leaving it
  unresolved
- that teardown path now also emits structured warning logs with the
  connection outcome, terminal detail, pending gateway request ids, per-scope
  counts, and affected worker ids, so operators can distinguish disconnect-
  driven cleanup from downstream request failures
- if a northbound client sends a `JSONRPCResponse` or `JSONRPCError` for a
  server request that is no longer pending, the gateway now treats that as a
  protocol violation, closes the northbound WebSocket, and rejects any other
  still-pending downstream server requests instead of silently ignoring the
  out-of-order reply
- dedicated northbound regression coverage now also pins the scope and
  pending-request-id warning fields for both response and error variants of
  that protocol-violation path before the socket is closed
- dedicated northbound regression coverage now also pins those same teardown
  warning fields for malformed post-handshake client payloads, so invalid
  JSON-RPC frames do not rely only on close-frame assertions
- that same malformed-payload teardown coverage now also pins any answered-
  but-unresolved server-request routes after the client has already replied but
  downstream `serverRequest/resolved` has not arrived yet
- dedicated northbound regression coverage now also pins the answered-but-
  unresolved variant of that protocol-violation path for both response and
  error frames, so already-answered downstream routes still appear in teardown
  logs before the connection closes
- if a downstream app-server session reuses a still-pending server-request id,
  the gateway now closes the northbound WebSocket with a gateway-owned error
  close and rejects the older pending request instead of silently overwriting
  the original route
- that collision path now also emits a structured warning log with the scope,
  worker id, colliding request id/method, and the gateway request ids already
  pending on the connection
- if one downstream worker disconnects during a multi-worker session while a
  thread-scoped server request from that worker is still pending or waiting on
  downstream `serverRequest/resolved`, the gateway now synthesizes
  `serverRequest/resolved` northbound so the client can dismiss that prompt
  instead of leaving it stranded on the shared session
- if one downstream worker disconnects during a multi-worker session while a
  connection-scoped server request from that worker is still pending or
  awaiting downstream `serverRequest/resolved`, the gateway now fails closed
  and ends the northbound WebSocket instead of silently dropping the
  unresolved prompt
- dedicated northbound regression coverage now also exercises the thread-scoped
  answered-but-unresolved variant of that worker-disconnect path, verifying
  that `serverRequest/resolved` is still synthesized after the client has
  already replied but before the worker can confirm resolution

## Topology and Policy Gaps

These are the known gaps that remain after the current Stage A coverage work.

| Area | Status | Notes |
| --- | --- | --- |
| remote runtime with more than one worker | `partial` | Northbound v2 WebSocket upgrades are now admitted in multi-worker remote mode, with one downstream session per worker plus aggregated `thread/list` / `thread/loaded/list`, deduplicated multi-worker `thread/list` results that keep the newest visible snapshot per thread id, aggregated bootstrap/setup discovery for `account/read`, `model/list`, `externalAgentConfig/detect`, `app/list`, `skills/list`, `mcpServerStatus/list`, and threadless `plugin/list`, translated server-request IDs across workers for `item/tool/requestUserInput`, `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `item/permissions/requestApproval`, `mcpServer/elicitation/request`, and `account/chatgptAuthTokens/refresh`, including a dedicated real-client regression where two workers keep the same downstream request id pending at once for `item/tool/requestUserInput`, `item/permissions/requestApproval`, and `mcpServer/elicitation/request` and the gateway still assigns distinct northbound ids, sticky thread routing, worker-discovery routing for `plugin/read` / `plugin/install` / `plugin/uninstall` / `mcpServer/oauth/login`, fanout for connection-scoped setup mutations such as `account/login/start` for external-auth `apiKey` and `chatgptAuthTokens`, `externalAgentConfig/import`, `config/batchWrite`, `memory/reset`, `account/logout`, `fs/watch`, and `fs/unwatch`, deduplicated `skills/changed` invalidation notifications until the client refreshes with `skills/list`, exact-duplicate suppression for connection-state notifications such as `account/updated`, `account/rateLimits/updated`, `account/login/completed`, `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`, `mcpServer/startupStatus/updated`, `mcpServer/oauthLogin/completed`, and `externalAgentConfig/import/completed`, synthesized `serverRequest/resolved` for thread-scoped prompts stranded by a worker disconnect, route backfill for already visible threads discovered through aggregated `thread/list` / `thread/loaded/list` plus lazy route recovery for later thread-scoped requests when that worker mapping is missing, in-session survival when one downstream worker disconnects and the remaining workers can still serve the shared northbound connection, lazy re-add of a recovered worker on later client requests so the same northbound session can resume routing work back onto that worker, and a short per-worker reconnect retry backoff so repeated worker failures do not trigger an immediate reconnect attempt on every later client request. Connection-scoped fanout mutations now also fail closed while any worker remains unavailable during retry backoff, instead of silently drifting shared setup or watch state onto only the surviving subset. Threadless `config/read` now also fails closed while the primary worker remains unavailable during retry backoff, instead of silently falling through to a surviving secondary worker's config. Cwd-aware `config/read` now also fails closed while any worker remains unavailable during retry backoff, instead of silently falling back to a surviving worker whose config layers do not match the requested path. Worker-discovery plugin requests now also fail closed while any worker remains unavailable during retry backoff, instead of selecting from an incomplete plugin inventory. That same worker-discovery fail-closed behavior now also applies to `mcpServer/oauth/login` while any worker remains unavailable during retry backoff, instead of selecting from an incomplete MCP inventory. Primary-worker-only requests now also fail closed while that worker remains unavailable during retry backoff, instead of silently falling through to a surviving secondary worker. That same degraded-session path now keeps multi-worker semantics active while only one worker is temporarily live, so connection-scoped fanout, dedupe, and gateway-owned server-request id translation do not silently collapse into single-worker handling before missing workers reconnect. Later refreshes of aggregated `account/read`, `account/rateLimits/read`, unpaginated `model/list`, threadless `config/read`, aggregated `thread/list`, aggregated `thread/loaded/list`, `plugin/read`, `plugin/install`, `plugin/uninstall`, `mcpServer/oauth/login`, `externalAgentConfig/detect`, `externalAgentConfig/import`, `experimentalFeature/list`, `collaborationMode/list`, `skills/list`, `app/list`, `mcpServerStatus/list`, threadless `plugin/list`, `config/batchWrite`, `config/value/write`, `memory/reset`, `account/logout`, `fs/watch`, and `fs/unwatch` can all now re-add a recovered worker. Broad steady-state and reconnect validation now exists for thread, turn, realtime, approval, setup, and recovery flows, but broader parity and rollout hardening are still pending before this matches embedded or single-worker remote support. |
| v2 scope enforcement | `complete` | Embedded, single-worker remote, and multi-worker remote v2 connections now derive tenant/project scope from the WebSocket upgrade headers, enforce thread visibility on thread-scoped requests, filter `thread/list` plus `thread/loaded/list`, reject downstream server requests for hidden threads, backfill worker ownership for already visible threads discovered through aggregated list responses, register resumed/forked thread IDs back into scope ownership, and now also register visible rollout paths so path-based `thread/resume` / `thread/fork` stay scope-checked instead of bypassing ownership. Hidden-thread downstream server requests now also emit structured warning logs with scope, worker id, request id, method, and hidden thread id when the gateway rejects them. History-based `thread/resume` is now treated as a connection-scoped request, so the gateway no longer rejects that app-server flow just because the placeholder `threadId` is not yet visible. Multi-worker remote runtime now also has real northbound `RemoteAppServerClient` regressions covering same-scope re-entry plus cross-scope filtering for aggregated `thread/list` / `thread/loaded/list` / `thread/read`, plus visible-path `thread/resume` / `thread/fork` routing through one shared client session. |
| v2 admission/rate limiting | `complete` | The gateway now applies the same per-scope request rate limit and turn-start quota policy to v2 JSON-RPC requests that it already applies to HTTP routes. |
| v2 per-message audit/metrics | `complete` | The gateway now emits per-request v2 metrics and audit logs for `initialize` and subsequent client JSON-RPC requests, tagged by method and outcome. |

## Required Test Subset

The current gateway compatibility tests cover this required subset:

- `initialize`
- `account/read`
- `account/rateLimits/read`
- `model/list`
- `externalAgentConfig/detect`
- `externalAgentConfig/import`
- `app/list`
- `mcpServerStatus/list`
- `mcpServer/oauth/login`
- `skills/list`
- `plugin/list`
- `plugin/read`
- `config/value/write`
- `plugin/install`
- `plugin/uninstall`
- `config/batchWrite`
- `memory/reset`
- `account/login/start`
- `account/login/cancel`
- `account/logout`
- `externalAgentConfig/detect`
- `externalAgentConfig/import`
- `app/list`
- `mcpServerStatus/list`
- `mcpServer/oauth/login`
- `skills/list`
- `plugin/list`
- `plugin/read`
- `config/read`
- `configRequirements/read`
- `experimentalFeature/list`
- `collaborationMode/list`
- `feedback/upload`
- `fs/watch`
- `fs/unwatch`
- `command/exec`
- `command/exec/write`
- `command/exec/resize`
- `command/exec/terminate`
- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/list`
- `thread/loaded/list`
- `thread/read`
- `thread/name/set`
- `thread/memoryMode/set`
- `thread/unsubscribe`
- `thread/archive`
- `thread/unarchive`
- `thread/metadata/update`
- `thread/turns/list`
- `thread/increment_elicitation`
- `thread/decrement_elicitation`
- `thread/inject_items`
- `thread/compact/start`
- `thread/shellCommand`
- `thread/backgroundTerminals/clean`
- `thread/rollback`
- `review/start`
- `thread/realtime/listVoices`
- `thread/realtime/start`
- `thread/realtime/appendAudio`
- `thread/realtime/appendText`
- `thread/realtime/stop`
- `turn/start`
- `turn/interrupt`
- `turn/steer`
- `thread/started` notification forwarding
- `thread/status/changed` notification forwarding
- `turn/started` notification forwarding
- `turn/completed` notification forwarding
- `hookStarted` notification forwarding
- `hookCompleted` notification forwarding
- `item/guardianApprovalReviewStarted` notification forwarding
- `item/guardianApprovalReviewCompleted` notification forwarding
- `item/started` notification forwarding
- `item/completed` notification forwarding
- `item/agentMessage/delta` notification forwarding
- `item/reasoning/summaryTextDelta` notification forwarding
- `item/reasoning/textDelta` notification forwarding
- `item/commandExecution/outputDelta` notification forwarding
- `item/fileChange/outputDelta` notification forwarding
- `thread/realtime/started` notification forwarding
- `account/updated` notification forwarding
- `account/rateLimits/updated` notification forwarding
- `app/list/updated` notification forwarding
- `account/login/completed` notification forwarding
- `mcpServer/oauthLogin/completed` notification forwarding
- `warning` notification forwarding
- `configWarning` notification forwarding
- `deprecationNotice` notification forwarding
- `skills/changed` notification forwarding
- `item/commandExecution/requestApproval` server-request round trip
- `item/fileChange/requestApproval` server-request round trip
- `item/permissions/requestApproval` server-request round trip
- `item/tool/requestUserInput` server-request round trip
- `mcpServer/elicitation/request` server-request round trip
- `account/chatgptAuthTokens/refresh` server-request round trip

This is still narrower than exhaustive TUI parity because some lower-frequency
notifications, reconnect paths, and operational edge cases still rely more on
targeted gateway regressions than on one broad end-to-end client harness.

Separate from the current TUI-focused matrix, dedicated northbound passthrough
coverage now also exercises several lower-frequency app-server methods outside
that hot path: `thread/archive`, `thread/unarchive`,
`thread/metadata/update`, `thread/turns/list`,
`thread/realtime/listVoices`, `thread/increment_elicitation`,
`thread/decrement_elicitation`, and `thread/inject_items`.

Dedicated northbound passthrough coverage now also exercises several
supporting/configuration methods that some clients and test tools use outside
the main TUI flow: `config/read`, `configRequirements/read`,
`experimentalFeature/list`, and `collaborationMode/list`.

Dedicated northbound passthrough coverage now also exercises current TUI
bootstrap/setup discovery requests outside the main account/model hot path:
`externalAgentConfig/detect`, `app/list`, `skills/list`,
`mcpServerStatus/list`, `mcpServer/oauth/login`, and `plugin/list`.

Dedicated northbound passthrough coverage now also exercises current TUI
setup-mutation and plugin-management requests:
`externalAgentConfig/import`, `plugin/read`, `plugin/install`,
`plugin/uninstall`, `config/batchWrite`, `memory/reset`, and
`account/logout`.

Dedicated northbound passthrough coverage now also exercises the core
thread/turn transport requests current TUI sessions rely on:
`thread/start`, `thread/resume`, `thread/fork`, `thread/list`,
`thread/loaded/list`, `thread/read`, `thread/name/set`,
`thread/memoryMode/set`, and `turn/start`.

Real northbound `RemoteAppServerClient` coverage now also exercises those same
supporting/configuration methods in embedded and single-worker remote mode, and
multi-worker remote mode now also routes threadless `config/read` by matching
`cwd` while keeping `configRequirements/read` on the primary worker, plus
aggregated capability discovery for `experimentalFeature/list` and
`collaborationMode/list`, so they are no longer validated only through
targeted passthrough fixtures.
That same real embedded coverage now also includes `account/rateLimits/read`,
so the client's background rate-limit refresh path is exercised through the
in-process gateway transport instead of only through targeted passthrough
fixtures.

Real northbound multi-worker coverage now also exercises the primary-worker
onboarding and feedback flows `account/login/start`,
`account/login/cancel`, `account/login/completed`, and `feedback/upload`.

Multi-worker hardening coverage notes:

- dedicated northbound multi-worker tests now verify exact-duplicate
  connection-state notifications such as `account/updated` are emitted only
  once on the shared northbound session
- dedicated northbound multi-worker tests now also verify that a later
  `mcpServer/oauth/login` reconnects a recovered worker before falling
  through to worker-discovery routing, instead of staying pinned to the
  primary worker
- the real multi-worker remote connection-state harness now also includes
  `account/login/completed`, `warning`, `configWarning`, and
  `deprecationNotice`, so shared-session dedupe is covered for onboarding
  completion and other visible connection-state notifications in addition to
  the existing account, rate-limit, connector, and MCP startup state
  notifications
- the real multi-worker remote setup-mutation harness now also includes
  `externalAgentConfig/import/completed` in both steady state and
  reconnect-after-recovery flows, so fanout import completion notifications
  are covered by shared-session dedupe regressions as well
- dedicated northbound multi-worker tests now also verify duplicate
  `skills/changed` invalidations are suppressed until the client refreshes with
  `skills/list`, and that one fresh invalidation is emitted after that refresh

Embedded-mode coverage notes:

- embedded gateway tests now include a real `RemoteAppServerClient` harness
  against the gateway's northbound v2 WebSocket transport, covering bootstrap
  requests plus `thread/start`, `thread/list`, `thread/loaded/list`,
  `thread/read`, `turn/start`, and
  `thread/started` notification delivery
- that embedded harness also covers the core turn-lifecycle notifications
  `thread/status/changed`, `turn/started`, `item/started`,
  `item/agentMessage/delta`, `item/completed`, and `turn/completed`
- that same embedded harness now also covers streamed reasoning notifications
  for `item/reasoning/summaryTextDelta` and `item/reasoning/textDelta`
- that same embedded harness now also covers the experimental
  `item/tool/call` round trip, including forwarded `serverRequest/resolved`
  plus `item/started` / `item/completed` for the dynamic-tool call lifecycle
  through `turn/completed`
- that embedded harness now also covers `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that embedded harness now also covers a real
  `item/tool/requestUserInput` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that embedded harness now also covers real
  `item/commandExecution/requestApproval` and
  `item/fileChange/requestApproval` round trips, including the forwarded
  `serverRequest/resolved` notification ordering before `turn/completed`
- that embedded harness now also covers a real
  `item/permissions/requestApproval` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that embedded harness now also covers a real
  `mcpServer/elicitation/request` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that same embedded harness now also covers `feedback/upload`, validating
  operator-facing feedback submission through the real northbound client
  transport instead of only through targeted passthrough fixtures
- that same embedded harness now also covers standalone command execution for
  `command/exec`, `command/exec/outputDelta`, `command/exec/write`,
  `command/exec/resize`, and `command/exec/terminate`
- that embedded harness now also covers lower-frequency thread-control flows
  for `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- that same embedded harness now also covers turn-control parity for
  `turn/steer` and `turn/interrupt`, including interrupted-turn completion
  through the real northbound client transport
- embedded gateway tests now cover `thread/list`, `thread/name/set`,
  `thread/memoryMode/set`, and `thread/loaded/list`
- embedded gateway tests also verify the current app-server behavior where
  `thread/resume` and `thread/fork` return `no rollout found for thread id ...`
  until the source thread has materialized rollout storage

Single-worker remote coverage notes:

- single-worker remote gateway tests now include a real
  `RemoteAppServerClient` harness against the gateway's northbound v2
  WebSocket transport, covering bootstrap requests plus `thread/start`,
  `thread/list`, `thread/loaded/list`, `thread/read`, `thread/name/set`,
  `thread/memoryMode/set`, `turn/start`, and `thread/started` notification
  delivery
- that single-worker remote harness now also covers
  `externalAgentConfig/detect`, `externalAgentConfig/import`,
  `account/rateLimits/read`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that same single-worker remote harness now also covers onboarding auth and
  feedback flows for `account/login/start`, `account/login/cancel`,
  `account/login/completed`, and `feedback/upload`
- that single-worker remote harness now also covers a real
  `item/tool/requestUserInput` server-request round trip over the gateway's
  northbound v2 transport
- that single-worker remote harness now also covers real
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`, and
  `account/chatgptAuthTokens/refresh` round trips over the same northbound v2
  transport
- that same single-worker remote harness now also covers the
  connection-scoped notifications `account/updated`,
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, `mcpServer/startupStatus/updated`, and
  `skills/changed`
- that same single-worker remote harness now also verifies thread re-entry
  from a later northbound v2 client session, covering `thread/read` after the
  original client disconnects
- that same real embedded harness now also verifies thread re-entry from a
  later northbound v2 client session, covering `thread/resume` plus
  follow-up `thread/read` after the original client disconnects
- that same single-worker remote harness now also covers the dynamic-tool
  `item/tool/call` server-request round trip, including forwarded
  `serverRequest/resolved` ordering plus `item/started` / `item/completed`
  before `turn/completed`
- that same single-worker remote harness now also covers standalone command
  execution for `command/exec`, `command/exec/outputDelta`,
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`
- that real embedded compatibility harness now also covers realtime workflow
  parity for `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`, and
  `thread/realtime/listVoices`, validating that the in-process gateway reaches
  the realtime sideband transport for start and append requests
- that single-worker remote harness also covers the core turn-lifecycle
  notifications `thread/status/changed`, `turn/started`,
  `item/started`, `item/agentMessage/delta`, `item/completed`, and
  `turn/completed`
- that same single-worker remote harness now also covers streamed reasoning
  notifications `item/reasoning/summaryTextDelta` and
  `item/reasoning/textDelta`, plus longer-running lifecycle notifications
  `hookStarted` and `hookCompleted`
- that single-worker remote harness now also covers realtime workflow parity
  for `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`, and
  `thread/realtime/listVoices`, plus delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still complete that realtime workflow after the downstream worker
  disconnects and the gateway restores the remote session
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still satisfy the broader bootstrap/setup discovery surface:
  `model/list`, `externalAgentConfig/detect`, threadless `app/list`,
  `skills/list`, threadless `plugin/list`, threadless `config/read`,
  `configRequirements/read`, `experimentalFeature/list`,
  `collaborationMode/list`, `mcpServerStatus/list`, and
  `mcpServer/oauth/login`
- that single-worker remote harness now also covers turn-control parity for
  `turn/steer` and `turn/interrupt`, plus the resulting
  `item/agentMessage/delta`, `turn/completed`, and
  `thread/status/changed` notifications
- that single-worker remote harness now also covers lower-frequency
  thread-control and review flows for `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still complete that lower-frequency thread-control path for
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- single-worker remote gateway tests also verify the current app-server
  behavior where `thread/resume` and `thread/fork` return
  `no rollout found for thread id ...` until the source thread has
  materialized rollout storage

Multi-worker remote coverage notes:

- multi-worker remote gateway tests now include a real
  `RemoteAppServerClient` harness against the gateway's northbound v2
  WebSocket transport, covering worker-affine `thread/start`, aggregated
  `thread/list` / `thread/loaded/list` / `thread/read`, and tenant/project
  scope filtering across worker-owned threads
- that multi-worker harness now also covers one shared bootstrap/setup session
  through aggregated `account/read`, `account/rateLimits/read`, `model/list`,
  `externalAgentConfig/detect`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, and worker-discovery `mcpServer/oauth/login`,
  instead of validating those setup paths only through separate targeted
  aggregation regressions
- that same multi-worker harness now also covers cwd-aware threadless
  `config/read`, primary-worker `configRequirements/read`, plus aggregated
  capability discovery for `experimentalFeature/list` and
  `collaborationMode/list`
- that same multi-worker harness now also covers the primary-worker
  onboarding and feedback flows for `account/login/start`,
  `account/login/cancel`, `account/login/completed`, and `feedback/upload`
- that same multi-worker harness now also covers translated per-worker
  `item/tool/call` round trips on one shared northbound session, including
  gateway-owned request-id translation for overlapping downstream request ids
- that same multi-worker harness now also covers exact-duplicate suppression
  for the connection-scoped notifications `account/updated`,
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, and `mcpServer/startupStatus/updated`, including after
  worker reconnect
- that multi-worker harness now also covers one shared setup-mutation session
  through `externalAgentConfig/import`, `config/batchWrite`,
  `config/value/write`, `memory/reset`, and `account/logout`, verifying that
  those connection-scoped mutations still fan out to both worker sessions
- that same multi-worker harness now also covers shared-session dedupe for
  `externalAgentConfig/import/completed` in both steady state and after worker
  reconnect
- that multi-worker harness now also covers lower-frequency thread-control and
  review routing for `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- dedicated northbound websocket reconnect coverage now also pins that same
  lower-frequency sticky thread-control set on one shared client session, so
  later requests for those worker-owned threads re-add the recovered worker
  before routing instead of falling through to the surviving worker
- that multi-worker harness now also covers sticky realtime request routing for
  `thread/realtime/start`, `thread/realtime/appendText`, and
  `thread/realtime/stop`, plus shared-session fan-in for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, and `thread/realtime/closed`
- that same multi-worker harness now also covers `thread/realtime/appendAudio`
  plus shared-session fan-in for `thread/realtime/outputAudio/delta`,
  `thread/realtime/sdp`, and `thread/realtime/error`
- that same multi-worker harness now also covers connection-scoped realtime
  voice discovery for `thread/realtime/listVoices`, aggregating distinct voice
  inventory across workers while keeping the primary worker defaults stable
- that same multi-worker harness now also covers the primary-worker
  standalone command-execution control plane for `command/exec`,
  `command/exec/outputDelta`, `command/exec/write`,
  `command/exec/resize`, and `command/exec/terminate`
- that same multi-worker reconnect harness now also verifies that realtime
  routing and fan-in still work after one worker disconnects and recovers,
  including aggregated `thread/realtime/listVoices` plus the recovered worker
  and a second healthy worker on one shared v2 client session
- that multi-worker harness now also covers sticky `turn/steer` and
  `turn/interrupt` routing on worker-owned threads, plus shared-session fan-in
  for the resulting `thread/status/changed`, `turn/started`,
  `item/started`, `item/agentMessage/delta`,
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, `item/fileChange/outputDelta`,
  `hookStarted`, `hookCompleted`, `item/completed`, and
  `turn/completed` notifications
- that multi-worker harness now also verifies that same-scope clients can
  re-enter threads created by another northbound session, while other
  tenant/project headers receive filtered lists and `thread not found` for
  hidden reads
- northbound multi-worker transport now also guards consumed
  `serverRequest/resolved` routes, dropping duplicate downstream replays
  instead of forwarding stale worker-local request ids to the client
- that replay-drop path now also emits a structured warning log with the
  scope, worker id, replayed downstream request id, and any still-buffered
  translated routes
- duplicate `skills/changed` suppression and exact-duplicate connection-state
  notification suppression now also emit structured warning logs with scope,
  worker id, method, and params when those notifications are dropped
- dedicated northbound multi-worker regression coverage now also verifies
  those duplicate downstream `serverRequest/resolved` replays are dropped while
  the shared session remains usable for follow-up requests
- that same multi-worker harness now also covers sticky `thread/resume` and
  `thread/fork` routing on worker-owned threads, plus follow-up `thread/read`
  on the returned forked thread
