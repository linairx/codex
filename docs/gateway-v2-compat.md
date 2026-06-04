# Gateway App-Server v2 Compatibility Plan

## Goal

Make `codex-gateway` compatible with the app-server v2 northbound protocol so
existing Codex clients can connect to the gateway directly, without changing
their transport contract.

This is explicitly an additive track on top of the existing gateway HTTP/SSE
surface. The current HTTP API remains the platform-facing API. The new v2
surface is a compatibility API.

## Current State

Today `codex-gateway` exposes a narrowed HTTP/SSE API:

- `GET /healthz`
- `GET /v1/events`
- `GET /v1/threads`
- `POST /v1/threads`
- `GET /v1/threads/{thread_id}`
- `POST /v1/threads/{thread_id}/turns`
- `POST /v1/threads/{thread_id}/turns/{turn_id}/interrupt`
- `POST /v1/server-requests/respond`

This is sufficient for a custom external client, but it is not a drop-in
replacement for app-server because current Codex clients speak the app-server
v2 JSON-RPC protocol over WebSocket and rely on its request, notification,
response, and server-request semantics.

Implementation status as of this document revision:

- Milestone 1 is complete
- Milestone 2 is complete for the embedded topology
- Milestone 3 is complete for the single-remote-worker topology
- Milestone 4 is complete

## Recommendation

Use a transparent v2 transport proxy architecture, not a gateway-specific
reimplementation of the full app-server API.

The gateway should:

- terminate northbound v2 WebSocket client connections
- authenticate and scope them
- create or attach to downstream app-server sessions
- proxy JSON-RPC requests, notifications, responses, and server requests
- selectively intercept only the parts needed for policy, routing, auditing,
  observability, and compatibility

The gateway should not:

- duplicate app-server request handling logic method by method where a transport
  proxy can preserve semantics
- fork v2 payload shapes into gateway-specific equivalents
- try to centralize all execution logic in the gateway process

## Why This Route

This route minimizes semantic drift:

- `codex-app-server-protocol` already defines the wire contract
- `codex-app-server-client` already implements downstream WebSocket session
  management, initialize/initialized flow, and server-request response handling
- the gateway can stay focused on boundary concerns: auth, tenancy, quotas,
  auditing, routing, health, and worker selection

It also creates a cleaner long-term split:

`Codex client -> codex-gateway -> codex-app-server -> core`

instead of:

`Codex client -> codex-gateway (reimplements app-server) -> core`

## Main Constraints

### 1. v2 is much larger than the current HTTP surface

The v2 protocol covers far more than the current gateway HTTP API. It includes:

- initialize and connection lifecycle
- thread management and metadata mutation
- turn start, steer, interrupt, review, compact, rollback
- filesystem APIs
- command execution control plane
- MCP tools, resources, OAuth, and status
- plugin and marketplace APIs
- config APIs
- realtime APIs
- server requests and server notifications

A manual adapter layer for the whole surface would be high-churn and fragile.

### 2. Multi-worker routing is harder for v2 than for HTTP

The current HTTP gateway can choose a worker per request and maintain sticky
thread routing in a registry.

For v2 WebSocket compatibility, one northbound client connection may issue:

- connection-scoped requests such as `initialize`
- thread-scoped requests for many thread IDs
- server-request responses that must go back to the original worker
- notifications and streaming events that may arrive asynchronously

This means the gateway needs an explicit routing model for multiplexing a
single northbound connection over one or more downstream worker sessions.

### 3. Connection semantics matter

Existing Codex clients expect:

- initialize/initialized handshake ordering
- request/response correlation
- delivery of async notifications and server requests on the same session
- stable error shapes

Compatibility work must preserve those semantics, not just the payload schema.

## Compatibility Strategy

### Stage A: Embedded and Single-Worker Compatibility First

First ship a mode where one northbound v2 connection maps to exactly one
downstream app-server session.

Supported topologies:

- embedded mode
- remote mode with exactly one remote worker

This is the fastest route to "an existing Codex client can point at the
gateway".

Properties:

- minimal semantic translation
- no cross-worker fanout yet
- easier debugging and lower compatibility risk

Non-goal for Stage A:

- aggregated multi-worker thread visibility on one v2 connection

### Stage B: Coordinated Multi-Worker v2 Routing

Once Stage A is stable, add gateway-side coordination so one northbound v2
connection can work across multiple downstream workers.

This likely requires:

- a per-connection router object inside the gateway
- a downstream session per selected worker
- a mapping for thread ID -> worker ID
- a mapping for request ID -> downstream worker/session
- a mapping for server request ID -> downstream worker/session
- filtered fan-in of notifications back to the northbound connection

This is the point where the gateway stops being a pure 1:1 proxy and becomes a
protocol-preserving connection coordinator.

## Proposed Architecture

Add a new northbound transport module, for example:

```text
codex-rs/gateway/src/northbound/
  mod.rs
  http.rs
  v2_websocket.rs
  v2_connection.rs
  v2_router.rs
```

Suggested responsibilities:

- `v2_websocket.rs`
  Accept northbound WebSocket upgrades and own socket read/write loops.
- `v2_connection.rs`
  Maintain per-client session state, request correlation, auth context, and
  shutdown behavior.
- `v2_router.rs`
  Decide which downstream app-server session owns each request and how
  notifications/server requests are forwarded back.

Reuse existing pieces:

- `codex-app-server-protocol` for wire types and method names
- `codex-app-server-client` for downstream sessions
- existing gateway auth, scope, audit, observability, admission, and remote
  health modules

## Policy Interception Points

The gateway should intercept as little as possible, but it still needs explicit
policy hooks.

### Intercept on initialize

Use initialize to:

- authenticate the northbound client
- stamp gateway session metadata
- apply capability masks if needed
- bind connection context for later audit and scope checks

### Intercept on thread creation and thread discovery

Use thread lifecycle events to:

- record `thread_id -> tenant/project`
- record `thread_id -> worker`
- prevent cross-scope visibility on later reads, turns, and notifications

### Intercept on server requests

Use server requests to:

- preserve approval and user-input flows
- correlate northbound client responses back to the original downstream worker
- reject or shape unsupported requests in a controlled way during staged rollout

### Intercept on admission

Rate limits and quotas should apply to v2 too, especially:

- thread creation
- turn start / turn steer
- command execution and other expensive control-plane requests when needed

## Roadmap

### Milestone 1: v2 WebSocket Skeleton

Status: complete

Deliverables:

- add a northbound WebSocket endpoint for app-server v2 traffic
- parse and emit JSON-RPC messages with the existing protocol types
- support `initialize` / `initialized`
- create one downstream app-server session per northbound connection
- proxy requests, notifications, responses, and server requests end to end in
  embedded mode

Exit criteria:

- a test client can complete initialize and issue a basic typed request through
  the gateway

### Milestone 2: Drop-In Client Parity in Embedded Mode

Status: complete

Deliverables:

- validate normal Codex client boot and thread workflow through the gateway
- preserve server notifications and server requests on the same northbound
  connection
- add gateway audit and metrics coverage for v2 traffic
- apply auth and scope enforcement on the v2 transport

Exit criteria:

- an unmodified Codex client can target an embedded gateway and complete a
  normal session

Recent progress:

- the gateway now has a real embedded-mode compatibility harness that connects
  through `codex_app_server_client::RemoteAppServerClient`, proving the
  existing app-server v2 transport can bootstrap against `codex-gateway`
  without a client-side transport fork
- that embedded harness covers `account/read`, `model/list`, `thread/start`,
  `thread/list`, `thread/name/set`, `thread/memoryMode/set`,
  `thread/loaded/list`, and `thread/read`, and verifies `thread/started`
  notifications still arrive on the same northbound v2 connection
- that embedded harness also covers `turn/start` plus the core turn-lifecycle
  notifications current clients rely on during normal execution:
  `thread/status/changed`, `turn/started`, `item/started`,
  `item/agentMessage/delta`, `item/completed`, and `turn/completed`
- that same embedded harness now also covers the streamed reasoning
  notifications current clients consume during active turns:
  `item/reasoning/summaryTextDelta` and `item/reasoning/textDelta`
- the embedded harness now also verifies app-server's current unmaterialized
  thread semantics, preserving the expected `no rollout found for thread id ...`
  errors for `thread/resume` and `thread/fork` over the gateway's northbound v2
  transport
- that embedded harness now also covers real
  `item/commandExecution/requestApproval` and
  `item/fileChange/requestApproval` round trips, including the forwarded
  `serverRequest/resolved` notification ordering before `turn/completed`

### Milestone 3: Single Remote Worker Compatibility

Status: complete

Deliverables:

- support v2 passthrough when `gateway` is configured with exactly one remote
  worker
- preserve current worker health and reconnect behavior
- make startup and health output explicit about single-worker compatibility mode

Exit criteria:

- an unmodified Codex client can target a remote gateway backed by one
  app-server worker

Recent progress:

- the gateway now has a real single-worker-remote compatibility harness that
  connects through `codex_app_server_client::RemoteAppServerClient`, proving the
  northbound v2 transport can bootstrap against `codex-gateway` even when the
  gateway itself is proxying to a downstream remote app-server worker
- that remote harness covers `account/read`, `model/list`, `thread/start`,
  `thread/list`, `thread/name/set`, `thread/memoryMode/set`,
  `thread/loaded/list`, and `thread/read`, and verifies `thread/started`
  notifications still arrive on the same northbound v2 connection
- the remote harness now also verifies app-server's current unmaterialized
  thread semantics, preserving the expected `no rollout found for thread id ...`
  errors for `thread/resume` and `thread/fork` over the gateway's northbound v2
  transport
- that remote harness now also covers `turn/start` plus the core turn-lifecycle
  notifications current clients rely on during normal execution:
  `thread/status/changed`, `turn/started`, `item/started`,
  `item/agentMessage/delta`, `item/completed`, and `turn/completed`
- that same remote harness now also covers additional streamed turn
  notifications the current TUI consumes during active runs:
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and `item/fileChange/outputDelta`
- that same remote harness now also covers longer-running turn lifecycle
  notifications the current TUI uses to render item and hook progress,
  including `item/started`, `item/completed`, `hook/started`, and
  `hook/completed`
- dedicated northbound gateway coverage now also exercises item lifecycle
  notifications the TUI uses for longer-running turn state, covering
  `item/started` and `item/completed`
- dedicated northbound gateway coverage now also exercises hook lifecycle
  notifications the TUI renders for hook progress and completion, covering
  `hook/started` and `hook/completed`
- dedicated northbound gateway coverage now also exercises guardian review
  lifecycle notifications for approval auto-review UX, covering
  `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed`
- dedicated northbound gateway coverage now also exercises richer turn
  notifications that the TUI consumes opportunistically, including
  `error`, `plan/delta`, `reasoning/summaryTextDelta`,
  `reasoning/summaryPartAdded`, `reasoning/textDelta`,
  `terminalInteraction`, `commandExecution/outputDelta`,
  `fileChange/outputDelta`, `turn/diffUpdated`, `turn/planUpdated`,
  `thread/tokenUsage/updated`, `mcpToolCall/progress`, `contextCompacted`,
  and `model/rerouted`
- that same dedicated northbound connection-notification coverage now also
  includes `account/login/completed` for onboarding auth completion flows
- that same dedicated northbound connection-notification coverage now also
  includes `externalAgentConfig/import/completed`, so fanout imports do not
  surface duplicate completion notifications on one shared multi-worker
  session
- the real single-worker remote compatibility harness now also exercises
  `account/login/completed` during an end-to-end onboarding auth flow
- dedicated northbound v2 passthrough coverage now also verifies the
  unsupported-but-transparent `item/tool/call` server-request round trip, so
  gateway transport behavior is covered even though the current TUI still does
  not handle that request family
- the real embedded compatibility harness now also exercises that same
  unsupported-but-transparent `item/tool/call` round trip, so the in-process
  northbound client transport is covered beyond dedicated passthrough fixtures
- that same real embedded compatibility harness now also verifies
  `serverRequest/resolved` plus `item/started` / `item/completed` forwarding
  for the dynamic-tool call lifecycle through turn completion
- the real single-worker remote compatibility harness now also exercises that
  same unsupported-but-transparent `item/tool/call` round trip, so the
  northbound client transport is covered beyond dedicated passthrough fixtures
- that same real single-worker remote compatibility harness now also verifies
  `serverRequest/resolved` plus `item/started` / `item/completed` ordering for
  the dynamic-tool call lifecycle before turn completion
- the real single-worker remote compatibility harness now also covers the
  standalone `command/exec` control plane, validating `command/exec`,
  `command/exec/outputDelta`, `command/exec/write`, `command/exec/resize`,
  and `command/exec/terminate` through the gateway-backed remote worker
  session instead of only through targeted passthrough fixtures
- the real multi-worker remote compatibility harness now also exercises
  `item/tool/call`, verifying per-worker request-id translation and one shared
  northbound session round trip across multiple downstream workers

### Milestone 4: Coverage Audit and Method Gaps

Status: complete

Deliverables:

- inventory the v2 methods that Codex clients actually use today
- mark methods as:
  - transparent passthrough
  - passthrough with policy interception
  - unsupported in first rollout
- add compatibility tests for the required subset

Exit criteria:

- there is a documented required-method matrix for drop-in compatibility

Current artifact:

- see [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)

Recent progress:

- the gateway now emits per-request v2 metrics and audit logs for northbound
  JSON-RPC traffic, including `initialize`
- the gateway now also emits per-connection v2 metrics and audit logs for
  northbound WebSocket session outcomes, including initialize timeouts,
  downstream disconnects, protocol violations, and normal client closes
- v2 connection metrics, audit logs, and operator-facing completion logs now
  include the terminal pending and answered-but-unresolved server-request
  counts, and audit / completion logs also include terminal detail, so
  stranded-session debugging can start from the normal connection outcome
  record instead of requiring a separate `/healthz` sample
- v2 server-request saturation and scope-policy rejection now also emit a
  dedicated rejection counter tagged by server-request method and reason
  (`pending_limit` or `hidden_thread`), so rollout dashboards can distinguish
  prompt-state pressure and hidden-thread prompt drops from ordinary request
  failures or connection outcomes; the matching rejection logs now also
  include the affected worker websocket URL
- multi-worker v2 reconnect activity now also emits a
  `gateway_v2_worker_reconnects` counter tagged by worker id and outcome, so
  operators can alert on repeated reconnect attempts, failures, replay
  failures, and retry-backoff suppression without scraping structured logs
- those reconnect outcome counters now also have direct reconnect-path
  regression coverage for successful reconnects, connection failures, replay
  failures, and retry-backoff suppression, so the metric contract is pinned at
  the transport boundary rather than only at the observability helper
- multi-worker v2 fail-closed requests now also emit a
  `gateway_v2_fail_closed_requests` counter tagged by method and
  `reconnect_backoff_active`, so dashboards can separate deliberate degraded
  topology protection from ordinary upstream request failures
- ordinary multi-worker upstream request failures observed while worker routes
  are unavailable now emit `gateway_v2_upstream_request_failures` with the
  same method and reconnect-backoff tags, so those non-policy failures remain
  directly measurable
- dedicated routing-boundary regression coverage now verifies that the same
  upstream-failure metric is emitted once from `handle_client_request` when a
  surviving downstream session fails while another required worker route is
  unavailable, without double-counting the outer WebSocket error handling path
- suppressed multi-worker connection notifications now also emit
  `gateway_v2_suppressed_notifications` with notification method and reason
  tags; current reasons are `duplicate` for exact-duplicate connection-state
  notifications and `pending_refresh` for repeated `skills/changed`
  invalidations before the client refreshes `skills/list`, plus
  `hidden_thread` when a thread-scoped downstream notification is outside the
  current gateway request scope. Hidden-thread notification suppression logs
  include tenant/project scope, affected worker websocket URL, notification
  method, hidden thread id, and payload context so operators can identify the
  downstream session that emitted the hidden event.
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  gateway request-id translation now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_resolved_replay` and `method=serverRequest/resolved`, so
  server-request replay anomalies are visible in metrics alongside the
  structured warning logs. Those warning logs include the source worker
  websocket URL.
- downstream server requests that reuse a still-pending gateway request id now
  also emit `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` and the colliding server-request method
  before the gateway fails the session closed, so request-id collision
  anomalies are visible in metrics alongside the close outcome and structured
  warning log. The warning log includes the source worker websocket URL.
- worker-loss cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` for synthetic thread-scoped
  `serverRequest/resolved` notifications and stranded connection-scoped server
  requests, tagged by the original server-request method, so
  disconnect-driven prompt cleanup is visible in metrics alongside the
  structured cleanup logs and can be split by prompt family
- those worker-loss cleanup lifecycle counters are recorded before any
  synthetic `serverRequest/resolved` notification is sent to the northbound
  client, so slow-client send timeouts do not hide the route cleanup event from
  metrics
- client-side connection cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` for rejected thread-scoped
  pending prompts, rejected connection-scoped pending prompts, and
  answered-but-unresolved prompts left behind by client disconnect,
  protocol-violation, or slow-send teardown paths
- successful delivery of client-side pending-prompt cleanup rejections now also
  emits `event=client_cleanup_rejection_delivered`, complementing the existing
  failure and skipped-route cleanup delivery counters
- the ordinary northbound WebSocket client-disconnect path now has direct
  regression coverage for that successful cleanup-delivery counter, in
  addition to the slow-client timeout path
- client replies for unknown server-request ids now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response`, so protocol-violation
  prompt replies are visible in metrics before the gateway fails the v2
  session closed; the matching warning logs include pending
  gateway/downstream server-request ids, affected thread ids, worker ids, and
  whether the unexpected reply was a JSON-RPC `Response` or `Error`
- malformed or out-of-order v2 client traffic now also emits
  `gateway_v2_protocol_violations` with `phase` and `reason` tags, covering
  pre-initialize ordering errors, invalid JSON-RPC payloads, invalid UTF-8
  binary frames, and repeated `initialize` requests after the handshake; direct
  northbound websocket regressions pin those metric tags at the transport
  boundary, and repeated post-handshake `initialize` requests now also have
  matching request audit-log coverage with tenant/project scope
- pre-initialize malformed text-payload coverage now also pins the matching
  gateway v2 connection audit log with tenant/project scope, terminal detail,
  and `outcome="invalid_client_payload"`, so parse failures before
  `initialize` are visible through both metrics and audit logs
- pre-initialize invalid UTF-8 binary-frame coverage now also pins the matching
  gateway v2 connection audit log with tenant/project scope, terminal detail,
  and `outcome="invalid_client_payload"`, so malformed binary startup payloads
  are visible through both metrics and audit logs too
- post-initialize invalid UTF-8 binary-frame coverage now also pins the
  matching gateway v2 connection audit log with tenant/project scope, terminal
  detail, and `outcome="invalid_client_payload"`, so malformed binary payloads
  remain visible after a downstream app-server session has already been opened
- post-initialize malformed text-payload coverage now also pins the matching
  gateway v2 connection audit log with tenant/project scope, terminal detail,
  and `outcome="invalid_client_payload"`, so JSON-RPC parse failures remain
  visible after a downstream app-server session has already been opened
- downstream app-server event stream lag/backpressure now also emits
  `gateway_v2_downstream_backpressure_events` with the affected worker id
  before the gateway sends the close frame, so operator dashboards can
  distinguish downstream event loss from ordinary connection closes even when
  the client is too slow to receive the close frame cleanly
- slow-client northbound send timeouts now also emit
  `gateway_v2_client_send_timeouts`, so client backpressure can be tracked
  directly alongside the `client_send_timed_out` connection outcome and
  server-request cleanup lifecycle metrics
- multi-worker `thread/list` dedupe and visible-thread route recovery now also
  emit `gateway_v2_thread_list_deduplications` and
  `gateway_v2_thread_route_recoveries`, so duplicate thread snapshots and
  lazy route probe misses are visible in metrics alongside the existing
  structured routing logs; the aggregate `thread/list` dedupe regression now
  also asserts the selected-worker metric on the real route-selection path, and
  dedupe plus route-recovery success/miss logs include worker websocket URLs so
  operators can identify the affected
  downstream sessions directly
- lazy visible-thread route recovery regressions now also assert the
  `gateway_v2_thread_route_recoveries` success and miss metrics while pinning
  that ordinary probe success/miss outcomes do not emit fail-closed or upstream
  failure metrics
- the v2 transport now applies thread scope enforcement to downstream
  server-request forwarding, rejecting hidden-thread requests at the gateway
  boundary instead of leaking them to the northbound client
- embedded compatibility tests now cover additional Stage A thread control
  requests (`thread/list`, `thread/name/set`, `thread/memoryMode/set`, and
  `thread/loaded/list`)
- embedded compatibility tests now also verify that unmaterialized
  `thread/resume` and `thread/fork` requests preserve app-server's current
  `no rollout found for thread id ...` error semantics
- single-worker remote compatibility tests now also verify that unmaterialized
  `thread/resume` and `thread/fork` requests preserve app-server's current
  `no rollout found for thread id ...` error semantics
- single-worker remote compatibility tests now cover the remaining Stage A
  bootstrap passthrough methods, additional thread control passthrough methods,
  and the main approval / elicitation / token-refresh server-request round trips
- the real single-worker remote `RemoteAppServerClient` harness now also covers
  the bootstrap/setup methods current clients use beyond account/model
  discovery, including `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, `plugin/list`,
  `plugin/read`, `config/batchWrite`, `memory/reset`, `account/logout`, and
  `account/sendAddCreditsNudgeEmail`
- the real single-worker remote setup harness now also observes the
  `externalAgentConfig/import/completed` notification after
  `externalAgentConfig/import`, so import completion delivery is covered by the
  release-quality remote client path before multi-worker duplicate-suppression
  routing is involved
- that same real single-worker remote harness now also covers the
  supporting/configuration methods that some clients and test tools use
  outside the main TUI flow: `config/read`, `configRequirements/read`,
  `experimentalFeature/list`, and `collaborationMode/list`
- that same real single-worker remote harness now also covers low-frequency
  setup/config/MCP/account paths that previously relied on targeted
  passthrough fixtures: `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `mcpServer/resource/read`, `mcpServer/tool/call`, and
  `account/sendAddCreditsNudgeEmail`
- that same real single-worker reconnect harness now also verifies that
  `feedback/upload` still succeeds after worker recovery, so feedback
  submission remains part of the recovered single-worker release-gate path
- that same real single-worker remote harness now also covers plugin
  management flows current clients use after bootstrap, including
  `plugin/install` and `plugin/uninstall`, and verifies the resulting
  `plugin/list` installed-state transitions over the gateway transport
- that real single-worker remote harness now also covers a bidirectional
  server-request round trip for `item/tool/requestUserInput`, validating that
  the gateway forwards the downstream request to the client and proxies the
  typed response back to the same worker session
