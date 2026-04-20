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
| `account/read` | Bootstrap auth/account state | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests. |
| `account/rateLimits/read` | Background rate-limit refresh for status surfaces | `transparent passthrough` | Covered by dedicated gateway passthrough tests; multi-worker remote mode now also aggregates the per-limit map across workers while preserving the primary historical view from the first worker. |
| `model/list` | Bootstrap model picker/default model discovery | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests. |
| `externalAgentConfig/detect` | Import existing external agent config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `externalAgentConfig/import` | Import external agent config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `app/list` | Connector/app discovery and refresh | `transparent passthrough` | Multi-worker remote mode now aggregates the current threadless `app/list` path across workers, including gateway-owned pagination over the merged result set. |
| `mcpServerStatus/list` | MCP inventory/bootstrap refresh | `transparent passthrough` | Covered by dedicated gateway passthrough tests; multi-worker remote mode now also aggregates connection-scoped MCP server inventory across workers with gateway-owned pagination. |
| `skills/list` | Skills picker / refresh | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `plugin/list` | Plugin catalog discovery | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. |
| `plugin/read` | Plugin detail view | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. |
| `config/value/write` | Plugin enablement and other targeted config updates | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests; multi-worker remote mode now fans this connection-scoped mutation out across workers so plugin/config state does not drift. |
| `config/batchWrite` | Reload user config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `account/logout` | Sign-out flow | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `memory/reset` | Reset memory mode state | `transparent passthrough` | Covered by single-worker remote compatibility tests. |

## Plugin Management Requests

These are the plugin-management requests the current TUI issues after bootstrap.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `plugin/install` | Install a selected plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. |
| `plugin/uninstall` | Remove an installed plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. |

## Thread and Turn Requests

These methods are used by the current TUI for normal thread lifecycle and turn
control.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/start` | Start a new session | `transparent passthrough` | Covered by embedded and single-worker remote gateway tests. |
| `thread/resume` | Resume an existing session | `policy interception` | `threadId`-based resume is covered by single-worker remote compatibility tests, but gateway scope policy rejects `history`- or `path`-based resume because those forms bypass thread ownership checks. |
| `thread/fork` | Fork an existing session | `policy interception` | `threadId`-based fork is covered by single-worker remote compatibility tests, but gateway scope policy rejects `path`-based fork because it bypasses thread ownership checks. |
| `thread/list` | Session picker/history | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/loaded/list` | Discover currently loaded subagent threads | `transparent passthrough` | Covered by single-worker remote compatibility tests, including scope filtering. |
| `thread/read` | Read thread metadata/history | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/name/set` | Rename thread | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/memoryMode/set` | Update memory mode | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/unsubscribe` | Drop local subscription | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/compact/start` | Compact thread history | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/shellCommand` | Start shell command from thread context | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/backgroundTerminals/clean` | Clean background terminals | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `thread/rollback` | Roll back thread turns | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `review/start` | Start review flow | `transparent passthrough` | Covered by single-worker remote compatibility tests, including review-thread scope registration. |
| `turn/start` | Start a turn | `transparent passthrough` | Covered by single-worker remote compatibility tests, including turn lifecycle notifications. |
| `turn/interrupt` | Interrupt active turn | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `turn/steer` | Steer active turn | `transparent passthrough` | Covered by single-worker remote compatibility tests. |

## Realtime Requests

These methods are used by the TUI realtime conversation path.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/realtime/start` | Start realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. |
| `thread/realtime/appendAudio` | Stream microphone audio | `transparent passthrough` | Covered by dedicated gateway passthrough tests. |
| `thread/realtime/appendText` | Stream text into realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. |
| `thread/realtime/stop` | Stop realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. |

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
- `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta` are now covered
  by real single-worker remote and multi-worker remote turn-workflow
  compatibility tests
- `thread/realtime/started` is covered by a dedicated gateway compatibility
  test for experimental realtime notification forwarding
- additional experimental realtime notifications now also have dedicated
  gateway compatibility coverage for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- multi-worker northbound v2 regression coverage now also verifies fan-in for
  experimental realtime notifications on one shared client session, including
  cross-worker `thread/realtime/transcript/delta` and
  `thread/realtime/outputAudio/delta`
- lower-frequency user-visible notifications now also have dedicated gateway
  compatibility coverage for `warning`, `configWarning`,
  `deprecationNotice`, and `mcpServer/startupStatus/updated`
- broader notification coverage is still deferred to parity validation and
  hardening work, especially less common realtime notifications

## Server Requests

These are the server-initiated requests the current TUI explicitly handles.

| Method | Client handling status | Gateway status | Notes |
| --- | --- | --- | --- |
| `item/commandExecution/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests. |
| `item/fileChange/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests. |
| `item/permissions/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness. |
| `item/tool/requestUserInput` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests. |
| `mcpServer/elicitation/request` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness. |
| `item/tool/call` | TUI marks unsupported today | `transparent passthrough` | This is a client capability gap, not a gateway transport gap. |
| `account/chatgptAuthTokens/refresh` | TUI accepts | `transparent passthrough` | Covered by single-worker remote round-trip tests. |
| legacy `apply_patch` / `exec_command` approvals | TUI marks unsupported today | `transparent passthrough` | These are not part of the primary Stage A drop-in target. |

Additional transport behavior:

