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

- supported topologies: embedded runtime and remote runtime with exactly one
  downstream worker
- transport model: one northbound v2 connection maps to one downstream
  app-server session
- multi-worker remote runtime is explicitly out of scope for this matrix

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
| `model/list` | Bootstrap model picker/default model discovery | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests. |
| `externalAgentConfig/detect` | Import existing external agent config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `externalAgentConfig/import` | Import external agent config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `skills/list` | Skills picker / refresh | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `config/batchWrite` | Reload user config | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `account/logout` | Sign-out flow | `transparent passthrough` | Covered by single-worker remote compatibility tests. |
| `memory/reset` | Reset memory mode state | `transparent passthrough` | Covered by single-worker remote compatibility tests. |

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
- `thread/realtime/started` is covered by a dedicated gateway compatibility
  test for experimental realtime notification forwarding
- broader notification coverage is still deferred to parity validation and
  hardening work, especially additional item-delta variants and realtime
  notifications

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

## Topology and Policy Gaps

These are the known gaps that remain after the current Stage A coverage work.

| Area | Status | Notes |
| --- | --- | --- |
| remote runtime with more than one worker | `deferred` | The gateway currently returns `501 Not Implemented` for northbound v2 connections in multi-worker remote mode. |
| v2 scope enforcement | `partial` | Embedded and single-worker remote v2 connections now derive tenant/project scope from the WebSocket upgrade headers, enforce thread visibility on thread-scoped requests, filter `thread/list` plus `thread/loaded/list`, reject downstream server requests for hidden threads, and register resumed/forked thread IDs back into scope ownership. Resume/fork flows that bypass `threadId` are rejected for now, and those resume/fork plus list-filtering behaviors are covered by dedicated northbound JSON-RPC regression tests. |
| v2 admission/rate limiting | `complete` | The gateway now applies the same per-scope request rate limit and turn-start quota policy to v2 JSON-RPC requests that it already applies to HTTP routes. |
| v2 per-message audit/metrics | `complete` | The gateway now emits per-request v2 metrics and audit logs for `initialize` and subsequent client JSON-RPC requests, tagged by method and outcome. |

## Required Test Subset

The current gateway compatibility tests cover this required subset:

- `initialize`
- `account/read`
- `model/list`
- `externalAgentConfig/detect`
- `externalAgentConfig/import`
- `skills/list`
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

Embedded-mode coverage notes:

- embedded gateway tests now include a real `RemoteAppServerClient` harness
  against the gateway's northbound v2 WebSocket transport, covering bootstrap
  requests plus `thread/start`, `thread/list`, `thread/loaded/list`,
  `thread/read`, and
  `thread/started` notification delivery
- that embedded harness now also covers `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `skills/list`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that embedded harness now also covers a real
  `item/tool/requestUserInput` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
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