- that real single-worker remote harness now also covers additional typed
  server-request round trips for `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`,
  `mcpServer/elicitation/request`, and
  `account/chatgptAuthTokens/refresh`, so the real northbound client path now
  exercises multiple server-request payload families instead of only
  `requestUserInput`
- that same real single-worker reconnect harness now also re-exercises those
  additional typed server-request round trips after worker recovery, so the
  release-gate client path now verifies approval, elicitation, and token-
  refresh payload families in both steady state and recovered-session mode
- that same real single-worker remote harness now also covers the
  connection-scoped client notifications `account/updated`,
  `account/rateLimits/updated`, `app/list/updated`, and `skills/changed`, so
  ordinary refresh and invalidation flows are exercised through the same real
  northbound client path
- single-worker remote compatibility coverage now also verifies same-scope
  thread re-entry from a later northbound v2 client session, verifying that a
  second client can still `thread/read` a previously created thread after the
  original client disconnects
- that same real single-worker remote harness now also covers onboarding auth
  and feedback flows for `account/login/start`, `account/login/cancel`,
  `account/login/completed`, and `feedback/upload`, so those TUI-owned session
  setup paths are exercised through the drop-in client transport too
- dedicated northbound gateway passthrough coverage now also includes the
  current TUI's onboarding auth and feedback methods:
  `account/login/start`, `account/login/cancel`, and `feedback/upload`
- dedicated northbound gateway server-request round-trip coverage now also
  includes `account/chatgptAuthTokens/refresh`, so token-refresh prompts are
  validated directly at the gateway transport boundary in addition to the real
  single-worker remote client harness
- embedded mode still relies on that targeted northbound coverage for
  `account/chatgptAuthTokens/refresh` because the in-process downstream
  app-server client rejects ChatGPT token-refresh prompts before they can be
  observed through the real embedded compatibility harness
- that same dedicated northbound passthrough coverage now also includes
  lower-frequency app-server methods outside the current TUI hot path:
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/realtime/listVoices`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`, and
  `thread/inject_items`
- dedicated northbound v2 passthrough coverage now also includes
  supporting/configuration methods that some clients and test tools use outside
  the main TUI flow: `config/read`, `configRequirements/read`,
  `experimentalFeature/list`, and `collaborationMode/list`
- dedicated northbound v2 passthrough coverage now also includes the
  standalone `command/exec` control plane:
  `command/exec`, `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`, plus `command/exec/outputDelta` notification
  forwarding
- the real embedded `RemoteAppServerClient` harness now also covers the
  standalone `command/exec` control plane for in-process gateway sessions,
  validating `command/exec` plus `command/exec/outputDelta` through the
  drop-in client transport instead of only through targeted passthrough
  fixtures
- the real single-worker remote `RemoteAppServerClient` harness now also
  covers the standalone `command/exec` control plane, validating
  `command/exec`, `command/exec/write`, `command/exec/resize`,
  `command/exec/terminate`, and `command/exec/outputDelta` through the
  gateway-backed remote worker session instead of only through targeted
  passthrough fixtures
- dedicated northbound request-routing regression coverage now also verifies
  reconnect-before-routing for the primary-worker `command/exec` control plane,
  covering `command/exec`, `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate` so standalone command sessions recover the missing
  primary worker before multi-worker routing falls back
- the real embedded `RemoteAppServerClient` harness now also covers those same
  supporting/configuration methods, validating `config/read`,
  `configRequirements/read`, `experimentalFeature/list`, and
  `collaborationMode/list` through the in-process gateway transport instead of
  only through targeted passthrough fixtures
- that same real embedded compatibility harness now also verifies thread
  re-entry from a later northbound v2 client session, so a second client can
  still `thread/resume` and then `thread/read` a previously materialized
  thread after the original client disconnects
- that same real embedded compatibility harness now also covers
  `account/rateLimits/read`, so the client's background rate-limit refresh
  path is exercised through the in-process gateway transport instead of only
  through targeted passthrough fixtures
- the real embedded `RemoteAppServerClient` harness now also covers the
  external-auth onboarding path for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating the
  client-supplied `chatgptAuthTokens` flow plus the resulting
  `account/updated` notification through the in-process transport
- that same real embedded compatibility harness now also covers
  `feedback/upload`, so feedback submission is exercised through the
  in-process gateway transport instead of only through dedicated passthrough
  fixtures
- that same real embedded compatibility harness now also validates that the
  client-supplied `chatgptAuthTokens` are carried through to the downstream
  model request path after onboarding, instead of stopping at account-state
  notifications alone
- that same real embedded compatibility harness now also covers a plan-mode
  turn, verifying proposed-plan `item/started` and `item/completed`
  notifications through the in-process gateway transport with an unmodified
  `RemoteAppServerClient` session
- the real embedded turn workflow harness now also covers
  `item/reasoning/summaryPartAdded` from Responses summary-part events and
  `thread/tokenUsage/updated` after a token-bearing Responses completion, so
  those low-frequency notification forwarding paths are part of the
  in-process drop-in baseline
- the real embedded plan-mode harness now also covers `item/plan/delta`
  before the proposed-plan item completes, so streamed plan text is also part
  of the in-process drop-in baseline
- the real single-worker remote compatibility harness now also covers that same
  plan-mode turn path, verifying proposed-plan `item/started` and
  `item/completed` notifications through a gateway-backed remote worker session
  with an unmodified `RemoteAppServerClient`
- the real multi-worker remote compatibility harness now also covers that same
  plan-mode turn path on a worker-owned thread, verifying proposed-plan
  `item/started` and `item/completed` notification fan-in on one shared
  `RemoteAppServerClient` session
- the real single-worker remote `RemoteAppServerClient` harness now also covers
  the same external-auth onboarding path for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating that the
  gateway preserves the immediate `chatgptAuthTokens` login completion and the
  resulting `account/updated` notification through the remote transport
- that same real single-worker remote compatibility harness now also covers
  `account/rateLimits/read`, so the client's background rate-limit refresh
  path is exercised through the gateway-backed remote worker session instead of
  only through targeted passthrough fixtures or aggregated multi-worker tests
- that same real multi-worker remote compatibility harness now also covers
  `account/rateLimits/read`, preserving the primary worker's historical
  top-level snapshot while merging the per-limit map across workers for the
  current Stage B transport
- the real multi-worker remote `RemoteAppServerClient` harness now also covers
  the external-auth onboarding path for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating that the
  gateway preserves the immediate `chatgptAuthTokens` login completion and the
  resulting `account/updated` notification through the current Stage B
  multi-worker transport
- that same external-auth onboarding harness now also verifies duplicate
  `account/login/completed` / `account/updated` emissions from token-login
  fanout are suppressed on the shared `RemoteAppServerClient` session before
  the follow-up aggregated `account/read`
- dedicated northbound multi-worker regression coverage now also verifies that
  `account/login/start` with external-auth `apiKey` and
  `chatgptAuthTokens` is fanned out to every downstream worker session
  instead of updating only the primary worker
- the real multi-worker remote `RemoteAppServerClient` API-key onboarding
  harness now also observes the resulting `account/updated` notification and
  verifies duplicate worker emissions remain suppressed before the follow-up
  aggregated `account/read`
- dedicated northbound v2 regression coverage now also verifies that if a
  downstream app-server session reuses a still-pending server-request id, the
  gateway fails closed instead of silently overwriting the original pending
  route
- dedicated northbound v2 regression coverage now also verifies that if a
  downstream worker disconnects while a connection-scoped server request is
  still pending or awaiting downstream `serverRequest/resolved` on a shared
  multi-worker session, the gateway fails closed instead of silently dropping
  the unresolved prompt
- the real multi-worker `RemoteAppServerClient` harness now also covers that
  stranded connection-scoped prompt path, verifying that a pending
  `account/chatgptAuthTokens/refresh` request reaches the client before the
  gateway closes the shared session on worker disconnect
- that same real multi-worker harness now also covers the answered-but-
  unresolved variant of that prompt path, verifying that the shared session
  still fails closed if the client has already replied to
  `account/chatgptAuthTokens/refresh` but the owning worker disconnects before
  downstream `serverRequest/resolved`
- those disconnect-driven cleanup paths now also emit structured gateway
  warning logs with the gateway request ids that were auto-resolved for
  thread-scoped prompts and the gateway request ids that stranded the session
  for connection-scoped prompts, so operators can correlate cleanup behavior
  with the affected prompts
- the real embedded `RemoteAppServerClient` harness now also covers the same
  `item/tool/requestUserInput` server-request round trip, validating that the
  gateway proxies the typed response back into the in-process app-server
  session and forwards `serverRequest/resolved` before turn completion
- the real embedded `RemoteAppServerClient` harness now also covers the same
  plugin discovery path that current clients use beyond skills/app discovery,
  including `plugin/list` and `plugin/read`
- the real embedded `RemoteAppServerClient` harness now also covers the same
  post-bootstrap plugin management path, including `plugin/install` and
  `plugin/uninstall`, and verifies the resulting `plugin/list` installed-state
  transitions over the gateway transport
- the real embedded `RemoteAppServerClient` harness now also covers
  `item/permissions/requestApproval`, validating that the gateway preserves the
  in-process approval round trip and `serverRequest/resolved` ordering for the
  request-permissions flow too
- the real embedded `RemoteAppServerClient` harness now also covers
  `mcpServer/elicitation/request`, validating that the gateway preserves the
  in-process MCP elicitation round trip and `serverRequest/resolved` ordering
  for connector-driven tool flows too
- the real embedded `RemoteAppServerClient` harness now also covers
  lower-frequency thread-control flows, including `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- that same real embedded compatibility harness now also covers the
  realtime request workflow for `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`,
  `thread/realtime/stop`, and `thread/realtime/listVoices`, validating that
  the in-process gateway reaches the realtime sideband transport for start and
  append requests
- that same real embedded compatibility harness now also covers the resulting
  realtime notification forwarding for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`, validating that
  sideband events still surface over the gateway's northbound v2 stream in
  embedded mode
- that same real embedded compatibility harness now also covers turn-control
  parity for `turn/steer` and `turn/interrupt`, verifying the active turn is
  steered successfully and later completes as `interrupted` through the
  gateway transport
- multi-worker remote runtime now also aggregates the current threadless
  `app/list` request path across workers, including gateway-owned pagination
  over the merged result set instead of exposing only the primary worker's app
  inventory
- dedicated gateway passthrough tests now cover the remaining bootstrap-critical
  `account/read` and `model/list` requests that current Codex clients issue
  during startup
- single-worker remote compatibility tests now also cover `turn/start` plus the
  core turn-lifecycle notifications (`thread/status/changed`, `turn/started`,
  and `turn/completed`)
- northbound v2 connections now send an explicit WebSocket close frame with a
  gateway-owned reason when the downstream app-server session disconnects,
  instead of ending as an unexplained socket drop

### Milestone 5: Multi-Worker Connection Coordination

Status: in progress

Deliverables:

- support more than one downstream worker behind one northbound v2 connection
- route thread-scoped requests by sticky worker ownership
- maintain request and server-request correlation per worker
- fan in notifications safely from the owning worker(s)
- keep error semantics stable when a worker disconnects

Exit criteria:

- one northbound v2 connection can operate correctly in a multi-worker gateway
  deployment

### Milestone 6: Hardening

Status: in progress

Deliverables:

- backpressure strategy for northbound WebSocket clients
- explicit timeout and disconnect behavior
- load and reconnect tests
- operator docs for compatibility mode, caveats, and rollout

Exit criteria:

- the v2 compatibility surface has clear operational behavior and test coverage

Recent progress:

- northbound v2 connections now enforce an explicit initialize handshake
  timeout and close the WebSocket with a concrete gateway-owned reason instead
  of waiting indefinitely for a client that never completes the JSON-RPC
  bootstrap
- northbound v2 connections now also enforce an explicit client-send timeout on
  gateway-originated WebSocket writes, so a slow or wedged client fails closed
  instead of holding the compatibility session open indefinitely
- dedicated northbound v2 regression coverage now also exercises malformed
  client payload handling on both sides of the initialize boundary: invalid
  JSON-RPC text frames before and after initialize, plus invalid UTF-8 binary
  frames before and after initialize
- dedicated northbound v2 regression coverage now also verifies Ping/Pong
  behavior on the northbound socket both before and after initialize, so
  heartbeat handling remains stable across the handshake boundary
- dedicated northbound v2 regression coverage now also verifies that stray
  JSON-RPC notifications, responses, and errors received before `initialize`
  are ignored until the client sends the real handshake request, for both text
  and binary frame paths
- dedicated northbound v2 regression coverage now also verifies that a later
  duplicate `initialize` request is rejected with an invalid-request JSON-RPC
  error instead of re-entering handshake flow, for both text and binary client
  request paths
- dedicated northbound v2 regression coverage now also verifies the
  `initialize must be the first request` rule on both text and binary client
  request paths
- the gateway now also has dedicated northbound v2 regression coverage for the
  unresolved server-request cap, verifying that once one forwarded server
  request is still pending, a second downstream server request is rejected with
  a rate-limited JSON-RPC error and the compatibility session remains usable
- that server-request saturation path now also emits structured gateway warning
  logs with scope, worker id, the incoming gateway request id, and the still-
  pending gateway request ids that consumed the cap, so operators can diagnose
  why the connection hit the unresolved prompt limit without reproducing the
  whole session
- northbound v2 connections now also fail closed for pending downstream server
  requests when the northbound connection ends mid-flow, including client
  disconnects and gateway-owned protocol closes for malformed client payloads,
  plus other terminal northbound I/O failures such as client-send timeouts or
  broken pipes,
  rejecting those requests back to the downstream app-server session with a
  gateway-owned internal error instead of leaving them unresolved
- the gateway now has real northbound JSON-RPC regression tests for the current
  scope-policy hardening on `thread/resume` and `thread/fork`, verifying that
  hidden rollout paths are still rejected while visible path-based
  `thread/resume` / `thread/fork` requests stay allowed without bypassing
  thread ownership checks
- unknown path-based `thread/resume` re-entry and `thread/fork` rollout paths
  now also have request-metric coverage, verifying the gateway records
  `gateway_v2_requests{outcome="invalid_params"}` when scope policy rejects
  the path before routing downstream
- path-based `thread/resume` / `thread/fork` scope-policy tests now also pin
  that placeholder `threadId` values are ignored for route affinity while the
  rollout `path` remains the scope-check key
- legacy `getConversationSummary` requests that use `rolloutPath` now also use
  visible rollout paths as the scope-check key, route through the multi-worker
  path-discovery fallback, and register returned `conversationId` / `path`
  ownership for later sticky routing
- the real multi-worker legacy compatibility harness now also validates
  `getConversationSummary` by `conversationId` after thread ownership is
  registered, so both legacy summary request shapes are covered through an
  unmodified `RemoteAppServerClient` session
- legacy `gitDiffToRemote` requests now also use multi-worker
  worker-discovery routing instead of defaulting to the primary worker, so
  cwd-local git diff requests can be served by the worker that owns the
  requested checkout while degraded sessions still fail closed before serving
  an incomplete worker set; dedicated northbound WebSocket regressions now pin
  both steady-state fallback and lazy recovered-worker fallback
- deprecated `getAuthStatus` requests now also use multi-worker aggregation,
  preserving the primary worker's reported auth method/token while OR-ing
  `requiresOpenaiAuth` across all workers and failing closed during reconnect
  backoff instead of returning partial legacy auth state
- the gateway now also has real northbound JSON-RPC regression tests for
  `thread/list` and `thread/loaded/list` scope filtering, verifying that hidden
  threads are stripped from v2 responses at the transport boundary
- hidden-thread downstream server requests that are rejected by gateway scope
  policy now also emit structured warning logs with scope, worker id, worker
  websocket URL, request id, method, and hidden thread id, with direct
  northbound regression coverage
- multi-worker northbound v2 aggregation now also backfills missing worker
  ownership for already visible threads discovered through `thread/list` and
  `thread/loaded/list`, so later sticky thread-scoped requests such as
  `thread/read` do not misroute to the primary worker when the gateway only
  knew scope ownership before discovery
- the gateway now also has dedicated northbound passthrough coverage for the
  remaining Stage A thread/turn control methods that are less common in normal
  sessions, including `thread/unsubscribe`, `thread/compact/start`,
  `thread/shellCommand`, `thread/backgroundTerminals/clean`,
  `thread/rollback`, `review/start`, `turn/interrupt`, and `turn/steer`
- the embedded compatibility harness now also covers additional bootstrap/setup
  requests that current clients use during startup and settings flows:
  `externalAgentConfig/detect`, `externalAgentConfig/import`, `app/list`,
  `skills/list`, `mcpServerStatus/list`, `mcpServer/oauth/login`,
  `config/batchWrite`, `fs/watch`, `fs/unwatch`, `memory/reset`, and
  `account/logout`
- dedicated northbound passthrough coverage now also exercises those current
  TUI bootstrap/setup discovery requests directly at the gateway boundary:
  `externalAgentConfig/detect`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, and `plugin/list`
- dedicated northbound passthrough coverage now also exercises current TUI
  setup-mutation and plugin-management requests directly at the gateway
  boundary: `externalAgentConfig/import`, `plugin/read`, `plugin/install`,
  `plugin/uninstall`, `config/batchWrite`, `memory/reset`, and
  `account/logout`
- dedicated northbound passthrough coverage now also exercises the core
  thread/turn transport requests current TUI sessions rely on directly at the
  gateway boundary: `thread/start`, `thread/resume`, `thread/fork`,
  `thread/list`, `thread/loaded/list`, `thread/read`, `thread/name/set`,
  `thread/memoryMode/set`, and `turn/start`
- dedicated northbound notification coverage now also pins
  `thread/name/updated`, and the real embedded plus single-worker remote
  compatibility harnesses now also observe that rename notification during
  `thread/name/set` flows through unmodified `RemoteAppServerClient` sessions
- the real embedded compatibility harness now also covers `configWarning`
  during startup, and the real single-worker remote compatibility harness now
  also covers `warning`, `configWarning`, `deprecationNotice`,
  `account/login/completed`, `mcpServer/oauthLogin/completed`, and
  `mcpServer/startupStatus/updated`, so those visible notification paths are
  exercised through unmodified `RemoteAppServerClient` sessions in addition to
  dedicated northbound coverage
- the real single-worker remote compatibility harness now also covers the
  threadless experimental realtime discovery request
  `thread/realtime/listVoices`
- that same real single-worker remote compatibility harness now also covers
  connection-scoped filesystem watch setup and teardown through the gateway,
  exercising `fs/watch` and `fs/unwatch` with an unmodified
  `RemoteAppServerClient` session instead of only through targeted
  passthrough-style regression coverage
- northbound v2 connections now fail closed when the downstream app-server
  event stream reports backpressure via `Lagged`, closing the WebSocket with an
  explicit gateway-owned reason instead of silently continuing after best-effort
  event loss
- dedicated northbound v2 regression coverage now also verifies that a
  downstream `Lagged` event closes the northbound WebSocket with the expected
  policy close code and gateway-owned backpressure reason
- gateway-owned WebSocket close reasons are now clamped to protocol-safe
  length, so downstream disconnect errors cannot turn into invalid oversized
  control frames at the northbound client
- dedicated northbound v2 regression coverage now also verifies that an
  oversized downstream disconnect reason is truncated before the gateway emits
  the northbound close frame
- that close-reason truncation coverage now also exercises the invalid-payload
  close path, verifying that oversized malformed-payload errors are clamped
  before the gateway emits a protocol close frame
- multi-worker remote runtime now has explicit integration coverage for the
  current Stage B transport profile, verifying both the `/healthz`
  compatibility signal and that northbound v2 WebSocket upgrades are admitted
  instead of returning `501 Not Implemented`
- the northbound v2 transport now establishes one downstream app-server
  session per configured remote worker in multi-worker mode, including
  gateway-owned request routing for aggregated `thread/list` /
  `thread/loaded/list` responses plus translated server-request IDs across
  worker sessions