- if a northbound v2 connection ends while a forwarded server request is still
  pending, including client disconnects and malformed-payload protocol closes,
  plus other terminal northbound I/O failures such as client-send timeouts or
  broken pipes,
  the gateway now rejects that pending request back to the downstream
  app-server session with a gateway-owned internal error instead of leaving it
  unresolved
- if one downstream worker disconnects during a multi-worker session while a
  thread-scoped server request from that worker is still pending or waiting on
  downstream `serverRequest/resolved`, the gateway now synthesizes
  `serverRequest/resolved` northbound so the client can dismiss that prompt
  instead of leaving it stranded on the shared session

## Topology and Policy Gaps

These are the known gaps that remain after the current Stage A coverage work.

| Area | Status | Notes |
| --- | --- | --- |
| remote runtime with more than one worker | `partial` | Northbound v2 WebSocket upgrades are now admitted in multi-worker remote mode, with one downstream session per worker plus aggregated `thread/list` / `thread/loaded/list`, aggregated bootstrap/setup discovery for `account/read`, `model/list`, `externalAgentConfig/detect`, `app/list`, `skills/list`, and threadless `plugin/list`, translated server-request IDs across workers for `item/tool/requestUserInput`, `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `item/permissions/requestApproval`, `mcpServer/elicitation/request`, and `account/chatgptAuthTokens/refresh`, sticky thread routing, worker-discovery routing for `plugin/read` / `plugin/install` / `plugin/uninstall`, fanout for connection-scoped setup mutations such as `externalAgentConfig/import`, `config/batchWrite`, `memory/reset`, and `account/logout`, deduplicated `skills/changed` invalidation notifications until the client refreshes with `skills/list`, exact-duplicate suppression for connection-state notifications such as `account/updated`, `account/rateLimits/updated`, and `app/list/updated`, synthesized `serverRequest/resolved` for thread-scoped prompts stranded by a worker disconnect, route backfill for already visible threads discovered through aggregated `thread/list` / `thread/loaded/list`, and in-session survival when one downstream worker disconnects and the remaining workers can still serve the shared northbound connection. Broader parity and hardening work are still pending before this matches embedded or single-worker remote support. |
| v2 scope enforcement | `partial` | Embedded, single-worker remote, and multi-worker remote v2 connections now derive tenant/project scope from the WebSocket upgrade headers, enforce thread visibility on thread-scoped requests, filter `thread/list` plus `thread/loaded/list`, reject downstream server requests for hidden threads, backfill worker ownership for already visible threads discovered through aggregated list responses, and register resumed/forked thread IDs back into scope ownership. Multi-worker remote runtime now also has a real northbound `RemoteAppServerClient` regression covering same-scope re-entry plus cross-scope filtering for aggregated `thread/list` / `thread/loaded/list` / `thread/read`. Resume/fork flows that bypass `threadId` are rejected for now, and those resume/fork plus list-filtering behaviors are covered by dedicated northbound JSON-RPC regression tests. |
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
- `skills/list`
- `plugin/list`
- `plugin/read`
- `config/value/write`
- `plugin/install`
- `plugin/uninstall`
- `config/batchWrite`
- `memory/reset`
- `account/logout`
- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/list`
- `thread/loaded/list`
- `thread/read`
- `thread/name/set`
- `thread/memoryMode/set`
- `thread/unsubscribe`
- `thread/compact/start`
- `thread/shellCommand`
- `thread/backgroundTerminals/clean`
- `thread/rollback`
- `review/start`
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
- `item/agentMessage/delta` notification forwarding
- `item/reasoning/summaryTextDelta` notification forwarding
- `item/reasoning/textDelta` notification forwarding
- `item/commandExecution/outputDelta` notification forwarding
- `item/fileChange/outputDelta` notification forwarding
- `thread/realtime/started` notification forwarding
- `item/commandExecution/requestApproval` server-request round trip
- `item/fileChange/requestApproval` server-request round trip
- `item/permissions/requestApproval` server-request round trip
- `item/tool/requestUserInput` server-request round trip
- `mcpServer/elicitation/request` server-request round trip
- `account/chatgptAuthTokens/refresh` server-request round trip

This is still narrower than the full TUI request surface because several
realtime flows, additional item-delta-heavy turn streams, and a few less common
passthrough methods still lack dedicated gateway parity tests.

Multi-worker hardening coverage notes:

- dedicated northbound multi-worker tests now verify exact-duplicate
  connection-state notifications such as `account/updated` are emitted only
  once on the shared northbound session
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
  `thread/status/changed`, `turn/started`, `item/agentMessage/delta`, and
  `turn/completed`
- that embedded harness now also covers `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `skills/list`, `config/batchWrite`,
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
  `externalAgentConfig/detect`, `externalAgentConfig/import`, `skills/list`,
  `config/batchWrite`, `memory/reset`, and `account/logout`
- that single-worker remote harness now also covers a real
  `item/tool/requestUserInput` server-request round trip over the gateway's
  northbound v2 transport
- that single-worker remote harness now also covers real
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`, and
  `account/chatgptAuthTokens/refresh` round trips over the same northbound v2
  transport
- that single-worker remote harness also covers the core turn-lifecycle
  notifications `thread/status/changed`, `turn/started`,
  `item/agentMessage/delta`, and `turn/completed`
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
- that multi-worker harness now also verifies that same-scope clients can
  re-enter threads created by another northbound session, while other
  tenant/project headers receive filtered lists and `thread not found` for
  hidden reads