- multi-worker remote runtime now also has a real northbound v2 client
  regression for server-request round trips across two workers that reuse the
  same downstream request ID, covering `item/tool/requestUserInput`,
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`, and
  `mcpServer/elicitation/request`, plus the connection-scoped
  `account/chatgptAuthTokens/refresh`, and verifying that the gateway
  translates those IDs uniquely for the northbound client and routes each
  resolved response back to the correct worker session
- multi-worker remote runtime now also has a real northbound v2 client
  regression for per-thread `turn/start` routing and turn-lifecycle
  notification fan-in across two workers on one gateway session, verifying
  that turn requests reach the owning worker and the resulting
  `thread/status/changed`, `turn/started`, `item/agentMessage/delta`, and
  `turn/completed` notifications all fan back into the shared northbound
  connection
- that same multi-worker turn-routing regression now also covers additional
  streamed turn notifications the current TUI consumes:
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and `item/fileChange/outputDelta`
- that same multi-worker turn-routing regression now also covers
  longer-running turn lifecycle notifications, including `item/started`,
  `item/completed`, `hook/started`, and `hook/completed`, so one shared client
  session still sees item and hook progress from the owning worker
- multi-worker remote runtime now also has a real northbound v2 client
  regression for steady-state sticky realtime request routing and notification
  fan-in across two workers on one gateway session, verifying
  `thread/realtime/start`, `thread/realtime/appendText`, and
  `thread/realtime/stop` stay on the owning worker while
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, and `thread/realtime/closed` all fan
  back into the shared northbound connection
- that same multi-worker realtime regression now also covers
  `thread/realtime/appendAudio` plus shared-session delivery for the
  lower-frequency notifications `thread/realtime/outputAudio/delta`,
  `thread/realtime/sdp`, and `thread/realtime/error`
- that same multi-worker realtime coverage now also includes the
  connection-scoped discovery request `thread/realtime/listVoices`,
  aggregating distinct voice inventory across workers while keeping the
  primary worker defaults stable
- multi-worker remote runtime now also has a real northbound v2 client
  regression for turn-control parity across two workers on one shared gateway
  session, verifying sticky `turn/steer` and `turn/interrupt` routing plus
  shared-session fan-in for the resulting `item/agentMessage/delta`,
  `turn/completed`, and `thread/status/changed` notifications
- multi-worker remote runtime now also has a broader real northbound
  `RemoteAppServerClient` bootstrap/setup regression that exercises one shared
  session through aggregated `account/read`, `account/rateLimits/read`,
  `model/list`, `externalAgentConfig/detect`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, and worker-discovery `mcpServer/oauth/login`,
  instead of validating those setup paths only through separate targeted
  aggregation regressions
- that same multi-worker bootstrap/setup regression now also verifies that
  thread-scoped `app/list` stays sticky to the owning worker instead of
  falling into the threadless aggregation path
- that same multi-worker bootstrap/setup regression now also routes threadless
  `config/read` by matching `cwd` while keeping `configRequirements/read` on
  the primary worker, plus aggregated capability discovery for
  `experimentalFeature/list` and `collaborationMode/list`
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for primary-worker onboarding and
  feedback flows, covering `account/login/start`, `account/login/cancel`,
  `account/login/completed`, `feedback/upload`, and primary-worker-affine
  `account/sendAddCreditsNudgeEmail`
- that same real multi-worker primary-worker harness now also covers the
  standalone `command/exec` control plane
  (`command/exec`, `command/exec/outputDelta`, `command/exec/write`,
  `command/exec/resize`, and `command/exec/terminate`), so primary-worker
  command execution has baseline drop-in coverage in addition to reconnect
  recovery coverage
- multi-worker steady-state Stage B coverage now spans the main shared-session
  surface: connection-scoped setup mutations such as
  `externalAgentConfig/import`, `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `config/value/write`, `config/batchWrite`, `memory/reset`, and
  `account/logout` fan out across workers; bootstrap and setup discovery such
  as `account/read`, `model/list`,
  `externalAgentConfig/detect`, and `skills/list` aggregate instead of
  exposing only the primary worker view; and lower-frequency thread-control,
  review, and thread-mutation paths remain sticky to the owning worker,
  including `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, `review/start`, and
  `thread/name/set`
- multi-worker northbound initialization now also verifies that client identity,
  experimental capability, and `optOutNotificationMethods` are propagated to
  every downstream worker session, and lazy reconnect now reuses those same
  initialized capability parameters when a missing worker is re-added, keeping
  shared-session capability negotiation aligned with the embedded and
  single-worker release baselines
- the gateway northbound v2 boundary now also enforces exact
  `optOutNotificationMethods` suppression before forwarding downstream
  notifications, so the client-visible compatibility contract is preserved
  even if a downstream worker emits an opted-out notification
- opted-out notification suppression now also emits a structured gateway log
  with tenant/project scope, worker identity, method, and payload context, so
  operators can identify client-requested notification drops separately from
  duplicate suppression or hidden-thread filtering
- that opt-out suppression now also has dedicated multi-worker fan-in coverage,
  verifying that an opted-out notification from one worker is dropped while a
  different, non-opted-out notification from another worker is still forwarded
  on the shared northbound session
- downstream malformed JSON-RPC frames now close the northbound v2 WebSocket
  with explicit gateway-owned behavior, record a downstream protocol-violation
  metric, and report the connection outcome as `downstream_protocol_violation`
  instead of folding the case into ordinary downstream session termination
- malformed downstream frame handling now also emits a structured gateway log
  with tenant/project scope, worker identity, reason, downstream message, active
  worker count, and pending / answered-but-unresolved server-request routes, so
  surviving multi-worker sessions still expose the worker protocol violation to
  operators; malformed downstream JSON-RPC after initialization now also has
  real northbound WebSocket coverage for that structured warning-log path
- post-handshake downstream malformed JSON-RPC and non-text frame regressions
  now also pin the matching gateway v2 connection audit record with
  `outcome="downstream_protocol_violation"`, so worker protocol failures are
  visible through audit logs as well as close behavior, metrics, and structured
  diagnostics
- post-handshake downstream unexpected JSON-RPC response and error id
  regressions now also pin that same gateway v2 connection audit record, so
  out-of-order worker traffic has the same audit visibility as malformed
  worker frames
- downstream non-text JSON-RPC data frames now also fail closed as explicit
  `invalid_binary` protocol violations instead of being silently ignored by the
  remote app-server transport, so malformed-frame hardening covers both invalid
  text payloads and unsupported binary data frames
- downstream non-text frames during the app-server initialize handshake are now
  also classified as downstream `invalid_binary` protocol violations, so
  setup-time malformed worker traffic is reported with the same gateway-owned
  outcome and metric as post-handshake malformed frames; the underlying remote
  app-server client transport now also has direct regression coverage for
  failing connection setup on those non-text initialize frames
- downstream invalid JSON-RPC during the app-server initialize handshake is now
  also classified as a downstream `invalid_jsonrpc` protocol violation, closing
  the setup-time malformed text response path alongside post-handshake invalid
  JSON-RPC handling; the underlying remote app-server client transport now also
  has direct regression coverage for failing connection setup on invalid
  initialize JSON-RPC text frames
- downstream initialize responses or errors with a valid JSON-RPC envelope but
  the wrong request id now also fail immediately as malformed initialize
  responses instead of waiting for the initialize timeout; gateway northbound
  coverage verifies these setup-time protocol violations record the downstream
  `invalid_jsonrpc` metric and terminal `downstream_protocol_violation`
  outcome
- malformed downstream initialize-response coverage now also pins the matching
  gateway v2 audit log with tenant/project scope and
  `outcome="downstream_protocol_violation"`, so startup compatibility failures
  are visible through metrics, protocol-violation counters, connection health,
  and audit logs together
- downstream connect failures after a valid northbound initialize request now
  also have dedicated request-metric coverage, verifying
  `gateway_v2_requests{method="initialize",
  outcome="downstream_connect_error"}` stays separate from downstream
  protocol-violation outcomes
- that downstream connect-failure coverage now also pins the matching gateway
  v2 audit log with tenant/project scope and
  `outcome="downstream_connect_error"`, so worker reachability failures are
  visible through both telemetry paths
- initialize timeout coverage now also pins the matching gateway v2 audit log
  with tenant/project scope and `outcome="timed_out"` alongside the request
  metric, so startup stalls are visible through both telemetry paths
- invalid initialize params now also have dedicated request-metric coverage,
  verifying `gateway_v2_requests{method="initialize",
  outcome="invalid_request"}` for parseable but malformed startup requests
- that invalid initialize-params coverage now also pins the matching gateway
  v2 audit log with tenant/project scope and `outcome="invalid_request"`, so
  malformed startup requests are visible through both telemetry paths
- pre-initialize ordering-violation coverage now also pins the matching gateway
  v2 audit log with tenant/project scope and `outcome="invalid_request"`, so
  requests sent before `initialize` are visible through metrics,
  protocol-violation counters, and audit logs together
- downstream server requests received during the app-server initialize
  handshake now also have direct remote transport coverage for the unsupported
  request path: unsupported setup-time requests are rejected with a JSON-RPC
  method-not-found error while the initialize handshake can still complete
- valid downstream server requests received during the app-server initialize
  handshake now also have direct gateway northbound coverage, verifying that
  the pending request is forwarded after northbound initialize completes and
  that the northbound client response is routed back to the owning worker
- unsupported downstream server requests received during the app-server
  initialize handshake now also have direct gateway northbound coverage,
  verifying method-not-found rejection at the worker transport while the
  northbound session still completes initialize and serves a follow-up request
- downstream JSON-RPC responses or errors with unknown request ids after
  initialization now also fail closed as downstream `invalid_jsonrpc` protocol
  violations instead of being silently ignored, so out-of-order downstream
  traffic is visible at the gateway boundary
- setup-time downstream protocol violations now also emit a structured gateway
  warning log with tenant/project scope, violation reason, and downstream
  transport detail before the initialize request returns an error, so malformed
  worker handshakes have the same operator-visible diagnostics as
  post-handshake malformed frames
- lazy multi-worker reconnect now also classifies malformed downstream
  initialize traffic as a downstream protocol violation, records the
  protocol-violation metric, and emits a structured warning with tenant/project
  scope plus worker id / URL, so recovered-worker handshakes cannot disappear
  into generic reconnect failures
- that same steady-state Stage B surface now also keeps tenant/project scope
  enforcement and worker-local state coherent across one shared session:
  same-scope clients can re-enter aggregated
  `thread/list` / `thread/loaded/list` / `thread/read` state, hidden threads
  still fail closed as `thread not found`, `skills/changed` invalidations are
  deduplicated until a later `skills/list` refresh, and exact-duplicate
  connection-state notifications such as `account/updated`,
  `account/rateLimits/updated`, `app/list/updated`, and
  `mcpServer/startupStatus/updated` are suppressed across workers
- multi-worker steady-state plugin and marketplace handling now also has real
  shared-session coverage: threadless `plugin/list` merges marketplace state
  across workers, and `plugin/read`, `plugin/install`, and
  `plugin/uninstall` route to the worker that can satisfy the selected plugin
  request instead of exposing only the primary worker's local plugin state
- dedicated plugin-merge coverage now also pins that an installed worker copy
  of a repeated plugin remains the selected summary even when later workers
  report the same plugin as only available, keeping aggregated `plugin/list`
  installed state stable across worker ordering
- single-worker remote runtime now also has a northbound reconnect regression
  test, verifying that after the gateway reconnects its downstream worker, a
  later v2 client session can still bootstrap and complete `thread/start`
- single-worker remote runtime now also has a real northbound v2 client
  regression for realtime workflow parity, covering
  `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, and `thread/realtime/stop`, plus delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- single-worker remote runtime now also has a real northbound v2 client
  regression for turn-control parity, covering `turn/steer` and
  `turn/interrupt` plus the resulting `item/agentMessage/delta`,
  `turn/completed`, and `thread/status/changed` notifications
- single-worker remote runtime now also has a broader real northbound v2
  client regression for lower-frequency thread-control and review flows,
  covering `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- the real embedded compatibility harness now also covers that same
  lower-frequency thread-control set for `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- that same single-worker remote real bootstrap/setup harness now also covers
  `app/list` and `mcpServerStatus/list`, keeping connection-scoped discovery
  closer to the embedded parity harness
- that reconnect regression now waits for `/healthz` to report a recovered
  single-worker compatibility profile, so it covers both worker recovery and
  operator-visible health convergence after reconnect and subsequent
  northbound v2 admission
- `/healthz` operator coverage now also includes a slow-client fail-closed
  regression in single-worker remote mode, and the real multi-worker health
  harness now also pins the same operator-visible teardown path, pinning
  `v2Connections.lastConnectionOutcome=client_send_timed_out` and the gateway-
  owned timeout detail plus `v2Connections.lastConnectionDurationMs` instead of
  only the normal `client_closed` path
- that reconnect regression now also verifies the recovered v2 session can
  still complete a bidirectional `item/tool/requestUserInput` server-request
  round trip, not just bootstrap and `thread/start`
- the real single-worker remote reconnect harness now also covers lower-
  frequency thread-control and detached review flows, verifying that a later
  client session can still `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`, and
  detached `review/start` through the recovered worker
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still complete an `item/tool/call` dynamic-tool round trip
  with `serverRequest/resolved` ordering plus `item/started` /
  `item/completed` forwarding before `turn/completed`
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still satisfy `account/read` and `account/rateLimits/read`,
  so bootstrap account state and background rate-limit refresh survive worker
  recovery too
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still apply low-frequency setup mutations:
  `externalAgentConfig/import`, `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `config/batchWrite`, `config/value/write`, `memory/reset`, and
  `account/logout`, so the single-worker release-quality baseline covers
  post-reconnect setup writes as well as discovery refreshes
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still complete the basic filesystem operation family:
  `fs/createDirectory`, `fs/writeFile`, `fs/readFile`, `fs/getMetadata`,
  `fs/readDirectory`, `fs/copy`, and `fs/remove`
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still complete the standalone command-execution control
  plane: `command/exec`, `command/exec/outputDelta`,
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`
- the real embedded compatibility harness now also exercises the standalone
  command-execution controls against a live PTY-backed process, so
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate` are covered by the embedded release-quality client
  path in addition to passthrough and remote harness coverage
- northbound v2 now lets a long-running `command/exec` response remain pending
  while the same WebSocket connection continues to process
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`, closing the embedded live-process control gap that
  passthrough-only fixtures could not exercise
- that pending `command/exec` response path is also bounded by the gateway's
  per-connection pending-request limit, so long-running standalone command
  sessions fail closed with a gateway-owned rate-limit error instead of
  accumulating unbounded background upstream requests
- dedicated northbound v2 regression coverage now verifies the overload path
  through a real WebSocket session: a second `command/exec` is rejected while
  the first request remains pending, and the connection still delivers the
  original command response after the downstream worker releases it
- that same overload regression now also verifies the v2 request metrics for
  `command/exec` with `outcome="rate_limited"` and `outcome="ok"`, making the
  bounded-saturation path observable separately from successful long-running
  command completion
- saturated pending client requests now also increment
  `gateway_v2_client_request_rejections` with `method` and `reason` tags, so
  background command-exec overload has a dedicated counter alongside request
  outcome metrics
- direct audit-log coverage now also pins the saturated pending client-request
  path as `gateway_v2_requests{method="command/exec",outcome="rate_limited"}`,
  so rejected long-running command requests remain visible in request-level
  audit trails as well as rejection counters
- dedicated log coverage now also pins the structured warning emitted for
  saturated pending client requests, including tenant/project scope, request
  id, method, current pending count, and configured limit
- `/healthz` now also reports active and last-completed pending v2 client
  request counts, making the background `command/exec` pending queue visible
  alongside existing server-request prompt lifecycle counts
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still complete the post-bootstrap plugin-management path:
  `plugin/read`, `plugin/install`, and `plugin/uninstall`, including the
  resulting installed-state refreshes through `plugin/list`
- that same single-worker reconnect regression now also verifies the recovered
  v2 session can still complete a realtime workflow for
  `thread/realtime/listVoices`, `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`, and
  `thread/realtime/stop`, plus delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that same single-worker reconnect regression now also verifies the recovered
  v2 session still delivers `account/updated`,
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, `mcpServer/startupStatus/updated`, and
  `skills/changed`
- multi-worker remote runtime now also has a northbound v2 reconnect
  regression, verifying that after one worker disconnects and recovers, a
  later client session can again route `thread/start` to the recovered worker
  and still aggregate `thread/list` / `thread/read` with a second healthy
  worker on the same gateway session
- that reconnect regression now also verifies the recovered multi-worker v2
  session can still complete a bidirectional `item/tool/requestUserInput`
  server-request round trip with the recovered worker, not just bootstrap and
  thread routing
- dedicated northbound v2 regression coverage now also verifies that if one
  downstream worker disconnects after a thread-scoped server request has
  already been answered but before downstream `serverRequest/resolved`, the
  gateway still synthesizes `serverRequest/resolved` northbound instead of
  leaving the shared session stranded
- the real multi-worker remote `RemoteAppServerClient` harness now also covers
  that answered-but-unresolved thread-scoped server-request disconnect path,
  validating that `serverRequest/resolved` is still synthesized northbound and
  the shared session can continue onto later worker-owned threads
- that same reconnect regression now also verifies the recovered multi-worker
  v2 session can still route one shared realtime workflow across the recovered
  worker and a second healthy worker, covering aggregated
  `thread/realtime/listVoices`, sticky `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`, and
  `thread/realtime/stop` plus fan-in for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that reconnect regression now also verifies the recovered multi-worker v2
  session still suppresses exact-duplicate connection-state notifications
  across workers for `account/updated`, `account/rateLimits/updated`,
  `account/login/completed`, `app/list/updated`, `warning`,
  `configWarning`, `deprecationNotice`,
  `mcpServer/startupStatus/updated`, and
  `externalAgentConfig/import/completed`
- dedicated northbound v2 regression coverage now also pins the steady-state
  exact-duplicate suppression path for `account/rateLimits/updated` and
  `app/list/updated` alongside `account/updated`, so the core connection-state
  dedupe set is covered directly at the gateway boundary
- dedicated northbound v2 notification forwarding coverage now also includes
  the full core connection-state set of `account/updated`,
  `account/rateLimits/updated`, and `app/list/updated`, so single-worker
  passthrough and multi-worker dedupe coverage exercise the same notification
  family
- that reconnect regression now also verifies the recovered multi-worker v2
  session still suppresses duplicate `skills/changed` invalidations across
  workers until the client refreshes with `skills/list`, and still emits one
  fresh invalidation after that refresh
- dedicated northbound multi-worker v2 regression coverage now also verifies
  that a later aggregated `skills/list` request reconnects and re-includes a
  previously dropped worker within the same shared client session, instead of
  leaving connection-scoped discovery pinned to the surviving subset
- that same reconnect-hardening slice now also covers aggregated `app/list`,
  verifying that later connector discovery requests re-add the recovered worker
  to the shared result set instead of leaving marketplace / app inventory
  partially degraded after a transient worker loss
- that same reconnect-hardening slice now also verifies a later thread-scoped
  `app/list` request still routes to the recovered worker on the same shared
  session, instead of falling back to the surviving worker or the threadless
  aggregation path after reconnect
- that same reconnect-hardening slice now also covers aggregated
  `mcpServerStatus/list`, verifying that later MCP inventory refreshes
  re-include a recovered worker instead of leaving connection-scoped tool /
  resource status partially stale after a transient worker loss
- that same reconnect-hardening slice now also covers threadless
  `plugin/list`, verifying that later plugin-catalog refreshes re-add a
  recovered worker into the shared marketplace view instead of leaving
  aggregated plugin discovery partially degraded after a transient worker loss
- that same reconnect-hardening slice now also covers aggregated
  `account/read` and `model/list`, verifying that later bootstrap
  refreshes re-include a recovered worker in merged auth/model state instead
  of leaving one shared session pinned to stale bootstrap inventory
- that same reconnect-hardening slice now also covers aggregated
  `account/rateLimits/read`, verifying that later background rate-limit
  refreshes re-include a recovered worker in the merged per-limit view instead
  of leaving one shared session pinned to stale quota state
- that same reconnect-hardening slice now also covers cwd-aware threadless
  `config/read`, verifying that later config refreshes still reconnect and
  select the worker whose config layer matches the requested cwd instead of
  falling back to stale primary-worker config after a transient worker loss
- that same reconnect-hardening slice now also covers aggregated
  `thread/list` and `thread/loaded/list`, verifying that later thread-discovery
  refreshes re-include a recovered worker and backfill sticky ownership for
  the visible threads it contributes instead of leaving routing pinned to the
  surviving subset
- that same reconnect-hardening slice now also covers `plugin/read`,
  `plugin/install`, and `plugin/uninstall`, verifying that later fallback
  plugin-management requests reconnect a recovered worker before selecting the
  first worker that can satisfy the operation
- that same reconnect-hardening slice now also covers connection-scoped
  `fs/watch` and `fs/unwatch`, verifying that later filesystem watch setup and
  teardown reconnect a recovered worker before the shared watch state is
  applied, so one northbound session does not silently lose worker-local watch
  coverage after a transient disconnect
- dedicated northbound request-routing regression coverage now also verifies
  reconnect-before-routing for primary-worker-only requests in
  `handle_client_request`, covering `configRequirements/read`, managed
  `account/login/start`, `account/login/cancel`, and `feedback/upload` so
  primary-owned setup and feedback flows do not silently drift onto a
  surviving secondary worker after reconnect
- dedicated northbound v2 metrics coverage now also fixes the
  `client_send_timed_out` connection outcome in regression tests, so rollout
  dashboards can treat slow-client send failures as a stable gateway-owned
  signal instead of depending on incidental string behavior
- that same reconnect-hardening slice now also covers aggregated
  `externalAgentConfig/detect`, `experimentalFeature/list`, and
  `collaborationMode/list`, verifying that later capability/discovery refreshes
  re-include a recovered worker instead of leaving one shared session pinned
  to a stale worker subset for imported-config, feature-flag, or collaboration
  mode inventory
- that same reconnect-hardening slice now also covers aggregated
  `thread/realtime/listVoices`, verifying that later realtime voice discovery
  refreshes re-include a recovered worker instead of leaving one shared
  session pinned to stale voice inventory after a transient worker loss
- northbound multi-worker v2 connections now also survive loss of one
  downstream worker within the same client session, dropping the failed worker,
  synthesizing `serverRequest/resolved` for that worker's thread-scoped
  in-flight prompts, and continuing to serve `thread/start` plus aggregated
  `thread/list` through the remaining workers
- those same shared multi-worker v2 sessions now also re-add a recovered
  downstream worker lazily on later client requests, so the session can again
  route `thread/start`, aggregated `thread/list` / `thread/loaded/list`, and
  sticky `thread/read` through that worker without forcing a northbound
  reconnect
- that same-session recovery path now also verifies sticky `thread/resume` and
  `thread/fork` on the recovered worker, plus a follow-up `thread/read` of the
  returned forked thread to confirm worker ownership is re-registered for the
  new thread id after reconnect
- that same-session recovery path now also verifies visible path-based
  `thread/resume` and `thread/fork` on the recovered worker, so rollout-path
  routing stays sticky after reconnect on one shared northbound session
- that same-session recovery path now also verifies sticky
  `thread/name/set` on the recovered worker, plus a follow-up `thread/read`
  that returns the renamed state over the same northbound session
- that same-session recovery path now also verifies sticky
  `thread/memoryMode/set` on the recovered worker, so thread-scoped mutation
  requests do not fall back to the surviving worker after reconnect
- same-session recovery coverage now spans the broader shared-session surface:
  recovered workers stay sticky for thread mutation and control flows such as
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `review/start`, `turn/start`, `turn/steer`, `turn/interrupt`,
  `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`,
  `thread/unsubscribe`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`; recovered workers
  are re-added into connection-scoped fanout and discovery paths such as
  `fs/watch` / `fs/unwatch`, `account/read`, deprecated `getAuthStatus`,
  `account/rateLimits/read`, `model/list`, threadless `app/list`,
  threadless `plugin/list`,
  `skills/list`, `mcpServerStatus/list`, `thread/realtime/listVoices`,
  threadless and cwd-aware `config/read`, `experimentalFeature/list`, and
  `collaborationMode/list`; recovered primary-worker flows stay usable for
  `configRequirements/read`, managed `account/login/start` /
  `account/login/cancel`, `account/login/completed`, `feedback/upload`, and
  the standalone `command/exec` control plane; plugin discovery and fallback
  management continue to re-include the recovered worker; translated
  `item/tool/requestUserInput`, `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`,
  `mcpServer/elicitation/request`, and
  `account/chatgptAuthTokens/refresh` server requests also continue routing
  back through the re-added worker without leaking stale worker-local request
  ids; and follow-up `thread/read` still confirms ownership for review threads
  materialized after reconnect
- that same recovery surface now also keeps notification fan-in intact after a
  worker returns, covering recovered-worker turn lifecycle notifications
  (`thread/status/changed`, `turn/started`, `hook/started`, `item/started`,
  `item/agentMessage/delta`, reasoning / command / file-change deltas,
  `hook/completed`, `item/completed`, and `turn/completed`) plus the current
  full realtime notification set, while dedicated regressions also verify
  reconnect-on-demand for both a thread-scoped
  `item/tool/requestUserInput` round trip and a connection-scoped
  `account/chatgptAuthTokens/refresh` round trip, and confirm that a managed
  `account/login/start` can reconnect a missing primary worker before
  forwarding the recovered worker's `account/login/completed` notification
- that same dedicated northbound reconnect slice now also covers
  `configRequirements/read`, managed `account/login/cancel`,
  `feedback/upload`, and the standalone `command/exec` control plane, pinning
  those recovered primary-worker routes directly on the shared northbound
  WebSocket session instead of only through lower-level request-routing tests
- dedicated northbound reconnect coverage now also pins recovered-worker
  fanout on the shared WebSocket session for `externalAgentConfig/import`,
  `config/batchWrite`, `config/value/write`, `memory/reset`,
  `account/logout`, and external-auth `account/login/start`
- that same dedicated northbound reconnect slice now also covers `fs/watch`
  and `fs/unwatch`, pinning that one shared client session re-adds a
  recovered worker before shared filesystem watch setup or teardown fans back
  out across the downstream set
- that same dedicated northbound reconnect slice now also covers aggregated
  `thread/list` and `thread/loaded/list`, pinning that one shared client
  session re-adds a recovered worker into thread discovery and backfills
  sticky worker ownership for the visible threads it contributes
- that same dedicated northbound reconnect slice now also covers sticky
  thread-routed requests `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`,
  `thread/name/set`, `thread/memoryMode/set`, `turn/steer`, and
  `turn/interrupt`, pinning that one shared client session routes those
  requests back onto the recovered owning worker
- that same dedicated northbound reconnect slice now also covers detached
  `review/start`, pinning that one shared client session re-adds the recovered
  owning worker before review routing, re-registers the returned detached
  review thread, and keeps the follow-up `thread/read` sticky to that worker
- that same dedicated northbound reconnect slice now also covers sticky
  realtime requests `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, and `thread/realtime/stop`, pinning that one
  shared client session routes those worker-owned realtime requests back onto
  the recovered owning worker
- that same dedicated northbound reconnect slice now also covers sticky
  `turn/start`, pinning that one shared client session routes a recovered
  worker-owned turn start back onto the owning worker before the broader turn
  lifecycle fan-in begins
- lazy reconnect now replays the client's prior `initialized` notification and
  any active `fs/watch` registrations onto re-added downstream sessions, emits
  structured reconnect logs for attempt / failure / replay failure / success
  with direct regression coverage, applies per-worker retry backoff, and keeps
  the shared session on multi-worker semantics while only one downstream worker
  is temporarily live
- degraded-session handling now fails closed while required workers are still
  in reconnect retry backoff for connection-scoped fanout mutations,
  cwd-aware `config/read`, worker-discovery plugin requests, and
  primary-worker-only requests; those paths now also emit structured warning
  logs with scope and worker-route diagnostics, while worker-loss cleanup logs
  include the gateway request ids that were auto-resolved for thread-scoped
  prompts versus the ids that stranded the session for connection-scoped
  prompts
- worker-discovery fallback routing now also covers `mcpServer/oauth/login`,
  with dedicated passthrough coverage for the request itself and a
  reconnect-on-demand regression showing the gateway can re-add a recovered
  worker before selecting the downstream session that owns the named MCP
  server
- that same dedicated northbound reconnect coverage now also pins the follow-
  up `mcpServer/oauthLogin/completed` notification on the recovered session,
  so the worker-discovery request path and its completion notification stay
  coupled at the transport boundary
- worker-discovery plugin fallback now also has dedicated northbound websocket
  reconnect coverage for `plugin/read`, `plugin/install`, and
  `plugin/uninstall`, pinning that one shared client session can route those
  requests back onto a recovered worker instead of only relying on lower-level
  request-routing coverage
- threadless and cwd-aware `config/read` now also have dedicated northbound
  websocket reconnect coverage, pinning that one shared client session either
  reselects the recovered primary worker for threadless reads or re-selects
  the recovered worker whose config layers match the requested `cwd` instead
  of falling back to stale surviving-worker config
- aggregated capability discovery now also has dedicated northbound websocket
  reconnect coverage for `experimentalFeature/list` and
  `collaborationMode/list`, pinning that one shared client session re-adds a
  recovered worker before merging the feature and collaboration-mode inventory
- aggregated bootstrap and discovery refreshes now also have dedicated
  northbound websocket reconnect coverage for `account/read`,
  `account/rateLimits/read`, `model/list`,
  `externalAgentConfig/detect`, `skills/list`, threadless `app/list`,
  threadless `plugin/list`, `mcpServerStatus/list`, and
  `thread/realtime/listVoices`, pinning that one shared client session re-adds
  a recovered worker before those merged reads are served back northbound
- the real multi-worker same-session recovery harness now also verifies that
  `mcpServer/oauth/login` routes back to a lazily re-added worker and still
  forwards the resulting `mcpServer/oauthLogin/completed` notification on the
  shared northbound client session
- the real single-worker and multi-worker remote compatibility harnesses now
  also exercise `mcpServer/oauth/login` plus the follow-up
  `mcpServer/oauthLogin/completed` notification over the gateway's northbound
  v2 client transport
- the real single-worker reconnect harness now also verifies that later
  `model/list`, `externalAgentConfig/detect`, threadless `app/list`,
  `skills/list`, threadless `plugin/list`, threadless `config/read`,
  `configRequirements/read`, `experimentalFeature/list`,
  `collaborationMode/list`, `mcpServerStatus/list`, and
  `mcpServer/oauth/login` requests still succeed after worker recovery,
  including the follow-up `mcpServer/oauthLogin/completed` notification on
  the recovered session
- that same real single-worker reconnect harness now also re-exercises the
  lower-frequency thread-control path after worker recovery:
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- protocol and dedupe hardening now covers exact-duplicate suppression for the
  current multi-worker connection-state / completion invalidation set,
  including `warning`, `configWarning`, `deprecationNotice`,
  `externalAgentConfig/import/completed`, and `skills/changed`; dropping
  duplicate downstream `serverRequest/resolved` replays after request-id
  translation; metric coverage for duplicate external-agent import completion
  suppression; fail-closed handling when a client replies to a server request
  that is no longer pending, including rejection of any still-pending
  downstream prompts; and structured warning logs for both
  protocol-violation cleanup and unresolved server-request saturation
- that same protocol-violation logging now also includes any answered-but-
  unresolved server-request routes when the client replies to an unknown
  request id, and a real northbound regression now pins that ordering-sensitive
  path after one prior server request has already been answered
- downstream app-server event-stream backpressure now also emits a structured
  warning log with scope, worker id, worker websocket URL, skipped-event count,
  and any still-pending gateway/downstream server-request ids plus affected
  thread and worker ids before the gateway closes the northbound session, so
  lag-driven teardown is visible without reconstructing the session from
  connection outcomes alone
- that same backpressure path now also includes any answered-but-unresolved
  server-request routes in those warning logs, and a northbound regression now
  pins the fail-closed close frame plus translated gateway/downstream request
  ids plus affected thread ids after the client has already answered but
  downstream `serverRequest/resolved` never arrives before lag closes the
  session
- slow-client send timeouts now also emit a structured warning log with scope,
  terminal timeout detail, any still-pending gateway/downstream server-request
  ids, and affected thread and worker ids before pending downstream prompts are
  rejected, so `client_send_timed_out` outcomes can be tied back to the
  stranded session state directly from logs
- that same slow-client warning now also includes pending background
  client-request ids and methods, so long-running `command/exec` responses are
  visible in teardown diagnostics alongside stranded server-request prompt
  state; dedicated northbound WebSocket coverage now pins that slow-client
  diagnostic path while `command/exec` remains pending in the background
- pending background client-request diagnostics now also include downstream
  worker ids and WebSocket URLs, so multi-worker `command/exec` stalls and
  aborts can be attributed to the owning app-server session from logs
- v2 connection accounting now also records terminal outcome and decrements the
  active connection count even if an early handshake-time close frame or
  JSON-RPC error response cannot be delivered, so `/healthz` does not retain
  stale active-session state after setup-time send failures
- `/healthz` now also captures the last completed connection's pending and
  answered-but-unresolved server-request counts, so partially completed prompt
  lifecycles remain visible even after the gateway has already rejected or
  cleaned up the stranded downstream work
- client-side cleanup now also emits server-request lifecycle metrics when
  pending prompt rejection delivery fails or must be skipped because the owning
  downstream worker route is already unavailable, and dedicated cleanup
  coverage pins both the log messages and metric events; the matching warning
  logs include the affected worker websocket URL for both failed and skipped
  rejection delivery, while the pre-cleanup warning includes pending
  downstream server-request ids and affected thread ids for northbound /
  downstream prompt correlation; answered-but-unresolved route logs now also
  include affected thread ids
- worker-loss cleanup now also emits a server-request lifecycle metric and
  structured warning when synthesized `serverRequest/resolved` delivery fails,
  so thread-scoped prompt cleanup failures are visible before the gateway
  returns the terminal send error; that lifecycle metric is tagged by the
  original server-request method, and the send-failure warning includes the
  affected worker websocket URL plus prompt method
- successful synthesized `serverRequest/resolved` delivery during worker-loss
  cleanup now also emits a server-request lifecycle metric on the real
  northbound WebSocket path, tagged by the original server-request method, so
  operators can distinguish delivered synthetic prompt resolution from cleanup
  send failures by prompt family
- worker-loss cleanup warning logs now also include affected thread ids for
  thread-scoped prompts that the gateway resolves synthetically after a
  downstream worker disappears, so cleanup diagnostics point back to the
  visible thread state directly
- if pending prompt rejection delivery fails during connection teardown, the
  gateway now still records the terminal connection health, metrics, and audit
  fields with the pre-cleanup pending and answered-but-unresolved prompt
  counts instead of replacing them with the zero-count outer error fallback
- that slow-client path now also has a real northbound regression where one
  pending downstream server request is left unresolved while large follow-up
  notifications wedge the client send path, verifying the timeout warning,
  `client_send_timed_out` outcome, and downstream prompt rejection together
- that same slow-client regression now also pins the server-request lifecycle
  metrics for the originally forwarded prompt and the later
  `client_cleanup_rejected_thread_scoped` cleanup event, so dashboards can
  correlate the timeout with the rejected prompt lifecycle rather than only the
  connection outcome
- those slow-client and connection-end teardown logs now also include any
  answered-but-unresolved server-request routes, with translated gateway ids,
  downstream ids, and worker ids, and that same slow-client path now also has
  a real northbound regression where the client has already replied but
  downstream `serverRequest/resolved` never arrives before timeout
- that answered-but-unresolved slow-client regression now also pins the
  forwarded, answered, delivered, and
  `client_cleanup_answered_but_unresolved` lifecycle metrics, so the partially
  completed prompt is visible from first northbound delivery through terminal
  cleanup
- that same connection-end teardown path now also has a real northbound
  regression for client disconnects after one server request has already been
  answered but before downstream `serverRequest/resolved` arrives, pinning the
  answered-but-unresolved route ids that should still appear in the teardown
  warning log
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  request-id translation now also emit structured warning logs with scope,
  worker id, the replayed downstream request id, and any still-buffered
  translated gateway ids, downstream ids, and worker ids
- that duplicate-replay path now also has direct northbound regression
  coverage, pinning the warning log fields emitted when a multi-worker
  downstream session replays `serverRequest/resolved` after the translated
  route has already been drained
- that same northbound regression now also pins the lifecycle metrics for both
  the initial `downstream_server_request_resolved` event and the later
  `duplicate_resolved_replay` event on the translated server-request route
- suppressed multi-worker notification dedupe paths now also emit structured
  warning logs: duplicate `skills/changed` invalidations include scope,
  worker id, worker websocket URL, and params until the client refreshes
  `skills/list`, and exact-duplicate connection-state notifications include
  the same dropped-worker context plus the original worker id and websocket URL
  for the already-forwarded payload
- that same exact-duplicate connection-state suppression now also covers
  `mcpServer/oauthLogin/completed`, so one shared northbound session does not
  surface duplicate MCP OAuth completion notifications when more than one
  worker emits the same payload
- the real multi-worker connection-state harnesses now also verify that
  `mcpServer/oauthLogin/completed` is emitted exactly once both in steady
  state and after worker reconnect, so that dedupe path is exercised through
  an unmodified `RemoteAppServerClient` session in addition to dedicated
  northbound regressions
- the real single-worker remote connection-state harness now also covers
  `warning`, `configWarning`, `deprecationNotice`, and
  `windows/worldWritableWarning` in both steady state and after worker
  reconnect, so the release-quality remote baseline exercises those visible
  notification paths through an unmodified `RemoteAppServerClient` session
- hidden-thread downstream server requests that are rejected by gateway scope
  policy now also emit structured warning logs with scope, worker id, the
  worker websocket URL, translated gateway request id, method, and hidden
  thread id, so approval prompts dropped at the transport boundary are visible
  without reproducing the client session
- real multi-worker `RemoteAppServerClient` scope coverage now also verifies
  that hidden-thread downstream server requests are rejected before they reach
  a cross-scope northbound client, while that shared client session remains
  usable for later filtered requests such as `thread/list`
- downstream disconnect cleanup now also covers the single-worker and final-
  worker paths: unresolved thread-scoped server requests are translated into
  `serverRequest/resolved` before the northbound session closes, while
  unresolved connection-scoped requests fail closed with the stranded-session
  close reason instead of being dropped silently on disconnect
- the real multi-worker `RemoteAppServerClient` harness now also covers that
  stranded connection-scoped path, verifying that a pending
  `account/chatgptAuthTokens/refresh` prompt is delivered northbound and then
  forces the shared session to fail closed when the owning worker disconnects
- that same real multi-worker harness now also covers the answered-but-
  unresolved variant of that connection-scoped path, verifying that the shared
  session still fails closed if the client has already replied to
  `account/chatgptAuthTokens/refresh` but the owning worker disconnects before
  downstream `serverRequest/resolved`
- that same cleanup policy now has explicit end-of-stream regression coverage
  for both single-worker and shared multi-worker sessions, so natural
  downstream session termination preserves the same fail-closed behavior for
  connection-scoped prompts and the same synthesized resolution path for
  thread-scoped prompts as the explicit disconnect path
- downstream worker disconnects are now also treated as single-delivery
  terminal events at the v2 transport boundary, so a later end-of-stream from
  the same dropped worker cannot evict a lazily re-added worker or spuriously
  close a surviving shared multi-worker session
- worker-loss cleanup logs now also have explicit end-of-stream regression
  coverage on both the single-worker and shared multi-worker paths, pinning
  the resolved-versus-stranded request-id fields that operators use to
  diagnose cleanup behavior after natural downstream session termination
- those same worker-loss cleanup logs now also include downstream request ids
  for both resolved thread-scoped prompts and stranded connection-scoped
  prompts, and a real shared-session regression now pins the answered-but-
  unresolved connection-scoped path where the client already replied but the
  worker session ends before downstream `serverRequest/resolved`
- legacy `execCommandApproval` and `applyPatchApproval` server requests now
  also use `conversationId` for gateway scope enforcement, so hidden-thread
  legacy prompts are rejected instead of bypassing visibility checks; dedicated
  northbound websocket regressions now also pin the legacy approval round-trip
  transport for both methods
- legacy `getConversationSummary` requests now also use `rolloutPath` for
  gateway path-scope enforcement and multi-worker worker discovery, so hidden
  rollout summary reads cannot fall through to the primary worker by default
- legacy `gitDiffToRemote` requests now also use multi-worker worker discovery
  with the same required-worker availability guard as plugin fallback routing,
  so local git diff reads do not silently reflect only the primary worker's
  checkout; real northbound WebSocket coverage now pins both the steady-state
  route and the recovered-worker route
- deprecated `getAuthStatus` requests now also mirror the `account/read`
  multi-worker aggregation policy, so legacy auth-state reads preserve primary
  auth details, reflect worker-wide OpenAI auth requirements, and fail closed
  while any required worker remains in reconnect backoff
- fail-closed handling for a downstream session that reuses a still-pending
  server-request id now also emits a structured warning log with scope, worker
  id, the colliding request id/method, and the gateway request ids already
  pending on the connection; that collision log now also includes pending
  downstream request ids, affected thread ids, and pending worker ids
- gateway-owned teardown of still-pending downstream server requests now also
  emits structured warning logs with connection outcome, terminal detail,
  pending gateway request ids, per-scope counts, and worker ids, so
  disconnect-driven cleanup is visible without reconstructing the session from
  downstream errors alone
- multi-worker `thread/list` dedupe now also emits a structured log with scope,
  thread id, selected worker, discarded worker, their websocket URLs, and
  snapshot timestamps when the gateway chooses one visible thread copy over
  another, so cross-worker snapshot selection is observable without
  reproducing the merged response path
- missing multi-worker thread routes now also emit structured success/failure
  logs when the gateway probes downstream `thread/read` to recover ownership,
  including scope, thread id, recovered or attempted worker ids, and recovered
  or attempted worker websocket URLs, so lazy route recovery no longer depends
  on inferring behavior from later request routing alone
- missing multi-worker visible-thread routes now also fail closed while a
  required worker remains unavailable during reconnect backoff, so lazy
  `thread/read` ownership probes cannot accidentally treat the surviving
  workers' incomplete thread set as authoritative for an already-visible
  thread
- degraded multi-worker `thread/list` and `thread/loaded/list` discovery now
  emits structured warning logs with scope, available worker ids, unavailable
  worker ids, unavailable worker websocket URLs, and reconnect-backoff worker
  ids when the shared session serves the surviving workers' partial thread
  view
- multi-worker thread routing now also deduplicates repeated `thread/list`
  entries, backfills sticky ownership from the selected visible winner, probes
  downstream ownership to recover missing routes for already-visible threads,
  and has real northbound `RemoteAppServerClient` regression coverage for
  `thread/resume` and `thread/fork`, including sticky routing plus follow-up
  `thread/read` on the returned forked thread
- that same real multi-worker `RemoteAppServerClient` harness now also covers
  visible path-based `thread/resume` and `thread/fork` routing through the
  gateway scope policy, so those rollout-path flows are exercised through one
  shared multi-worker client session in addition to the embedded and
  single-worker remote compatibility harnesses
- dedicated northbound request-routing regression coverage now also verifies
  same-session sticky recovery for `thread/name/set` and
  `thread/memoryMode/set`, so thread-scoped mutations reconnect the missing
  owning worker before routing instead of drifting onto the surviving worker
- that same reconnect-on-demand coverage now also verifies `thread/fork`
  re-registers the returned thread id onto the recovered worker and that a
  follow-up `thread/read` stays sticky to that worker within the same shared
  session
- that same reconnect-on-demand coverage now also verifies `review/start`
  re-registers the detached review thread onto the recovered worker and that a
  follow-up `thread/read` of the review thread stays sticky to that worker on
  the same shared session

## Operator Guidance

This section documents the currently supported rollout profiles for the v2
compatibility surface.

### Profile 1: Embedded Gateway

Use this when:

- the gateway and app-server should run in one process
- you want the simplest drop-in v2 compatibility path
- local execution is acceptable, or you plan to delegate execution separately
  via `--exec-server-url`

Example:

```bash
cargo run -p codex-gateway -- \
  --runtime embedded \
  --listen 127.0.0.1:8080
```

Variant with a static northbound token:

```bash
cargo run -p codex-gateway -- \
  --runtime embedded \
  --listen 127.0.0.1:8080 \
  --bearer-token gateway-dev-token
```

Variant with out-of-process execution:

```bash
cargo run -p codex-gateway -- \
  --runtime embedded \
  --listen 127.0.0.1:8080 \
  --exec-server-url http://127.0.0.1:9000
```

Operational notes:

- northbound HTTP routes remain available alongside the v2 WebSocket endpoint
  at `/`
- v2 compatibility is fully enabled in this profile
- request scoping, auth, audit logs, metrics, and rate limiting apply to v2
  traffic the same way they apply to HTTP traffic
- `/healthz` exposes `v2Connections.activeConnectionCount`,
  `v2Connections.activeConnectionPendingClientRequestCount`,
  `v2Connections.activeConnectionMaxPendingClientRequestCount`,
  `v2Connections.activeConnectionPeakPendingClientRequestCount`,
  `v2Connections.activeConnectionPendingClientRequestStartedAt`,
  `v2Connections.activeConnectionPendingClientRequestWorkerCounts`,
  `v2Connections.activeConnectionPendingClientRequestMethodCounts`,
  `v2Connections.activeConnectionPendingServerRequestCount`,
  `v2Connections.activeConnectionAnsweredButUnresolvedServerRequestCount`,
  `v2Connections.peakActiveConnectionCount`,
  `v2Connections.totalConnectionCount`,
  `v2Connections.lastConnectionStartedAt`,
  `v2Connections.lastConnectionDurationMs`,
  `v2Connections.lastConnectionPendingClientRequestCount`,
  `v2Connections.lastConnectionMaxPendingClientRequestCount`,
  `v2Connections.lastConnectionPendingClientRequestStartedAt`,
  `v2Connections.lastConnectionPendingClientRequestWorkerCounts`,
  `v2Connections.lastConnectionPendingClientRequestMethodCounts`,
  `v2Connections.lastConnectionPendingServerRequestCount`, and
  `v2Connections.lastConnectionAnsweredButUnresolvedServerRequestCount`, plus
  `v2Connections.activeConnectionServerRequestBacklogCount`,
  `v2Connections.activeConnectionMaxServerRequestBacklogCount`,
  `v2Connections.activeConnectionPeakServerRequestBacklogCount`,
  `v2Connections.activeConnectionServerRequestBacklogStartedAt`,
  `v2Connections.activeConnectionServerRequestBacklogWorkerCounts`,
  `v2Connections.activeConnectionServerRequestBacklogMethodCounts`,
  `v2Connections.lastConnectionServerRequestBacklogCount`,
  `v2Connections.lastConnectionMaxServerRequestBacklogCount`,
  `v2Connections.lastConnectionServerRequestBacklogStartedAt`,
  `v2Connections.lastConnectionServerRequestBacklogWorkerCounts`,
  `v2Connections.lastConnectionServerRequestBacklogMethodCounts`,
  `v2Connections.accountCapacityEventCounts`,
  `v2Connections.accountCapacityEventWorkerCounts`,
  `v2Connections.lastAccountCapacityEvent`,
  `v2Connections.lastAccountCapacityEventWorkerId`,
  `v2Connections.lastAccountCapacityEventTenantId`,
  `v2Connections.lastAccountCapacityEventProjectId`,
  `v2Connections.lastAccountCapacityEventReason`,
  `v2Connections.lastAccountCapacityEventAt`, and the latest completed v2
  connection outcome/detail/timestamp for quick northbound
  compatibility-session diagnostics
- those active counters roll up all currently active v2 compatibility
  sessions, making live background client-request and server-request buildup
  visible before a session closes and updates the latest completed connection
  fields
- the per-worker backlog counts split pending and answered-but-unresolved
  server-request buildup by owning worker id, so multi-worker operators can
  identify which downstream app-server session is accumulating unresolved
  prompts without exposing individual request ids
- initialize timeout and downstream disconnects surface as explicit WebSocket
  close frames, not silent socket drops
- tune `--v2-initialize-timeout-seconds` and
  `--v2-client-send-timeout-seconds` when you need stricter or looser
  northbound timeout behavior during rollout
- v2 transport hardening values are validated at startup: initialize timeout,
  client-send timeout, reconnect retry backoff, max pending server-request
  count, and max pending client-request count must all be greater than zero
- tune `--v2-max-pending-server-requests` when you need a tighter or looser
  cap on unresolved server-request state for each northbound compatibility
  session
- tune `--v2-max-pending-client-requests` when you need a tighter or looser
  cap on long-running background client requests such as live `command/exec`
  calls for each northbound compatibility session

### Profile 2: Remote Gateway With One Worker

Use this when:

- app-server execution must stay outside the gateway process
- you need Stage A drop-in v2 compatibility today
- one northbound client connection mapping to one downstream worker session is
  acceptable

Example:

```bash
cargo run -p codex-gateway -- \
  --runtime remote \
  --listen 127.0.0.1:8080 \
  --remote-websocket-url ws://127.0.0.1:8081
```

Variant with worker auth and northbound auth:

```bash
cargo run -p codex-gateway -- \
  --runtime remote \
  --listen 127.0.0.1:8080 \
  --bearer-token gateway-dev-token \
  --remote-websocket-url ws://127.0.0.1:8081 \
  --remote-auth-token worker-dev-token
```

Operational notes:

- this is the recommended remote rollout profile for current v2 compatibility
- v2 compatibility is fully enabled in this profile
- existing remote-worker health and reconnect behavior still applies
- `/healthz` exposes the same `v2Connections` snapshot in this profile, so
  operators can see whether compatibility sessions are currently active and
  what the last completed session ended with
- that `v2Connections` snapshot now also has real remote single-worker
  regression coverage, pinning active, peak, and total connection counts plus
  the last-started and last-outcome fields across live, concurrent, and
  settled client sessions
- worker disconnects surface to the v2 client as explicit close frames with a
  gateway-owned reason
- the same `--v2-initialize-timeout-seconds` and
  `--v2-client-send-timeout-seconds` knobs apply to the northbound v2 session
  in this profile too
- `--v2-max-pending-server-requests` also applies in this profile when you need
  to bound unresolved server-request state more aggressively during rollout
- `--v2-max-pending-client-requests` also applies in this profile when you
  need to bound long-running background client requests independently from
  downstream prompt state
- new threads can fail over to a healthy worker at the HTTP layer in multi-
  worker remote mode, but that does not apply to a single v2 WebSocket session
  yet because the Stage A transport remains 1:1

### Profile 3: Remote Gateway With Multiple Workers

Current status:

- HTTP/SSE platform routes are supported
- northbound v2 WebSocket upgrades are supported in the current Stage B
  transport profile
- the gateway establishes one downstream app-server session per configured
  worker and exposes `RemoteMultiWorker` from `/healthz`
- the current supported surface includes gateway-owned aggregation, fanout,
  primary-worker routing, worker discovery, thread-sticky routing, translated
  server-request IDs, bounded account handoff for explicit restore surfaces, and
  fail-closed behavior for live active-context requests; the method matrix is
  the source of truth for which route class applies to each method in the
  validated build
- project-aware account routing is active but still short of the final
  release-quality multi-worker profile: project-scoped `thread/start` requests
  reuse recorded project-to-worker routes for cache affinity, different
  projects prefer less-loaded eligible account-backed workers, and remote
  `/healthz` exposes those cached routes as `projectWorkerRoutes`. The
  northbound v2 multi-worker router uses the same configured account labels for
  project-scoped `thread/start` distribution and skips account-backed workers
  already marked exhausted in the shared remote-worker health registry. The
  HTTP gateway now records downstream quota, rate-limit, billing, credits, and
  429 failures as account-capacity exhaustion signals and exposes that state in
  `/healthz`; HTTP `thread/start` quota failover also now emits
  `gateway_account_capacity_events` metrics and structured logs for exhausted
  workers plus successful replacement routes, and publishes `/v1/events`
  operator events for account exhaustion and replacement-worker selection. v2
  multi-worker `thread/start` now records the same quota-like JSON-RPC failures
  and retries new thread creation against another eligible account-backed
  worker. v2 multi-worker account capacity also updates from
  `account/rateLimits/read` responses and `account/rateLimits/updated`
  notifications, allowing the health view to recover when fresh rate-limit
  state no longer reports an exhausted bucket; repeated snapshots with the
  same available or exhausted state do not emit duplicate account-capacity
  events or inflate the health mirror.
  The v2 new-thread failover path also emits
  `gateway_v2_account_capacity_events` metrics and structured logs for
  exhausted workers plus successful replacement routes, including tenant /
  project scope, worker/account identity, and the downstream quota reason, and
  publishes the same `/v1/events` operator notifications as HTTP new-thread
  failover. v2 path-based `thread/resume`, `thread/fork`, and
  `getConversationSummary`
  requests now use rollout paths as a bounded restoration surface: when the
  cached path route points at an exhausted account-backed worker, the gateway
  skips that worker, attempts the same path request on another available
  worker, updates the route on success, and fails closed if no replacement can
  restore the context. The real multi-worker legacy compatibility harness now
  also validates this through `account/rateLimits/read` plus
  `getConversationSummary.rolloutPath` on an unmodified
  `RemoteAppServerClient` session, and the real multi-worker v2 compatibility
  harness now also validates successful path-based `thread/resume` and
  `thread/fork` replacement-account restoration with the matching
  `gateway/accountPathHandoffSucceeded` operator events. v2 multi-worker
  routing still does not
  transfer a live in-memory turn or pending server-request context to a new
  account when the current account runs out of quota. The same real
  multi-worker legacy harness now also watches the real `/v1/events` SSE
  stream and verifies that successful rollout-path restoration publishes
  `gateway/accountPathHandoffSucceeded`, and now also verifies that
  `getConversationSummary.conversationId` restores through a replacement
  account-backed worker with a `gateway/accountThreadHandoffSucceeded`
  operator event. Thread-scoped requests for already
  routed active threads still fail closed instead of silently moving live
  in-memory context, and that no-handoff decision now publishes
  `gateway/accountActiveThreadHandoffFailed` plus a
  `gateway_v2_account_capacity_events{event="active_thread_handoff_failure"}`
  metric. Explicit v2 `thread/resume` by thread id now treats the resume call
  itself as a bounded restoration surface: if the visible thread is pinned to
  an exhausted account-backed worker, the gateway attempts the same resume on
  another available account, accepts only a response for the requested thread
  id, updates sticky thread routing on success, emits
  `thread_resume_handoff_success` or `thread_resume_handoff_failure` account
  capacity metrics, and publishes `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed` operator events. Explicit v2
  `thread/fork` by thread id now uses that same bounded restoration surface
  for the visible source thread: replacement account success emits
  `thread_fork_handoff_success` and registers the forked thread route, while
  no-replacement failure emits `thread_fork_handoff_failure` and fails closed
  without moving the existing source-thread route. The real multi-worker legacy
  compatibility harness now validates the successful explicit thread-id resume
  restoration path through an unmodified
  `RemoteAppServerClient` session, watches `/v1/events` for the resulting
  `gateway/accountThreadHandoffSucceeded` event, and verifies a follow-up
  `thread/read` remains sticky to the replacement account-backed worker. The
  direct v2 `thread/read` path now also treats the visible thread id as a
  bounded restoration surface: if the cached owner account is exhausted, the
  gateway attempts the read on another available account, accepts only the
  requested thread id, updates sticky routing on success, and publishes
  `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed` with
  `thread_read_handoff_success` / `thread_read_handoff_failure` metrics. A
  real multi-worker `RemoteAppServerClient` harness now also covers the
  successful direct `thread/read` restoration path and operator event, and the
  real no-replacement harness covers the direct `thread/read` fail-closed
  branch plus `gateway/accountThreadHandoffFailed`. Direct v2
  `thread/name/set` and `thread/memoryMode/set` now use the same visible
  thread-id restoration surface for rename and memory-mode updates, updating
  sticky routing after a replacement worker successfully restores the request
  and emitting `thread_name_set_handoff_success` /
  `thread_name_set_handoff_failure` and
  `thread_memory_mode_set_handoff_success` /
  `thread_memory_mode_set_handoff_failure` metrics with account
  thread-handoff operator events. A real multi-worker
  `RemoteAppServerClient` harness now also covers the successful direct
  `thread/name/set` restoration path, verifies the
  `gateway/accountThreadHandoffSucceeded` operator event, and checks that a
  follow-up `thread/read` remains sticky to the replacement worker. The same
  real-client account-exhaustion coverage now also validates successful direct
  `thread/memoryMode/set` restoration and follow-up `thread/read` stickiness
  on the replacement worker. Direct v2
  `thread/unarchive` now follows the same bounded restoration rule when the
  cached owner account is exhausted: the gateway only accepts a replacement
  response for the requested thread id, updates sticky routing on success, and
  emits `thread_unarchive_handoff_success` /
  `thread_unarchive_handoff_failure` metrics with account thread-handoff
  operator events. The real multi-worker `RemoteAppServerClient` harness now
  also validates the successful direct `thread/unarchive` restoration path,
  verifies the `gateway/accountThreadHandoffSucceeded` operator event, and
  checks that a follow-up `thread/read` stays sticky to the replacement worker.
  The no-replacement branch is also covered through that real-client harness:
  when every eligible account-backed worker is exhausted, `thread/unarchive`
  fails closed and publishes `gateway/accountThreadHandoffFailed` without
  changing the cached route.
  Direct v2 `thread/archive` now follows the same bounded
  restoration rule for exhausted cached owner accounts, using the requested
  thread id to update sticky routing after the replacement worker successfully
  archives the thread and emitting `thread_archive_handoff_success` /
  `thread_archive_handoff_failure` metrics plus account thread-handoff
  operator events. The real multi-worker `RemoteAppServerClient` harness now
  also validates the successful direct `thread/archive` restoration path,
  verifies the `gateway/accountThreadHandoffSucceeded` operator event, and
  checks that a follow-up `thread/read` stays sticky to the replacement worker.
  The no-replacement branch is also covered through that real-client harness:
  when every eligible account-backed worker is exhausted, `thread/archive`
  fails closed and publishes `gateway/accountThreadHandoffFailed` without
  changing the cached route.
  Direct v2 `thread/turns/list` now follows the same bounded
  restoration rule for history pagination, using the requested thread id to
  update sticky routing after the replacement worker successfully returns the
  page and emitting `thread_turns_list_handoff_success` /
  `thread_turns_list_handoff_failure` metrics plus account thread-handoff
  operator events. The real multi-worker `RemoteAppServerClient` harness now
  also validates the successful direct `thread/turns/list` restoration path,
  verifies the `gateway/accountThreadHandoffSucceeded` operator event, and
  checks that a follow-up `thread/read` stays sticky to the replacement
  worker; the no-replacement harness now also verifies `thread/turns/list`
  fails closed with `gateway/accountThreadHandoffFailed` when every eligible
  account-backed worker is exhausted.
  Direct v2 `thread/increment_elicitation` now follows the
  same bounded restoration rule for elicitation counter updates, using the
  requested thread id to update sticky routing after the replacement worker
  successfully increments the counter and emitting
  `thread_increment_elicitation_handoff_success` /
  `thread_increment_elicitation_handoff_failure` metrics plus account
  thread-handoff operator events. The real multi-worker
  `RemoteAppServerClient` harness now also validates the successful direct
  `thread/increment_elicitation` restoration path, verifies the
  `gateway/accountThreadHandoffSucceeded` operator event, and checks that a
  follow-up `thread/read` stays sticky to the replacement worker; the
  no-replacement harness now also verifies `thread/increment_elicitation`
  fails closed with `gateway/accountThreadHandoffFailed` when every eligible
  account-backed worker is exhausted. Direct v2
  `thread/decrement_elicitation` and
  `thread/inject_items` now use the same requested-thread-id restoration
  surface, updating sticky routing after a replacement worker successfully
  restores the visible counter update or item injection and emitting
  `thread_decrement_elicitation_handoff_success` /
  `thread_decrement_elicitation_handoff_failure` and
  `thread_inject_items_handoff_success` /
  `thread_inject_items_handoff_failure` metrics with account thread-handoff
  operator events. The real multi-worker `RemoteAppServerClient` harness now
  also validates the successful direct `thread/decrement_elicitation` and
  `thread/inject_items` restoration paths, verifies the
  `gateway/accountThreadHandoffSucceeded` operator events, and checks that
  follow-up `thread/read` requests stay sticky to the replacement worker; the
  no-replacement harness now also verifies both methods fail closed with
  `gateway/accountThreadHandoffFailed` when every eligible account-backed
  worker is exhausted.
  Direct v2 `thread/metadata/update` now follows the same
  bounded restoration rule for exhausted cached owner accounts, accepting only
  a replacement response for the requested thread id and emitting
  `thread_metadata_update_handoff_success` /
  `thread_metadata_update_handoff_failure` metrics plus the same account
  thread-handoff operator events. The real multi-worker
  `RemoteAppServerClient` harness now also validates the successful direct
  `thread/metadata/update` restoration path, verifies the
  `gateway/accountThreadHandoffSucceeded` operator event, and checks that a
  follow-up `thread/read` stays sticky to the replacement worker. The
  no-replacement branch is also covered through that real-client harness: when
  every eligible account-backed worker is exhausted,
  `thread/metadata/update` fails closed and publishes
  `gateway/accountThreadHandoffFailed` without changing the cached route.
  Direct v2
  `thread/rollback` now follows the
  same bounded restoration rule for exhausted cached owner accounts, accepting
  only a replacement response for the requested thread id and emitting
  `thread_rollback_handoff_success` /
  `thread_rollback_handoff_failure` metrics plus the same account
  thread-handoff operator events; real multi-worker `RemoteAppServerClient`
  harnesses now cover both successful direct rollback restoration and
  no-replacement fail-closed behavior. Active thread-scoped requests without an
  explicit restore surface remain deliberately fail-closed when the owning
  account-backed worker is exhausted; regression coverage now pins that
  behavior for `turn/start`, `turn/steer`, `turn/interrupt`,
  thread-scoped `app/list`, `thread/unsubscribe`, `thread/compact/start`,
  `thread/shellCommand`, `thread/backgroundTerminals/clean`,
  `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`,
  `mcpServer/resource/read`, `mcpServer/tool/call`, `review/start`, and client
  replies to pending thread-scoped approval / user-input / elicitation server
  requests, rather than silently replaying live or side-effecting work on a
  different account. The server-request answer branch now also has real
  northbound WebSocket coverage proving that an answer sent after the owning
  worker account is marked exhausted closes the shared client session, is not
  delivered to the exhausted downstream worker, and records matching
  account-capacity plus server-request lifecycle metrics.
  The same harness now also validates the successful explicit thread-id fork
  restoration path through the same real client session and operator-event
  stream. The same harness now also validates the fail-closed branches for
  explicit thread-id resume and fork when no replacement account-backed worker
  has capacity, including the `gateway/accountThreadHandoffFailed` operator
  events.

Example:

```bash
cargo run -p codex-gateway -- \
  --runtime remote \
  --listen 127.0.0.1:8080 \
  --remote-websocket-url ws://127.0.0.1:8081 \
  --remote-account-id acct-a \
  --remote-websocket-url ws://127.0.0.1:8082 \
  --remote-account-id acct-b
```

Operational notes:

- this profile is valid for the existing HTTP/SSE gateway API
- this profile now admits northbound v2 WebSocket sessions instead of failing
  closed at upgrade time
- current multi-worker v2 routing is still an incremental Stage B transport:
  use it for development and targeted validation, not broad drop-in rollout
- before a specific multi-worker deployment is described as release-quality,
  validate the method route classes in the matrix for that build: aggregation,
  fanout, primary-worker affinity, worker discovery, thread-sticky routing,
  bounded account handoff, and fail-closed live active-context behavior
- remaining release work is to repeat that validation under the deployment's
  steady-state, reconnect, degraded-route, slow-client, overload, and
  account-capacity conditions, then keep the promotion scoped to the exact
  worker topology, account labels, auth mode, timeout settings,
  pending-request limits, and v2 method families that were exercised
- account-backed worker pools have moved beyond identity observability:
  workers can be labeled with an account id for health and affinity
  inspection, account capacity is tracked separately from worker process
  health, and HTTP `thread/start` uses those labels to prefer less-loaded
  accounts for different projects in the same tenant. The northbound v2
  multi-worker router also uses those labels for project-scoped `thread/start`
  distribution and avoids account-backed workers already marked exhausted in
  the shared remote-worker health registry. HTTP and v2 multi-worker
  `thread/start` mark downstream quota-like failures as account-capacity
  exhaustion and avoid exhausted account-backed workers for later selection;
  HTTP now emits `gateway_account_capacity_events` metrics and structured logs
  plus `/v1/events` operator notifications for bounded new-thread failover, v2
  also retries the same new thread request against another eligible account
  when one is available, emits matching `/v1/events` operator notifications for
  exhaustion and replacement-worker selection, and refreshes account-capacity
  state from
  `account/rateLimits/read` / `account/rateLimits/updated` when workers report
  current rate-limit buckets. `/healthz.v2Connections` now also exposes
  `accountCapacityEventCounts`, a cumulative by-event summary of v2 account
  exhaustion, replacement-handoff, and active no-handoff decisions,
  `accountCapacityEventWorkerCounts` for the per-worker breakdown, plus
  `lastAccountCapacityEvent`, `lastAccountCapacityEventWorkerId`,
  `lastAccountCapacityEventTenantId`, `lastAccountCapacityEventProjectId`,
  `lastAccountCapacityEventReason`, and `lastAccountCapacityEventAt` so
  operators who are not scraping metrics or attached to the live `/v1/events`
  stream can identify the newest event, the affected worker, the gateway
  tenant/project scope, the reason, and whether it is recent. The HTTP
  operator-event path is covered by a real remote-runtime regression that
  watches `/v1/events` during quota failover.
  HTTP `turn/start` also fails closed when a visible thread is already pinned
  to an exhausted account-backed worker, emitting
  `gateway_account_capacity_events{event="active_thread_handoff_failure"}` and
  `gateway/accountActiveThreadHandoffFailed` instead of moving live context;
  downstream quota-like `turn/start` failures mark that owning account-backed
  worker exhausted and publish `gateway/accountCapacityExhausted` for later
  active-thread requests. HTTP `turn/interrupt` now uses the same exhausted
  account guard and operator event for active turn control. HTTP
  `serverRequest/respond` also records the pending request thread id and fails
  closed with the same operator event before forwarding approval or user-input
  responses to an exhausted account-backed worker; this is covered by a real
  remote HTTP regression that first marks another same-account worker exhausted
  through a 429 `thread/start` response. The HTTP gateway also keeps pending
  server requests registered until the downstream response is successfully
  forwarded, so route or transport failures do not silently drop partially
  completed approval or user-input lifecycles. Those HTTP delivery failures
  now also emit `gateway_server_request_lifecycle_events` for answered and
  delivery-failed stages plus
  `gateway_server_request_answer_delivery_failures`, with structured route
  diagnostics in warning logs; embedded and remote runtime paths both keep the
  pending request registered until the downstream response is accepted. HTTP
  `serverRequest/respond` now also checks exhausted account capacity directly
  from the pending server-request worker route, preserving fail-closed behavior
  even if the visible thread route is missing; that fail-closed branch now also
  emits the same answered / delivery-failed lifecycle metrics and direct
  answer-delivery-failure counter as other HTTP response delivery failures.
  HTTP `serverRequest/respond` now also emits
  `client_server_request_invalid_response` lifecycle metrics and structured
  tenant/project logs when a client answers with a response type that does not
  match the pending request, while keeping that pending request registered.
  HTTP `thread/read` now also treats a visible thread id as a bounded account
  restoration surface: if the cached route points at an exhausted
  account-backed worker, the gateway attempts the read on another healthy
  worker with available account capacity, updates the sticky route on success,
  and publishes `gateway/accountThreadHandoffSucceeded`; if no replacement
  restores the read, or if a replacement returns a different thread id than
  the requested one, it records `thread_read_handoff_failure` account-capacity
  metrics and publishes `gateway/accountThreadHandoffFailed`.
  `/healthz` now exposes `pendingServerRequestCount` and
  `pendingServerRequestKindCounts` for the gateway HTTP/SSE surface, so
  operators can see partially completed approval and user-input lifecycles
  while they are waiting for successful downstream delivery or explicit
  resolution, without exposing individual request ids. `/healthz` also exposes
  `pendingServerRequestRouteCounts`, grouped by owning worker route and
  response kind, so operators can identify worker-local buildup without
  exposing individual request ids. `/healthz` now also exposes
  `pendingServerRequestOldestAt`, giving operators the Unix timestamp of the
  oldest partially completed HTTP/SSE server-request lifecycle without
  exposing individual request ids. v2
  new-thread failover emits
  `gateway_v2_account_capacity_events` and structured logs for exhausted
  workers plus successful replacement routes. Thread-scoped v2 requests for
  already-routed threads fail closed when the owning worker's account is
  marked exhausted, so active context is not silently moved without an explicit
  resume or handoff path; that fail-closed path is covered by
  `gateway_v2_account_capacity_events` with
  `active_thread_handoff_failure`, publishes
  `gateway/accountActiveThreadHandoffFailed` on `/v1/events`, and is also
  covered by the same `gateway_v2_fail_closed_requests` metric and structured
  route diagnostic log used for unavailable worker routes, including when all
  downstream worker sessions are still connected. Path-based `thread/resume`,
  `thread/fork`, and
  `getConversationSummary` can now restore through another account-backed
  worker when the visible rollout path is recoverable; successful replacement
  routes emit `gateway_v2_account_capacity_events` with
  `path_thread_handoff_success` and structured logs with tenant/project scope,
  old worker/account identity, replacement worker/account identity, and the
  path, and publish `gateway/accountPathHandoffSucceeded` on `/v1/events`.
  Failed restoration emits `path_thread_handoff_failure` with the exhausted
  worker/account identity and the path, publishes
  `gateway/accountPathHandoffFailed` on `/v1/events`, and then fails closed.
  For path-based `thread/resume` and `getConversationSummary.rolloutPath`, a
  replacement worker must return the same rollout path that the client asked to
  restore; a different path is treated as a failed restoration and does not
  update the cached route.
  The real multi-worker compatibility harnesses now validate successful
  path-based `thread/resume`, path-based `thread/fork`, and
  `getConversationSummary.rolloutPath` replacement-account restoration through
  unmodified `RemoteAppServerClient` sessions; those same rollout-path
  surfaces now also have no-replacement fail-closed real-client coverage with
  `gateway/accountPathHandoffFailed` operator events.
  Legacy `getConversationSummary.conversationId` now also uses the visible
  thread id as a bounded restoration surface. If the cached owning account is
  exhausted, the gateway attempts the summary request on another available
  account, accepts only a summary for the requested conversation id, records
  `conversation_summary_handoff_success` and updates the returned
  conversation/path route on success, or records
  `conversation_summary_handoff_failure` and publishes
  `gateway/accountThreadHandoffFailed` when no replacement account can restore
  the summary. The real multi-worker legacy compatibility harness now covers
  both the successful replacement-account path and the no-replacement
  fail-closed path for this conversation-id form.
  Today's reconnect and v2 multi-worker logic should not be treated as full
  live-context-preserving quota failover
- future account-aware routing must preserve tenant/project scope while adding
  cache-affinity selection for same-project clients, policy-based distribution
  across accounts for different projects, and explicit context restoration
  when quota exhaustion requires a handoff to a replacement account
- `/healthz` exposes the effective v2 transport hardening config
  (`initializeTimeoutSeconds`, `clientSendTimeoutSeconds`,
  `reconnectRetryBackoffSeconds`, `maxPendingServerRequests`, and
  `maxPendingClientRequests`) so operators can confirm rollout settings
  without relying only on startup logs
- startup validation rejects zero-valued v2 transport hardening settings in
  this profile as well, so a multi-worker rollout cannot accidentally disable
  handshake, slow-client, reconnect-backoff, pending-prompt, or pending-client
  bounds
- `/healthz` also exposes the same `v2Connections` snapshot in this profile,
  and that connection-health view now has real multi-worker regression
  coverage for live, concurrent, and settled northbound client sessions
- `/healthz` remote-worker entries now also expose `lastStateChangeAt` and
  `lastErrorAt`, so operators can distinguish a worker that is actively
  flapping from one that is healthy again but still carries a historical
  `lastError`
- `/healthz` remote-worker entries also expose `reconnecting`,
  `reconnectAttemptCount`, `nextReconnectAt`, and
  `reconnectBackoffRemainingSeconds`, so operators can tell whether a remote
  worker is still in background reconnect backoff, how many reconnect attempts
  have already failed in the current loop, how long the next retry is still
  delayed, and whether the worker is only carrying a stale unhealthy flag
- remote `/healthz` responses expose `projectWorkerRoutes` with
  `tenantId`, `projectId`, `workerId`, `accountId`, and `workerHealthy` for the
  current project-affinity table, so operators can inspect cache-affinity
  routing decisions separately from account quota routing while still seeing
  whether a cached route currently points at a healthy worker; `accountId`
  reflects the corresponding configured `--remote-account-id` value when one
  is supplied
- remote-worker entries in `/healthz` also include `accountId`, giving
  operators the same account label on raw worker health and project-affinity
  route views that the quota-aware selection and bounded handoff paths use
- remote-worker entries in `/healthz` now include `accountCapacity`,
  `accountCapacityReason`, and `accountCapacityLastChangedAt`; project-affinity
  route entries also include `accountCapacity`, making it visible when a cached
  project route points at a currently exhausted account-backed worker
- HTTP `thread/start` regression coverage now verifies that a cached
  same-project route to an unhealthy worker is bypassed in favor of another
  healthy worker, and that the `projectWorkerRoutes` entry is updated after the
  successful replacement route is established
- remote-worker reconnect-backoff health now has direct runtime and HTTP
  health regression coverage, pinning `reconnectBackoffRemainingSeconds` for
  reconnecting workers and `null` for healthy workers
- remote worker disconnects, reconnect attempts, reconnect failures, and
  successful reconnects now also emit structured gateway logs with worker id,
  websocket URL, and retry timing so rollout debugging does not depend only on
  repeated `/healthz` polling
- northbound v2 connections now also emit structured gateway logs with
  connection outcome, scope, duration, and terminal error detail when present,
  while `/healthz` exposes the same latest completed connection duration via
  `v2Connections.lastConnectionDurationMs`, so rollout debugging can
  distinguish slow-client, handshake, and downstream session failures without
  depending only on metrics or audit-enabled logs
- those connection completion logs and audit logs also include terminal detail
  plus pending client-request, pending server-request, and
  answered-but-unresolved server-request counts, matching the `/healthz` v2
  connection snapshot and connection metrics used to diagnose background
  command saturation and stranded prompt lifecycles
- `/healthz.v2Connections` also exposes active and last-completed
  server-request backlog worker counts, grouping pending and
  answered-but-unresolved prompt lifecycle buildup by owning worker id so
  multi-worker rollouts can correlate backlog pressure with downstream worker
  health and reconnect state
- `/healthz.v2Connections` also exposes active and last-completed pending
  client-request worker and method counts, so long-running background requests
  such as `command/exec` can be correlated with the owning worker route and
  client method before or after a slow-client teardown
- connection-level metrics now also include
  `gateway_v2_connection_pending_client_requests{outcome}`, so post-teardown
  dashboards can track background client-request buildup with the same outcome
  tags used for pending and answered-but-unresolved server-request lifecycles
- connection teardown also logs any aborted pending background client requests
  with tenant/project scope, terminal outcome/detail, request ids, methods, and
  downstream worker ids / WebSocket URLs, so operators can correlate partially
  completed `command/exec` lifecycles with the matching connection outcome and
  owning app-server session
- if downstream app-server shutdown also fails after a gateway v2 connection
  error, teardown now emits a structured warning with tenant/project scope,
  terminal outcome/detail, pending client-request count, pending client
  request ids / methods / worker routes, pending server-request count,
  answered-but-unresolved server-request count, backlog worker and method
  count summaries, and the shutdown error detail, and increments
  `gateway_v2_downstream_shutdown_failures{outcome}` so cleanup failures are
  visible as a direct rollout signal
- completed background client requests are settled before the gateway attempts
  to send their final northbound JSON-RPC response, so slow-client teardown
  diagnostics only report truly still-pending `command/exec` requests rather
  than requests that already completed downstream; direct regression coverage
  pins the pending-count and active-route cleanup invariant
- if final delivery of that completed background response fails because the
  northbound client times out or disconnects, the gateway records
  `gateway_v2_requests{method,outcome}` with the connection-owned failure
  outcome before closing the session, so completed-but-undelivered
  `command/exec` calls stay visible in per-method telemetry
- connection teardown now also drains already completed background responses
  before aborted-request diagnostics are emitted, so a `command/exec` that
  finished concurrently with northbound disconnect is removed from active
  pending state first
- aborted background client requests also record
  `gateway_v2_requests{method,outcome}` with the connection terminal outcome,
  so dashboards can separate successful, rate-limited, and interrupted
  long-running `command/exec` calls at the request-metric boundary
- duplicate in-flight background client-request ids now fail the northbound v2
  connection closed with a gateway-owned protocol close reason before the
  pending route can be overwritten, even if the duplicate id is used by a
  different follow-up method; regression coverage pins the protocol-violation
  metric and duplicate-request log fields, including the original worker id and
  WebSocket URL
- that duplicate pending client-request path now also records the offending
  follow-up request as `gateway_v2_requests{method,outcome="protocol_violation"}`,
  and emits the matching request audit log, so request-level telemetry and
  audit trails identify which method reused the in-flight id
- aborted background `command/exec` requests now also have direct audit-log
  coverage for the per-request terminal outcome, so client disconnect,
  protocol-violation, and slow-client teardown paths stay visible at the same
  request boundary as successful and rejected command requests
- primary-worker fail-closed routing for background `command/exec` now also
  records `gateway_v2_requests{method="command/exec",outcome="internal_error"}`
  and emits the matching request audit log before the northbound connection
  ends, so route failures for standalone command execution remain visible in
  per-method telemetry
- primary-worker route failures for ordinary request/response methods now also
  have real northbound WebSocket coverage for the matching
  `gateway_v2_requests{method,outcome="internal_error"}` metric and request
  audit log, so degraded primary-worker setup reads are visible at the same
  request boundary as background command execution
- `/healthz.v2Connections` also exposes active-session pending client-request
  totals plus pending and answered-but-unresolved server-request totals, so
  background client-request saturation and multi-worker prompt lifecycle
  buildup are visible during a live shared session rather than only after
  teardown
- saturated and hidden-thread downstream server-request rejections also
  increment the `gateway_v2_server_request_rejections` counter with `method`
  and `reason` tags; current reasons are `pending_limit` and `hidden_thread`.
  The matching rejection logs include tenant/project scope plus the affected
  worker websocket URL, and saturated-connection logs include the configured
  pending limit plus pending gateway/downstream server-request ids and affected
  thread / worker ids.
- multi-worker remote reconnect loops increment
  `gateway_v2_worker_reconnects` with `worker_id` and `outcome` tags; current
  outcomes are `attempt`, `success`, `connect_failure`, `replay_failure`, and
  `backoff_suppressed`. Retry-backoff suppression also emits a structured
  warning log with worker id, worker websocket URL, replay-state context, and
  both configured and remaining backoff seconds.
- multi-worker remote requests that fail closed because required worker routes
  are unavailable increment `gateway_v2_fail_closed_requests` with `method`
  and `reconnect_backoff_active` tags, matching the structured warning log
  emitted for the same event; the warning log includes available,
  unavailable, and reconnect-backoff worker websocket URLs plus
  reconnect-backoff remaining seconds, including paired worker id / URL /
  remaining-second route diagnostics for each backoff worker
- multi-worker remote upstream request failures that happen while worker routes
  are unavailable increment `gateway_v2_upstream_request_failures` with
  `method` and `reconnect_backoff_active` tags, so operators can separate
  downstream/request failures from gateway-owned fail-closed routing policy
- degraded multi-worker `thread/list` and `thread/loaded/list` requests
  increment `gateway_v2_degraded_thread_discovery` with `method` and
  `reconnect_backoff_active` tags, matching the structured warning log emitted
  when the gateway intentionally serves the surviving workers' partial visible
  thread set; the same warning log includes available, unavailable, and
  reconnect-backoff worker websocket URLs plus reconnect-backoff remaining
  seconds, including paired worker id / URL / remaining-second route
  diagnostics, so operators can identify the affected downstream sessions
  directly
- primary-worker-only multi-worker requests now also have method-family
  reconnect-backoff coverage for config requirements, managed login, login
  cancellation, add-credits nudge email, feedback upload, standalone command
  control, basic filesystem operations, streaming fuzzy file-search sessions,
  and Windows sandbox setup, so the fail-closed metric and no-fallback behavior
  are pinned across the full primary route set
- suppressed multi-worker notification dedupe increments
  `gateway_v2_suppressed_notifications` with `method` and `reason` tags,
  matching the structured warning logs emitted when duplicate connection-state
  notifications, repeated `skills/changed` invalidations, opted-out
  notifications, or hidden-thread notifications are dropped; exact-duplicate
  connection-state suppression logs include both the dropped worker route and
  the original forwarded worker route, with real northbound WebSocket coverage
  now pinning tenant/project scope, both worker websocket URLs, notification
  method, and payload context for exact-duplicate `warning` suppression;
  opted-out suppression logs include tenant/project scope, affected worker URL,
  notification method, and payload context, and hidden-thread suppression logs
  include tenant/project scope,
  affected worker URL, notification method, hidden thread id, and payload
  context
- forwarded downstream v2 notifications now also increment
  `gateway_v2_forwarded_notifications{method}` after successful northbound
  delivery, with multi-worker fan-in count coverage plus opt-out and
  hidden-thread negative coverage ensuring suppressed notifications do not
  inflate delivered fan-in during rollout
- failed northbound delivery of downstream v2 notifications now increments
  `gateway_v2_notification_send_failures{method,outcome}`, so a terminal
  notification send failure can be attributed to the downstream notification
  family as well as the gateway-owned connection outcome
- failed downstream notification delivery now also emits a structured warning
  with tenant/project scope, worker id, worker websocket URL, method, outcome,
  and error detail for direct rollout diagnosis
- failed northbound delivery of downstream v2 server requests now emits
  `event=downstream_server_request_forward_delivery_failed` on the
  `gateway_v2_server_request_lifecycle_events` counter, plus a structured
  warning with tenant/project scope, worker route, method, outcome, and error
  detail
- those downstream server-request forward delivery failures also increment
  `gateway_v2_server_request_forward_send_failures{method,outcome}`, so
  rollout dashboards can alert directly on prompt / elicitation delivery
  failures without filtering the broader lifecycle event stream
- client answers to forwarded server requests that fail during downstream
  delivery now also increment
  `gateway_v2_server_request_answer_delivery_failures{response_kind}`, so
  answered-but-not-delivered prompt / elicitation replies have a direct rollout
  counter in addition to the broader lifecycle event stream
- gateway-owned server-request rejections that fail during downstream delivery
  now also increment
  `gateway_v2_server_request_rejection_delivery_failures{method}`, so prompt /
  elicitation rejections that never reach the owning worker have a direct
  rollout counter in addition to the broader lifecycle event stream
- failed northbound delivery of gateway v2 client request responses now emits
  a structured warning with tenant/project scope, JSON-RPC request id, method,
  outcome, and error detail so response-path send failures can be correlated
  with the existing per-request outcome metric
- failed northbound delivery of gateway v2 client request responses also
  increments `gateway_v2_client_response_send_failures{method,outcome}`,
  including initialize handshake responses, ordinary request responses, and
  background `command/exec` completions, so response delivery failures have a
  direct rollout counter
- dedicated northbound WebSocket regression coverage now also exercises
  ordinary success and error client-response send-failure paths, so that direct
  counter is pinned at the compatibility boundary and not only by lower-level
  helper coverage
- failed delivery of gateway-owned v2 close frames increments
  `gateway_v2_close_frame_send_failures{code,outcome}`, so rollout dashboards
  can separate terminal policy/protocol close delivery failures from ordinary
  response, notification, and server-request send failures; those failures now
  also emit structured warning logs with tenant/project scope, the close code,
  protocol-safe reason, outcome, and send error detail
- dedicated northbound WebSocket regression coverage now also exercises that
  close-frame send-failure path at the compatibility boundary, including the
  direct counter and structured warning log fields for a gateway-owned policy
  close
- failed delivery of synthesized worker-cleanup `serverRequest/resolved`
  notifications now also increments
  `gateway_v2_notification_send_failures{method="serverRequest/resolved",outcome}`
  and records
  `gateway_v2_server_request_lifecycle_events{event="worker_cleanup_resolution_send_failed",method}`
  with the original server-request method; the cleanup warning includes
  tenant/project scope plus prompt method, so stranded thread-scoped prompt
  dismissal failures are visible through notification telemetry and remain
  distinguishable by prompt family in lifecycle metrics
- degraded multi-worker thread discovery now has regression coverage asserting
  that each `thread/list` or `thread/loaded/list` request emits only one
  gateway-owned degraded-route warning and one
  `gateway_v2_degraded_thread_discovery` metric point, keeping reconnect
  backoff dashboards tied to request volume rather than duplicate log paths
- duplicate downstream `serverRequest/resolved` replays increment
  `gateway_v2_server_request_lifecycle_events` with `event` and `method`
  tags, matching the structured warning log emitted when the translated route
  has already been drained
- downstream server requests that reuse a still-pending gateway request id also
  increment `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` and the colliding server-request method,
  matching the structured warning log emitted before the gateway closes the
  session; that warning log includes the pending gateway/downstream request ids,
  affected thread ids, and pending worker ids present at collision time
- worker-loss cleanup increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=worker_cleanup_resolved_thread_scoped` and
  `event=worker_cleanup_stranded_connection_scoped`, tagged by the original
  server-request method; the counters are emitted immediately after route
  cleanup, before any synthetic resolution notification is sent to the client,
  and the matching warning logs include affected thread ids for the
  thread-scoped prompts plus the affected worker websocket URL and prompt
  method when synthesized-resolution delivery fails. Successful and failed
  synthesized-resolution delivery lifecycle events are also tagged by the
  original server-request method, while the direct notification send-failure
  counter remains tagged as `method="serverRequest/resolved"` because that is
  the notification being sent northbound
- client-side connection cleanup increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=client_cleanup_rejected_thread_scoped`,
  `event=client_cleanup_rejected_connection_scoped`, and
  `event=client_cleanup_answered_but_unresolved`, tagged by the original
  server-request method; the matching cleanup warning includes gateway request
  ids, downstream request ids, affected thread ids, worker ids, worker
  websocket URLs, and method lists for the pending prompts being rejected, plus
  affected thread ids and methods for answered-but-unresolved routes
- once the gateway attempts to reject pending prompts during client-side
  cleanup, successful deliveries increment
  `event=client_cleanup_rejection_delivered`, failed deliveries increment
  `event=client_cleanup_rejection_failed`, and unavailable worker routes
  increment `event=client_cleanup_rejection_skipped_unavailable_worker`, all
  tagged by the original server-request method
- failed or skipped client-side cleanup rejection deliveries now also increment
  `gateway_v2_server_request_rejection_delivery_failures{method}`, using the
  original server-request method, so teardown-time prompt cleanup handoff
  failures and unavailable-worker skips have the same direct alerting signal as
  ordinary gateway-owned server-request rejection delivery failures
- normal client replies to forwarded server requests increment
  `gateway_v2_server_request_lifecycle_events` with
  `event=client_server_request_answered` and `method=response` or
  `method=error`, so operators can distinguish prompts that reached the
  downstream answered-but-unresolved stage from prompts still pending on the
  northbound client
- downstream delivery of those client replies now also increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=client_server_request_delivered` or
  `event=client_server_request_delivery_failed`, so a reply that reached the
  gateway but could not be handed back to the owning app-server worker is
  visible as its own partially completed lifecycle stage; direct regression
  coverage now pins the delivery-failure branch when the owning worker route is
  no longer available
- that delivery-failure branch now also emits a structured warning log with
  tenant/project scope, response kind, worker id, worker websocket URL,
  gateway request id, downstream request id, thread id, and error detail, so
  operators can diagnose answered-but-not-delivered prompt lifecycles without
  reconstructing the route table from metrics alone
- that same answered-but-not-delivered branch now also increments
  `gateway_v2_server_request_answer_delivery_failures{response_kind}`, so
  prompt / elicitation replies that reached the gateway but failed on the
  downstream handoff have a direct alerting counter alongside the broader
  lifecycle event stream
- pending server-request routes now snapshot the worker websocket URL when the
  prompt is forwarded, so answered-but-undeliverable and cleanup diagnostics
  retain the original downstream route even after the worker route disappears
- pending server-request log summaries now also include
  `pending_worker_websocket_urls` alongside `pending_worker_ids` for unexpected
  client replies, saturation, downstream backpressure, slow-client timeout, and
  downstream protocol violation plus duplicate downstream request-id
  diagnostics, so operator logs retain the original worker routes across prompt
  lifecycle failures
- pending server-request log summaries now also include
  `pending_server_request_methods`, and answered-but-unresolved summaries
  include `answered_but_unresolved_server_request_methods`, so teardown and
  failure diagnostics identify the same prompt method families that
  `/healthz.v2Connections` reports in its server-request backlog method counts
- slow-client, downstream-backpressure, unexpected client-reply, and
  client-side cleanup prompt summaries now also emit
  `server_request_backlog_count` alongside pending and
  answered-but-unresolved prompt counts, so teardown diagnostics stay aligned
  with `/healthz.v2Connections.*ServerRequestBacklogCount`
- worker-loss cleanup diagnostics now also include
  `resolved_thread_scoped_server_request_methods` for synthesized
  `serverRequest/resolved` notifications and
  `stranded_connection_scoped_server_request_methods` for connection-scoped
  prompts that force the northbound session closed, so worker disconnect logs
  retain the prompt family as well as the gateway and downstream request ids
- v2 connection completion and audit logs now include
  `server_request_backlog_count`,
  `server_request_backlog_worker_counts` and
  `server_request_backlog_method_counts`, keeping normal connection teardown
  diagnostics aligned with the `/healthz.v2Connections` backlog summaries
- last-completed `/healthz.v2Connections` server-request backlog timestamps use
  the connection completion time as a conservative fallback when terminal
  pending counts are first observed during teardown, so nonzero completed
  prompt backlog snapshots still carry a timestamp for rollout diagnostics
- connection metrics now also export
  `gateway_v2_connection_pending_client_requests_by_worker`,
  `gateway_v2_connection_pending_client_requests_by_method`,
  `gateway_v2_connection_max_pending_client_requests`,
  `gateway_v2_connection_server_request_backlog`,
  `gateway_v2_connection_pending_server_requests_by_worker`,
  `gateway_v2_connection_answered_but_unresolved_server_requests_by_worker`,
  `gateway_v2_connection_server_request_backlog_by_worker`,
  `gateway_v2_connection_pending_server_requests_by_method`,
  `gateway_v2_connection_answered_but_unresolved_server_requests_by_method`,
  `gateway_v2_connection_server_request_backlog_by_method`, and
  `gateway_v2_connection_max_server_request_backlog`, tagged with `outcome`,
  `worker_id`, or `method`, so terminal background client-request and combined
  prompt backlog can be split by worker route, command, approval, user-input,
  elicitation, or token-refresh family in metrics dashboards while lifecycle
  peak pressure remains visible without recomputing totals from separate
  pending and answered-but-unresolved series
- downstream-shutdown failure logs now also include pending-client worker and
  method count summaries alongside the individual pending background request
  ids, methods, worker ids, and worker WebSocket URLs, keeping that failure
  path aligned with `/healthz.v2Connections` and terminal connection metrics
- downstream-shutdown failure logs now also include
  `server_request_backlog_count` alongside pending and
  answered-but-unresolved prompt counts, keeping that failure path aligned with
  `/healthz.v2Connections.*ServerRequestBacklogCount`
- pending-client abort logs now also include worker and method count summaries
  alongside individual pending background request ids, methods, worker ids,
  and worker WebSocket URLs, so aborted `command/exec` or other long-running
  requests can be reconciled with health snapshots and connection metrics
- answered-but-unresolved server-request log summaries now also include
  `answered_but_unresolved_worker_websocket_urls`, and duplicate
  `serverRequest/resolved` replay diagnostics include the remaining resolved
  route URLs, so operators can identify the original worker route even after a
  client answer has already drained the pending prompt entry
- downstream server requests that pass gateway scope and saturation checks
  increment `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_forwarded` and the forwarded request method,
  so prompt lifecycle telemetry now marks when a prompt actually reached the
  northbound client
- downstream server requests rejected by gateway saturation or scope policy
  increment `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_rejected_pending_limit` or
  `event=downstream_server_request_rejected_hidden_thread`, so rejected prompts
  remain visible in the same lifecycle counter as forwarded, answered, and
  resolved prompts
- delivery of those gateway-owned downstream rejections now also increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_rejection_delivered` or
  `event=downstream_server_request_rejection_delivery_failed`, with structured
  warning-log route context on failure, so operators can tell whether the
  owning worker actually observed the gateway rejection
- failed delivery of those gateway-owned downstream rejections now also
  increments `gateway_v2_server_request_rejection_delivery_failures{method}`,
  so rejection handoff failures are directly measurable without filtering the
  broader lifecycle event stream
- normal downstream `serverRequest/resolved` notifications increment
  `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_resolved`, so completed prompt lifecycles
  are visible through the same lifecycle counter as duplicate replay and
  cleanup paths
- the real northbound WebSocket server-request error regression now also carries
  the rejected prompt through the downstream `serverRequest/resolved`
  notification, so error replies are covered through the same forwarded,
  answered, delivered, and resolved lifecycle telemetry as successful replies
- client replies for unknown server-request ids increment
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response` and `method=response` or
  `method=error`, matching the fail-closed protocol-violation path; the
  matching warning logs include pending gateway/downstream server-request ids
  plus affected pending and answered-but-unresolved thread ids plus pending
  worker ids
- malformed or out-of-order v2 client traffic increments
  `gateway_v2_protocol_violations` with `phase=pre_initialize` /
  `post_initialize` and reason tags such as `initialize_order`,
  `invalid_jsonrpc`, `invalid_utf8`, and `repeated_initialize`, so protocol
  misuse can be tracked without inferring it from broad connection outcomes
- downstream app-server event stream lag increments
  `gateway_v2_downstream_backpressure_events` with a `worker_id` tag before the
  gateway attempts to send the policy close frame to the northbound client; the
  matching warning log includes pending gateway/downstream server-request ids
  plus affected pending and answered-but-unresolved thread ids plus pending
  worker ids for prompt correlation
- slow-client northbound send timeouts increment
  `gateway_v2_client_send_timeouts` in addition to the terminal
  `client_send_timed_out` connection outcome; the matching warning log
  includes pending gateway/downstream server-request ids plus affected pending
  and answered-but-unresolved thread ids plus pending worker ids before cleanup
  rejection is attempted
- repeated multi-worker `thread/list` snapshots increment
  `gateway_v2_thread_list_deduplications` with the selected worker id, and
  lazy visible-thread ownership probes increment
  `gateway_v2_thread_route_recoveries` with `success` or `miss` outcomes
- dedicated northbound v2 regression coverage now also asserts those
  connection logs directly, pinning the outcome/detail mapping that operators
  rely on when diagnosing protocol-violation and other terminal session paths
- dedicated northbound notification coverage now also pins lower-frequency
  thread lifecycle notifications for `thread/archived`, `thread/unarchived`,
  and `thread/closed`, so archive and teardown state changes are covered at
  the same gateway v2 compatibility boundary as turn lifecycle streams
- the real multi-worker thread-control harness now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from both
  worker-owned threads on one shared `RemoteAppServerClient` session, so these
  lifecycle notifications are covered by the Stage B client harness as well as
  targeted northbound fixtures
- dedicated northbound notification coverage now also pins streamed item
  deltas for reasoning summaries, reasoning text, command output, and file
  changes, covering `item/reasoning/summaryTextDelta`,
  `item/reasoning/textDelta`, `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta` on the visible-thread forwarding path
- dedicated northbound notification coverage now also pins the thread-scoped
  `error` turn-failure notification and internal
  `rawResponseItem/completed` completion notification, so ordinary turn failure
  delivery and raw response item replay are covered at the same gateway v2
  compatibility boundary as turn lifecycle streams
- dedicated northbound notification coverage now also pins `fs/changed`, so
  filesystem watch change delivery is validated at the gateway v2 compatibility
  boundary alongside `fs/watch` and `fs/unwatch`
- the real single-worker remote compatibility harness now also observes
  `fs/changed` after `fs/watch`, so filesystem watch notification delivery is
  covered by the release-quality remote baseline as well as targeted
  northbound and multi-worker Stage B coverage
- the real single-worker remote reconnect harness now also replays `fs/watch`
  after worker recovery and observes the recovered worker's `fs/changed`
  notification on the same northbound client session
- that same single-worker remote reconnect harness now also replays
  `windowsSandbox/setupStart` after worker recovery and observes the recovered
  worker's `windowsSandbox/setupCompleted` notification on the same northbound
  client session
- dedicated northbound passthrough coverage now also pins
  `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop`, plus the
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  notifications used by streaming file-picker sessions
- the real single-worker remote reconnect harness now also replays
  `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop` after
  worker recovery, plus the resulting `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` notifications on the same northbound
  client session
- dedicated northbound passthrough coverage now also pins the basic filesystem
  operation family: `fs/readFile`, `fs/writeFile`, `fs/createDirectory`,
  `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and `fs/copy`
- dedicated northbound passthrough coverage now also pins the low-frequency
  Windows sandbox setup path: `windowsSandbox/setupStart`,
  `windows/worldWritableWarning`, and `windowsSandbox/setupCompleted`
- dedicated northbound passthrough coverage now also pins additional
  low-frequency v2 client requests: `marketplace/add`,
  `skills/config/write`, `experimentalFeature/enablement/set`,
  `config/mcpServer/reload`, `mcpServer/resource/read`,
  `mcpServer/tool/call`, and `account/sendAddCreditsNudgeEmail`
- the real single-worker remote compatibility harness now also exercises that
  low-frequency request family through an unmodified `RemoteAppServerClient`
  session, including a visible thread setup before thread-scoped MCP resource
  and tool calls
- the real multi-worker `RemoteAppServerClient` thread-routing harness now
  also exercises `mcpServer/resource/read` and `mcpServer/tool/call` against
  worker-owned visible threads, so thread-scoped MCP requests stay sticky to
  the owning worker in the Stage B shared-session profile
- that same multi-worker same-session recovery harness now also re-exercises
  `mcpServer/resource/read` and `mcpServer/tool/call` after the owning worker
  is lazily re-added, so recovered-worker routing covers thread-scoped MCP
  requests as well as thread, turn, review, and realtime calls
- the real multi-worker `RemoteAppServerClient` setup mutation harness now also
  exercises `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, and `config/mcpServer/reload` as
  fanout setup mutations, while the primary-worker harness covers
  `account/sendAddCreditsNudgeEmail` without duplicating the one-shot email
  side effect across workers
- that same steady-state setup mutation harness now also observes the
  `skills/changed` invalidation emitted after `skills/config/write` fanout and
  verifies duplicate worker emissions remain suppressed on the shared
  `RemoteAppServerClient` session
- that same steady-state setup mutation harness now also observes
  `account/updated` after `account/logout` fanout and verifies duplicate
  worker emissions remain suppressed on the shared `RemoteAppServerClient`
  session
- the real multi-worker same-session recovery harness now also re-exercises
  `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, and `config/mcpServer/reload` after a
  recovered worker is re-added, so these low-frequency setup mutations continue
  to fan out across the full worker set without requiring a northbound
  reconnect
- that same recovered setup-mutation harness now also observes the
  `skills/changed` invalidation emitted after `skills/config/write` fanout and
  verifies duplicate worker emissions remain suppressed on the shared
  `RemoteAppServerClient` session after worker re-add
- that same recovered setup-mutation harness now also observes
  `externalAgentConfig/import/completed` after `externalAgentConfig/import`
  fanout and verifies duplicate worker completions remain suppressed on the
  shared `RemoteAppServerClient` session after worker re-add
- that same recovered setup-mutation harness now also observes
  `account/updated` after recovered-worker `account/logout` fanout and
  verifies duplicate worker emissions remain suppressed on the shared
  `RemoteAppServerClient` session after worker re-add
- the real multi-worker same-session recovery harness now also re-exercises
  `account/sendAddCreditsNudgeEmail` after the primary worker is re-added, so
  recovered sessions keep that one-shot account side effect on the primary
  worker
- the real embedded and single-worker remote compatibility harnesses now also
  cover `windowsSandbox/setupStart` plus the follow-up
  `windowsSandbox/setupCompleted` notification through unmodified
  `RemoteAppServerClient` sessions, so this platform setup flow is part of the
  release-gate client path instead of only targeted JSON-RPC passthrough
  coverage
- multi-worker `windowsSandbox/setupStart` routing now also remains
  primary-worker affine, with reconnect-before-routing coverage for a missing
  primary worker and fail-closed coverage while that worker is still in
  reconnect backoff
- the real multi-worker `RemoteAppServerClient` primary-worker harness now
  also covers `windowsSandbox/setupStart` plus the follow-up
  `windowsSandbox/setupCompleted` notification, so this platform setup flow is
  exercised through the shared northbound session instead of only targeted
  routing tests
- the real multi-worker same-session primary-worker recovery harness now also
  re-exercises `windowsSandbox/setupStart` plus the follow-up
  `windowsSandbox/setupCompleted` notification after the primary worker is
  re-added, so platform setup remains routed through the recovered primary
  worker without requiring a northbound reconnect
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `windows/worldWritableWarning` and
  `windowsSandbox/setupCompleted`, so one shared northbound session does not
  surface duplicate platform setup notices when more than one worker emits the
  same payload
- the real multi-worker connection-state harnesses now also include
  `windows/worldWritableWarning` in both steady state and after worker
  reconnect, so Windows sandbox visibility warnings are covered by the same
  shared-session dedupe path as other connection-scoped notices
- the real embedded and single-worker remote compatibility harnesses now also
  cover fuzzy file search through unmodified `RemoteAppServerClient` sessions:
  embedded mode now exercises both one-shot search against a real temporary
  filesystem root and the streaming session start/update/stop request family
  with `fuzzyFileSearch/sessionUpdated` plus
  `fuzzyFileSearch/sessionCompleted`, while single-worker remote mode now
  exercises that same request and notification family through a remote worker
- the real embedded and single-worker remote compatibility harnesses now also
  cover the basic filesystem operation family through unmodified
  `RemoteAppServerClient` sessions, so file helper reads, writes, metadata,
  directory listing, copy, and remove paths are validated in the release-gate
  topologies
- multi-worker one-shot `fuzzyFileSearch` now fans out across all configured
  workers, deduplicates repeated root/path/type matches while keeping the
  highest-scoring duplicate, returns a score-sorted merged result set, and
  fails closed while any required worker remains in reconnect backoff; streaming
  fuzzy-file-search sessions remain primary-worker affine because their
  session ids own worker-local search state
- the real multi-worker `RemoteAppServerClient` filesystem harness now also
  verifies one-shot `fuzzyFileSearch` result fan-in from two workers on the
  shared northbound session, so the aggregation path is covered beyond raw
  JSON-RPC fixtures
- dedicated northbound request-routing coverage now also verifies that a later
  one-shot `fuzzyFileSearch` reconnects a missing worker before serving the
  aggregated result set, and dedicated northbound WebSocket coverage now pins
  that same recovered-worker fan-in path through a real shared client session,
  so degraded sessions do not silently search only the surviving worker subset
- the real multi-worker `RemoteAppServerClient` primary-worker harness now
  also validates `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop`, so one
  shared northbound session keeps streaming file-search helper state on the
  primary worker
- multi-worker basic filesystem operations now also remain primary-worker
  affine, so primary-local filesystem helper state does not silently drift to
  a secondary worker while the primary worker is unavailable
- the real multi-worker `RemoteAppServerClient` filesystem harness now also
  validates that primary-worker route for `fs/readFile`, `fs/writeFile`,
  `fs/createDirectory`, `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and
  `fs/copy`, so one shared northbound session keeps local file helper state on
  the owning primary worker
- the real multi-worker same-session primary-worker recovery harness now also
  re-exercises `fs/createDirectory`, `fs/writeFile`, `fs/readFile`,
  `fs/getMetadata`, `fs/readDirectory`, `fs/copy`, and `fs/remove` after the
  primary worker is re-added, so local filesystem helpers route through the
  recovered primary worker without requiring a northbound reconnect
- that same recovery harness now also re-exercises one-shot
  `fuzzyFileSearch` after the recovered worker is re-added, verifying the
  shared session can aggregate file-search results from both workers again
  after lazy reconnect
- that same primary-worker recovery harness now also re-exercises
  `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, and
  `fuzzyFileSearch/sessionStop` after the primary worker is re-added, so
  streaming file-search helper sessions remain primary-worker affine without
  requiring a northbound reconnect
- the real multi-worker steady-state filesystem harness now also observes
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  through the shared northbound client session, so streaming file-search
  notification fan-in is covered before recovery as well as after recovery
- that same recovery path now also observes the follow-up
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  notifications from the recovered primary worker, so streaming file-search
  notification fan-in is covered without requiring a northbound reconnect
- that filesystem change notification coverage now also pins multi-worker
  fan-in for `fs/changed`, so worker-local watch events from multiple
  downstream sessions reach one shared northbound v2 client session
- multi-worker reconnect coverage now also pins `fs/changed` delivery from a
  lazily re-added worker after the shared `fs/watch` request succeeds, so
  filesystem watch notification fan-in is covered across worker loss and
  recovery
- the real multi-worker same-session recovery harness now also validates that
  same recovered-worker `fs/changed` fan-in path through an unmodified
  `RemoteAppServerClient` session
- connection-state reconnect coverage now also pins
  `mcpServer/startupStatus/updated` delivery from a lazily re-added worker
  after `mcpServerStatus/list`, so recovered-worker MCP startup state remains
  visible on the shared northbound v2 session; the real multi-worker
  same-session bootstrap recovery harness now also observes that notification
  through an unmodified `RemoteAppServerClient` session
- that same real multi-worker bootstrap recovery harness now also observes
  recovered-worker `account/updated`, `account/rateLimits/updated`,
  `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`, and
  `windows/worldWritableWarning` delivery after the corresponding discovery
  refreshes, so the core connection-state notification and visible notice sets
  are exercised through one shared `RemoteAppServerClient` session after worker
  re-add
- that same real multi-worker bootstrap recovery harness now also re-exercises
  external-auth `account/login/start` with `chatgptAuthTokens` and `apiKey`
  after worker re-add, verifying that the fanout paths still reach the
  recovered worker and that the shared session receives deduplicated
  `account/login/completed` / `account/updated` notifications for the token
  flow plus deduplicated `account/updated` delivery and aggregated
  `account/read` state for the API-key flow
- that same bootstrap recovery harness now also verifies duplicate recovered
  connection-state notifications stay suppressed before follow-up login
  traffic, so stale duplicate account, rate-limit, app, warning, and sandbox
  setup state does not leak after a worker is lazily re-added
- the steady-state multi-worker external-auth harness now also explicitly
  verifies duplicate `account/login/completed` / `account/updated` emissions
  from token-login fanout are suppressed on the shared
  `RemoteAppServerClient` session
- the real connection-state notification harnesses now also observe
  `windowsSandbox/setupCompleted` through unmodified `RemoteAppServerClient`
  sessions in steady state, after reconnect, and in the multi-worker duplicate
  suppression path; the same-session bootstrap recovery harness also observes
  that notification after a worker is lazily re-added
- dedicated reconnect-backoff coverage now also pins
  `mcpServer/oauth/login` fail-closed behavior while a worker-discovery route
  is unavailable, so gateway routing does not continue from an incomplete MCP
  inventory during retry backoff; that coverage also asserts the fail-closed
  metric carries the active reconnect-backoff tag
- multi-worker exact-duplicate connection-notification suppression now tracks a
  bounded history of previously forwarded payloads and source workers for each
  deduplicated notification method, so a second distinct payload no longer
  immediately resets the duplicate memory for an earlier cross-worker warning
  or setup-state payload on the shared session while same-worker repeats remain
  deliverable and refresh their history position, and long-lived sessions keep
  capped dedupe memory; dedicated regressions pin that interleaved-payload
  behavior for `account/updated`, `account/rateLimits/updated`,
  `account/login/completed`, `app/list/updated`, `warning`, `configWarning`,
  `deprecationNotice`, `mcpServer/oauthLogin/completed`,
  `mcpServer/startupStatus/updated`, `windows/worldWritableWarning`, and
  `windowsSandbox/setupCompleted` notifications, same-worker repeat refreshes
  across the same broader connection-state notification set, and the
  per-method history bound; empty-payload
  `externalAgentConfig/import/completed` repeats from the same worker are also
  pinned so repeated imports do not look like cross-worker duplicates, and
  legitimate same-worker repeats are verified not to emit
  `gateway_v2_suppressed_notifications`
- multi-worker `skills/changed` invalidation suppression now only clears after
  a successful `skills/list` refresh has been returned to the northbound
  client, so failed aggregated refresh attempts do not reopen duplicate
  invalidation delivery on one shared session; dedicated websocket coverage
  pins that failed-refresh path alongside the existing successful-refresh path
- dedicated websocket coverage now also pins the failed-then-successful
  `skills/list` refresh sequence on one shared multi-worker session, verifying
  that duplicate `skills/changed` invalidations stay suppressed after the
  failed refresh and that a later successful aggregated refresh reopens fresh
  invalidation delivery
- multi-worker connection-scoped aggregated discovery requests now also fail
  closed while a required worker is unavailable during reconnect backoff,
  covering `account/read`, deprecated `getAuthStatus`,
  `account/rateLimits/read`, `model/list`, threadless `app/list`,
  `mcpServerStatus/list`,
  `externalAgentConfig/detect`, `skills/list`, `experimentalFeature/list`,
  `collaborationMode/list`, threadless `plugin/list`, and
  `thread/realtime/listVoices`, so clients cannot accidentally treat the
  surviving workers' partial inventory as a complete connection-scoped view
- multi-worker `model/list` aggregation now also supports gateway-owned
  pagination over merged worker inventories, draining worker-local model pages
  and returning stable northbound `model-offset:` cursors instead of exposing a
  primary-worker-only paginated view
- aggregated worker-local pagination now also fails closed when a downstream
  worker repeats a page cursor, preventing malformed paginated discovery
  responses from keeping a shared northbound v2 request open indefinitely
- the real multi-worker `RemoteAppServerClient` bootstrap harness now also
  validates that paginated `model/list` path through the gateway's northbound
  WebSocket surface, including follow-up `model-offset:` continuation requests
- the real multi-worker `RemoteAppServerClient` app and MCP status aggregation
  harnesses now also validate downstream worker-local page draining before
  gateway-owned `apps-offset:` and `mcp-status-offset:` cursors are exposed to
  the shared northbound session
- dedicated multi-worker aggregation coverage now also verifies
  `experimentalFeature/list` drains worker-local pages before applying
  gateway-owned `experimental-feature-offset:` pagination, keeping capability
  discovery pagination aligned with the model, app, and MCP inventory
  aggregation boundary
- the real multi-worker `RemoteAppServerClient` capability harness now also
  validates that paginated `experimentalFeature/list` path through the
  gateway's northbound WebSocket surface, including follow-up
  `experimental-feature-offset:` continuation requests after worker-local page
  draining
- `docs/gateway-v2-method-matrix.md` now explicitly records the current real
  multi-worker steady-state compatibility coverage for `thread/start`,
  aggregated `thread/list` / `thread/loaded/list`, sticky `thread/read` /
  `thread/name/set` / `thread/memoryMode/set`, detached `review/start`, and
  steady-state `turn/start` / `turn/steer` / `turn/interrupt`, so the rollout
  matrix no longer understates the existing Stage B validation surface

## Caveats

Current Stage A compatibility caveats:

- `thread/resume` is supported through the gateway scope policy with
  `threadId`, explicit `history`, or a rollout `path` that is already visible
  in the current tenant/project scope
- `thread/fork` is supported through the gateway scope policy with `threadId`
  or a rollout `path` that is already visible in the current tenant/project
  scope
- legacy `getConversationSummary` is supported through the same scope policy:
  `conversationId` follows thread ownership, while `rolloutPath` follows the
  visible rollout-path routing used by path-based `thread/resume` and
  `thread/fork`
- legacy `gitDiffToRemote` uses worker-discovery routing in multi-worker mode
  and fails closed while the configured worker set is incomplete, so local
  checkout diffs do not silently reflect only a surviving worker
- legacy `execCommandApproval` and `applyPatchApproval` prompts are outside
  the primary Stage A TUI target, but the gateway transport still treats their
  `conversationId` as thread ownership; the real multi-worker
  `RemoteAppServerClient` server-request harness now validates both legacy
  prompt methods with gateway-translated request ids across worker-owned
  visible threads
- deprecated `getAuthStatus` mirrors the `account/read` multi-worker
  compatibility behavior: primary-worker auth details stay authoritative while
  `requiresOpenaiAuth` reflects all configured workers
- full account-aware routing is still not the default release-quality
  compatibility profile: the gateway now has same-project affinity,
  cross-project account distribution, quota-aware new-thread selection, and
  bounded account handoff for explicit resumable surfaces, but arbitrary live
  active-context migration remains planned gateway work
- release-quality drop-in v2 compatibility currently means embedded runtime or
  remote runtime with exactly one downstream worker
- multi-worker remote runtime is now partially supported for v2, but it has
  broad validated steady-state and reconnect coverage without yet reaching the
  same parity and hardening level as embedded and single-worker remote
  deployments
- multi-worker notification fan-in now also has dedicated coverage for
  `rawResponseItem/completed` across worker-owned visible threads, closing one
  more turn replay path in the bounded Stage B profile
- the real multi-worker thread-control harness now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from
  worker-owned visible threads on one shared `RemoteAppServerClient` session,
  so thread lifecycle notification fan-in no longer relies only on targeted
  northbound fixtures in the bounded Stage B profile
- the real multi-worker thread-mutation harness now also observes
  `thread/name/updated` from both worker-owned visible threads after sticky
  `thread/name/set` routing on one shared `RemoteAppServerClient` session, so
  steady-state rename notification fan-in no longer relies only on targeted
  northbound fixtures or recovered-worker coverage in the bounded Stage B
  profile
- the real multi-worker same-session recovery harness now also observes those
  same thread lifecycle notifications from a lazily re-added worker on one
  shared `RemoteAppServerClient` session, so recovered-worker thread state
  changes are covered by broad Stage B client traffic as well
- that same recovery harness now also observes `thread/name/updated` after
  routing `thread/name/set` back to the lazily re-added worker, so recovered
  thread rename fan-in is covered beyond request/response routing
- the real multi-worker turn-routing harness now also observes
  `item/plan/delta`, `item/reasoning/summaryPartAdded`,
  `item/commandExecution/terminalInteraction`, `turn/diff/updated`,
  `turn/plan/updated`, `thread/tokenUsage/updated`,
  `item/mcpToolCall/progress`, `thread/compacted`, `model/rerouted`, and
  `rawResponseItem/completed` from worker-owned visible threads on one shared
  `RemoteAppServerClient` session, so lower-frequency turn notification
  forwarding no longer relies only on targeted northbound fixtures in the
  multi-worker Stage B profile
- the real multi-worker same-session recovery harness now also observes that
  same lower-frequency turn notification set after a worker is lazily
  re-added, so recovered-worker turn replay is covered beyond the basic
  lifecycle and streaming-delta notifications
- that real multi-worker turn coverage now also observes
  `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed` through unmodified
  `RemoteAppServerClient` sessions in steady state and after worker re-add, so
  guardian approval auto-review notification forwarding no longer relies only
  on targeted northbound fixtures in the bounded Stage B profile
- the real single-worker remote turn workflow harness now also observes
  `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed`, so guardian approval auto-review
  notification forwarding is covered by the release-quality remote baseline
  as well as targeted gateway fixtures and the multi-worker Stage B harness
- that real multi-worker turn coverage now also observes turn-scoped `error`
  notifications in steady state and after worker re-add, so opportunistic
  warning/error notification forwarding is covered by broad Stage B client
  traffic as well as targeted gateway regressions
- the real single-worker remote turn workflow harness now also observes
  turn-scoped `error` notifications, so opportunistic warning/error forwarding
  is covered by the release-quality remote baseline as well as targeted gateway
  regressions and the multi-worker Stage B harness
- the real multi-worker remote harness now also exercises plan-mode
  proposed-plan `item/started` and `item/completed` notifications on a worker-owned
  thread, so proposed-plan lifecycle fan-in is covered by broad Stage B client
  traffic as well as the embedded and single-worker release-quality baselines
- that same real single-worker remote turn workflow harness now also observes
  the lower-frequency turn notification set already covered by the multi-worker
  Stage B harness: `item/plan/delta`,
  `item/reasoning/summaryPartAdded`,
  `item/commandExecution/terminalInteraction`, `turn/diff/updated`,
  `turn/plan/updated`, `thread/tokenUsage/updated`,
  `item/mcpToolCall/progress`, `thread/compacted`, `model/rerouted`, and
  `rawResponseItem/completed`, so the release-quality remote baseline covers
  those forwarding paths through an unmodified `RemoteAppServerClient` session
- multi-worker remote runtime should still be treated as a bounded Stage B
  profile with explicit rollout guardrails, not as the default drop-in
  compatibility target
- backpressure on the downstream app-server event stream fails the northbound
  socket closed instead of silently degrading the session
- gateway-owned close reasons are truncated to WebSocket-safe length; operators
  should expect concise reasons rather than full downstream error payloads

## Rollout Guidance

Recommended rollout order:

1. Start with embedded mode for initial drop-in parity validation.
2. Move to single-worker remote mode if execution ownership must stay outside
   the gateway process.
3. Use multi-worker remote mode only for targeted Stage B validation until its
   broader request-routing and hardening work lands.

Topology guidance:

- embedded runtime is the simplest release-quality baseline for drop-in v2
  validation
- single-worker remote runtime is the recommended release-quality remote
  profile when the app-server must run out of process
- multi-worker remote runtime is suitable for targeted validation of the
  current Stage B transport, not for broad production rollout that assumes
  full parity with direct app-server v2 behavior

Startup examples:

```bash
# Embedded release-quality baseline.
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime embedded

# Single-worker remote release-quality baseline.
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime remote \
  --remote-websocket-url ws://127.0.0.1:9001 \
  --remote-account-id acct-primary

# Multi-worker Stage B validation profile.
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime remote \
  --remote-websocket-url ws://127.0.0.1:9001 \
  --remote-account-id acct-a \
  --remote-websocket-url ws://127.0.0.1:9002 \
  --remote-account-id acct-b \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64
```

When `--bearer-token` is configured, clients must use the same Bearer token
for the v2 WebSocket upgrade and `/v1/*` HTTP routes. When
`--remote-auth-token` is configured, the gateway uses that token for every
configured remote worker connection. If any `--remote-account-id` is provided,
provide exactly one non-blank account id for each non-blank
`--remote-websocket-url`.

Account-aware multi-worker guardrails:

- configure `--remote-account-id` for every worker before validating
  account-aware routing, otherwise project distribution and account-capacity
  diagnostics cannot identify account-backed routes; remote multi-worker
  CLI startup rejects blank account ids, and programmatic remote multi-worker
  startup emits a warning with unlabeled worker ids and WebSocket URLs when any
  worker is missing that label or has only a blank label. Blank remote worker
  WebSocket URLs are rejected before remote runtime startup for both CLI and
  programmatic gateway configuration paths. The normal remote startup log also
  includes `remote_account_labels_complete` and
  `remote_unlabeled_account_worker_count`, while `/healthz` reports
  `remoteAccountLabelsComplete`,
  `remoteUnlabeledAccountWorkerCount`,
  `remoteUnlabeledAccountWorkerIds`, and
  `remoteUnlabeledAccountWorkers` for remote runtimes so the same guardrail is
  visible in rollout health snapshots without reconstructing affected
  downstream routes from separate fields. Startup also records
  `gateway_remote_account_label_events{event="labeled",worker_id=...}` or
  `gateway_remote_account_label_events{event="unlabeled",worker_id=...}` for
  every worker in a multi-worker pool, so dashboards can alert on incomplete
  account labels and distinguish a fully labeled pool from missing metric data
  without scraping logs or polling `/healthz`.
- use same-project `thread/start` traffic to confirm cache affinity, and
  different-project `thread/start` traffic to confirm eligible accounts are
  distributed instead of concentrating unrelated projects on one account; the
  real multi-worker v2 scope harness now runs with account-labeled workers and
  verifies `/healthz.projectWorkerRoutes` reports the resulting
  tenant/project-to-worker/account bindings, while also verifying
  `/healthz.remoteAccountLabelsComplete` reports a complete labeled pool with
  no unlabeled workers and `/healthz.remoteWorkers[].accountId` carries the
  configured account labels
- treat new-thread quota failover as the only automatic account replacement
  path for active creation; verify `gateway/accountCapacityExhausted` and
  `gateway/accountFailoverSucceeded` arrive on `/v1/events` and that
  `/healthz.remoteWorkers[].accountId`, `accountCapacity`,
  `accountCapacityReason`, and `accountCapacityLastChangedAt` reflect the
  exhausted account. The remote HTTP new-thread quota failover harness now
  verifies those health fields alongside the replacement project route.
- treat explicit resumable surfaces as bounded account-handoff paths:
  `thread/read`, `thread/resume`, `thread/fork`, rollout-path
  `thread/resume` / `thread/fork`, legacy `getConversationSummary` variants,
  and resumable thread-id controls such as `thread/name/set`,
  `thread/memoryMode/set`, `thread/rollback`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`, and
  `thread/inject_items` may restore through another eligible account-backed
  worker when the requested thread id or rollout path is preserved
- expect active live-context requests without an explicit restore surface,
  such as `turn/start`, `turn/steer`, `turn/interrupt`, realtime append/stop,
  thread-scoped MCP calls, and approval / elicitation replies, to fail closed
  when the owning account is exhausted
- before widening multi-worker traffic, verify
  `/healthz.v2Connections.accountCapacityEventCounts`,
  `accountCapacityEventWorkerCounts`, and the
  `lastAccountCapacityEvent*` fields match the `/v1/events` handoff or
  fail-closed events observed during validation; the real multi-worker
  account-exhaustion harness now verifies that mirror for exhausted accounts,
  active fail-closed requests, path handoffs, and thread-id handoffs, and it
  also verifies `/healthz.remoteWorkers[].accountCapacity` plus
  `accountCapacityReason` and `accountCapacityLastChangedAt` reflect account
  exhaustion learned from v2 `account/rateLimits/read`; the same harness now
  pins `/healthz.v2Connections.failClosedRequestCounts` and the
  `lastFailClosedRequest*` fields for the active `turn/start` no-handoff
  decision
- do not document a multi-worker deployment as drop-in equivalent to embedded
  or single-worker remote until account-capacity events, fail-closed active
  requests, and bounded resumable handoffs have been exercised in that
  deployment shape

Suggested validation checklist before widening traffic:

1. Verify a client can complete `initialize`, `account/read`, and `model/list`
   over the gateway WebSocket endpoint.
2. Verify normal thread flow: `thread/start`, `thread/read`, `turn/start`, and
   turn completion notifications.
3. Verify at least one approval, user-input, elicitation, or token-refresh
   server-request round trip, because those flows exercise the bidirectional
   request/response path.
4. Verify `/healthz` reflects the expected runtime mode, worker health
   profile, remote-worker health/error timestamps, and reconnecting/backoff
   signals for the chosen deployment shape.
5. If auth is enabled, verify both the WebSocket upgrade and `/v1/*` routes
   reject missing or incorrect Bearer tokens.
6. Check rollout dashboards for the v2 transport counters that explain
   delivery health: `gateway_v2_forwarded_notifications`,
   `gateway_v2_suppressed_notifications`,
   `gateway_v2_notification_send_failures`,
   `gateway_v2_client_response_send_failures`,
   `gateway_v2_close_frame_send_failures`,
   `gateway_v2_downstream_shutdown_failures`,
   `gateway_v2_server_request_forward_send_failures`,
   `gateway_v2_server_request_answer_delivery_failures`,
   `gateway_v2_server_request_rejection_delivery_failures`, and
   `gateway_v2_server_request_lifecycle_events`.
7. For remote and multi-worker rollout, also check the counters that explain
   degraded topology behavior: `gateway_v2_worker_reconnects`,
   `gateway_v2_connections`,
   `gateway_v2_requests`,
   `gateway_v2_fail_closed_requests`,
   `gateway_v2_upstream_request_failures`,
   `gateway_v2_downstream_backpressure_events`, and
   `gateway_v2_client_send_timeouts`. Reconnect attempt/success/failure counts
   should match the worker-health changes visible in `/healthz` and the
   `v2Connections.workerReconnectEventCounts` /
   `v2Connections.workerReconnectEventWorkerCounts` health fields, fail-closed
   counts should correspond to intentional degraded-session protection and the
   matching `v2Connections.failClosedRequestCounts` /
   `lastFailClosedRequest*` health fields, and
   `gateway_v2_connections` should match
   `v2Connections.connectionOutcomeCounts`, `lastConnectionOutcome`,
   `lastConnectionDetail`, `lastConnectionDurationMs`, and
   `maxConnectionDurationMs`, so terminal connection outcomes and recent /
   peak connection durations can be reconciled from the same health snapshot
   and checked against `gateway_v2_connection_duration`,
   and the real embedded plus remote healthz regressions now pin
   `connectionOutcomeCounts` and `maxConnectionDurationMs` on actual
   client-close and slow-client-timeout teardown paths rather than only in
   registry-level tests,
   `gateway_v2_requests` should match `v2Connections.requestCounts`,
   `lastRequestMethod`, `lastRequestOutcome`, `lastRequestDurationMs`,
   `maxRequestDurationMs`, and `lastRequestAt`, so ordinary success, policy,
   quota, protocol-violation, internal-error request outcomes, and recent /
   peak gateway request latency can be reconciled from the same health
   snapshot and checked against `gateway_v2_request_duration`,
   request rejection counts should match
   `v2Connections.clientRequestRejectionCounts`,
   `lastClientRequestRejection*`,
   `serverRequestRejectionCounts`, and
   `lastServerRequestRejection*`, so pending-limit, overload, and
   scope-policy rejections can be reconciled with
   `gateway_v2_client_request_rejections` and
   `gateway_v2_server_request_rejections`.
   `gateway_v2_upstream_request_failures` should match
   `v2Connections.upstreamRequestFailureCounts` /
   `lastUpstreamRequestFailure*` health fields,
   timeout/backpressure health counters should match
   `v2Connections.clientSendTimeoutCount`,
   `lastClientSendTimeoutAt`, `downstreamBackpressureCounts`, and
   `lastDownstreamBackpressure*` health fields, and should stay at zero unless
   the validation deliberately uses a slow client or lagging downstream worker;
   the matching metrics may be absent rather than exported as explicit zero
   samples when no timeout or backpressure event occurred.
   The real single-worker and multi-worker slow-client health regressions now
   pin the client-send-timeout count and last-timeout timestamp alongside the
   terminal `client_send_timed_out` connection outcome, pin active plus
   terminal server-request backlog started-at and max/peak fields for the
   stranded ChatGPT token-refresh prompt, and pin
   `notificationSendFailureCounts` plus the newest notification-send failure
   method, outcome, and timestamp for the warning notification that triggered
   the timeout.
   Thread routing diagnostics should match
   `v2Connections.threadListDeduplicationCounts`,
   `lastThreadListDeduplication*`, `threadRouteRecoveryCounts`,
   `lastThreadRouteRecovery*`, `degradedThreadDiscoveryCounts`, and
   `lastDegradedThreadDiscovery*`, so duplicate thread-list selection, lazy
   route recovery, and intentionally degraded visible-thread discovery can be
   reconciled with `gateway_v2_thread_list_deduplications`,
   `gateway_v2_thread_route_recoveries`, and
   `gateway_v2_degraded_thread_discovery`.
   Suppressed notification counters should match
   `v2Connections.suppressedNotificationCounts` and
   `lastSuppressedNotification*`, so duplicate connection-state suppression,
   pending-refresh suppression, client opt-out drops, and hidden-thread
   notification drops can be reconciled with
   `gateway_v2_suppressed_notifications`.
   Forwarded notification and notification send-failure counters should match
   `v2Connections.forwardedNotificationCounts`,
   `lastForwardedNotification*`, `notificationSendFailureCounts`, and
   `lastNotificationSendFailure*`, so successful fan-in and slow-client
   delivery failures can be reconciled with
   `gateway_v2_forwarded_notifications` and
   `gateway_v2_notification_send_failures`.
   Transport and server-request delivery failure counters should match
   `v2Connections.clientResponseSendFailureCounts`,
   `downstreamShutdownFailureCounts`, `closeFrameSendFailureCounts`,
   `serverRequestForwardSendFailureCounts`,
   `serverRequestAnswerDeliveryFailureCounts`,
   `serverRequestRejectionDeliveryFailureCounts`, and their `last*` fields, so
   response write failures, close-frame failures, shutdown cleanup failures,
   and partially completed prompt delivery failures can be reconciled with the
   corresponding `gateway_v2_*_failure*` metrics.
   Server-request lifecycle counters should match
   `v2Connections.serverRequestLifecycleEventCounts`,
   `lastServerRequestLifecycleEvent`,
   `lastServerRequestLifecycleMethod`, and
   `lastServerRequestLifecycleAt`, so forwarded, answered, delivered,
   resolved, rejected, duplicate, cleanup, and delivery-failed prompt stages
   can be reconciled with `gateway_v2_server_request_lifecycle_events`.
   Protocol-violation counters should match
   `v2Connections.protocolViolationCounts`,
   `protocolViolationWorkerCounts`, `lastProtocolViolationPhase`,
   `lastProtocolViolationReason`, `lastProtocolViolationWorkerId`, and
   `lastProtocolViolationAt` so malformed client traffic and worker-specific
   downstream protocol regressions are visible in health snapshots as well as
   metrics.
   The gateway HTTP regression suite now also pins a representative non-empty
   `/healthz.v2Connections` JSON response for these rollout diagnostics, so
   operators can rely on the documented camelCase field names at the
   northbound API boundary and not only on registry-level tests.
8. For account-backed multi-worker validation, induce one quota-like worker
   failure and verify `gateway_v2_account_capacity_events`,
   `/v1/events`, and `/healthz.v2Connections` agree on the exhausted worker,
   tenant/project scope, reason, and either replacement route or deliberate
   no-handoff outcome.
9. During approval, user-input, elicitation, or ChatGPT token-refresh
   validation, verify
   `/healthz.v2Connections.activeConnectionServerRequestBacklogWorkerCounts`
   and `lastConnectionServerRequestBacklogWorkerCounts` identify the same
   worker-local prompt buildup that the server-request lifecycle metrics and
   logs report, with pending versus answered-but-unresolved counts split by
   worker. Also verify
   `activeConnectionServerRequestBacklogMethodCounts` and
   `lastConnectionServerRequestBacklogMethodCounts` identify the same prompt
   method family, so approval, user-input, elicitation, and token-refresh
   buildup are distinguishable in health output.
   The real multi-worker concurrent server-request regression now pins the
   active and last-completed health fields for simultaneous user-input,
   permissions approval, MCP elicitation, and ChatGPT token-refresh prompts
   across two workers, including the answered-but-unresolved counts left by
   earlier prompt classes. It also pins the active backlog start timestamp,
   current max, lifecycle peak, completed max, and completed started-at fields
   so rollout evidence can identify prompt buildup age and peak pressure from
   the same real-client multi-worker path.
   Worker-loss cleanup now also publishes
   `gateway/v2ServerRequestCleanup` on `/v1/events` when thread-scoped prompts
   are resolved or connection-scoped prompts are stranded, so validation
   evidence can tie cleanup counts, prompt method families, resolved thread
   ids, and gateway / downstream server-request ids back to the same worker
   route reported by health, metrics, and logs. Real northbound WebSocket
   regressions now pin that event payload for both a direct cleanup helper and
   the pending and answered-but-unresolved worker-loss paths that strand
   connection-scoped prompts. The cleanup log and operator event are emitted
   before synthesized `serverRequest/resolved` notifications are sent to the
   northbound client, so a later slow-client send failure does not hide the
   cleanup from `/v1/events`.

During a healthy validation pass, forwarded-notification counts should line up
with expected fan-in, suppression counts should match configured opt-outs or
duplicate connection-state notifications, and send-failure counters should stay
at zero unless the test deliberately exercises a slow or disconnected client.

Promotion evidence for multi-worker remote:

- record the exact gateway and worker configuration, including every
  `--remote-websocket-url`, `--remote-account-id`, auth mode, v2 timeout, and
  pending-request limit used during validation
- record which methods are expected to aggregate, fan out, stay primary-worker
  affine, use worker discovery, remain sticky to an owning thread route, or use
  a bounded account-handoff surface, and compare those expectations with the
  method matrix for the validated build
- capture `/healthz` before traffic, during steady-state traffic, while a
  worker is reconnecting, after recovery, and after an induced account-capacity
  event, including the v2 server-request backlog worker-count fields when a
  prompt round trip is intentionally left pending or answered-but-unresolved,
  plus the matching backlog method-count fields for the same connection window;
  when background client requests are intentionally left pending, also capture
  the pending client-request worker and method counts for that connection
  window
- capture the `/v1/events` stream for worker reconnect, account exhaustion,
  replacement-account handoff, active-thread no-handoff, and pending
  server-request cleanup cases exercised during validation; for v2 worker-loss
  cleanup, retain the `gateway/v2ServerRequestCleanup` event with the worker
  route, remaining worker count, cleanup counts, prompt method families,
  resolved thread ids, and gateway / downstream server-request ids
- export the gateway metric series named in the checklist above for the same
  window when events occurred; when send-failure, backpressure, or timeout
  failures were not intentionally induced, record the matching
  `/healthz.v2Connections` zero-valued health fields and treat absent metric
  series as equivalent to zero samples for that validation window
- when background client requests are intentionally left pending, export
  `gateway_v2_connection_pending_client_requests_by_worker` and
  `gateway_v2_connection_pending_client_requests_by_method` plus
  `gateway_v2_connection_max_pending_client_requests` for the same connection
  window so terminal `command/exec` or other long-running client request
  buildup can be correlated with the matching `/healthz.v2Connections`
  pending-client worker and method counts, while lifecycle peak pressure is
  preserved even if the queue drains before teardown
- when server requests are intentionally left pending or
  answered-but-unresolved, export
  `gateway_v2_connection_pending_server_requests_by_worker` and
  `gateway_v2_connection_answered_but_unresolved_server_requests_by_worker`
  plus the combined `gateway_v2_connection_server_request_backlog_by_worker`
  and `gateway_v2_connection_max_server_request_backlog` for the same
  connection window so terminal prompt backlog can be correlated with the
  matching `/healthz.v2Connections` server-request backlog worker counts
  without recomputing worker totals, while lifecycle peak pressure stays
  visible after short-lived prompt spikes drain
- capture `activeConnectionPeakServerRequestBacklogCount` and
  `lastConnectionMaxServerRequestBacklogCount` for the same prompt window so
  short-lived approval, user-input, elicitation, or token-refresh backlog
  spikes remain explainable even if the prompt queue drains before the active
  or terminal snapshot is collected
- capture `activeConnectionPendingClientRequestStartedAt` and
  `lastConnectionPendingClientRequestStartedAt` for the same window so the
  pending-client buildup age can be reconciled with connection start,
  completion, and terminal outcome; if the terminal pending-client count is
  first observed during teardown, `lastConnectionPendingClientRequestStartedAt`
  should use its conservative completion-time fallback rather than remaining
  absent
- capture `activeConnectionMaxPendingClientRequestCount` for the same window
  so aggregate pending-client pressure can be distinguished from one stalled
  northbound session accumulating most of the outstanding background work
- capture `activeConnectionPeakPendingClientRequestCount` and
  `lastConnectionMaxPendingClientRequestCount` for the same window so
  short-lived pending-client spikes remain explainable even if the queue
  drains before the active or terminal snapshot is collected
- for slow-client `command/exec` validation, capture an active
  `/healthz.v2Connections` snapshot while the downstream request is still
  pending and verify that active pending-client count, max count, buildup
  timestamp, worker counts, and method counts already match the request route
  before the connection reaches its terminal timeout outcome
- retain the corresponding v2 connection completion logs or audit logs for
  that window, including pending-client worker and method count summaries, so
  operators can reconcile terminal logs, health, and exported metrics without
  inspecting individual request payloads
- retain client-side transcripts for normal bootstrap, thread/turn, approval,
  elicitation, or token-refresh round trip, reconnect recovery,
  account-capacity failover, and explicit resumable-thread restoration flows
- document every intentional fail-closed result with the method, worker route,
  tenant/project scope, client-visible JSON-RPC error, `/v1/events` record,
  and matching metric labels
- document every intentionally pending prompt lifecycle with the worker route,
  server-request kind, health backlog worker and method counts, lifecycle
  metrics, and terminal cleanup or resolution outcome
- do not promote the deployment shape if any required evidence is missing, if
  health/events/metrics disagree about the affected worker or account, or if a
  live active-context request silently succeeds on a replacement account
  without an explicit restore surface
- keep the promotion scoped to the exact topology and method families that
  were validated; adding workers, changing account labels, changing timeout or
  pending-request limits, or enabling new v2 methods should trigger another
  evidence pass before the deployment is described as release-quality

## Testing Plan

Tests should be layered.

### Unit tests

- JSON-RPC proxying and request ID correlation
- scope filtering for thread-bound traffic
- server-request response routing
- error mapping and disconnect handling

### Integration tests

- embedded gateway with a northbound v2 WebSocket client
- remote gateway with one worker
- remote gateway with multiple workers
- initialize/initialized handshake ordering
- normal thread and turn flows
- approval, user-input, elicitation, and token-refresh round trips

### Compatibility tests

- run an existing Codex client or a close protocol harness against the gateway
- verify that no client-side transport changes are required in the Stage A
  profile

## Documentation Deliverables

When implementation starts, update:

- `docs/gateway.md` with milestone status
- gateway startup docs and examples
- a method-coverage matrix for v2 compatibility mode
- operator guidance for embedded, single-worker remote, and multi-worker remote
  profiles

## Recommended Execution Order

Build in this order:

1. northbound v2 WebSocket skeleton
2. embedded-mode drop-in parity
3. single remote-worker parity
4. coverage matrix and remaining gap analysis
5. multi-worker coordination
6. hardening and rollout docs

This keeps the critical path short and gets to a useful compatibility target
before taking on the hardest part, which is multi-worker multiplexing.
