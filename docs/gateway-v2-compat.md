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
  including `item/started`, `item/completed`, `hookStarted`, and
  `hookCompleted`
- dedicated northbound gateway coverage now also exercises item lifecycle
  notifications the TUI uses for longer-running turn state, covering
  `item/started` and `item/completed`
- dedicated northbound gateway coverage now also exercises hook lifecycle
  notifications the TUI renders for hook progress and completion, covering
  `hookStarted` and `hookCompleted`
- dedicated northbound gateway coverage now also exercises guardian review
  lifecycle notifications for approval auto-review UX, covering
  `item/guardianApprovalReviewStarted` and
  `item/guardianApprovalReviewCompleted`
- dedicated northbound gateway coverage now also exercises richer turn
  notifications that the TUI consumes opportunistically, including
  `plan/delta`, `reasoning/summaryPartAdded`, `terminalInteraction`,
  `turn/diffUpdated`, `turn/planUpdated`, `thread/tokenUsage/updated`,
  `mcpToolCall/progress`, `contextCompacted`, and `model/rerouted`
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
  failures or connection outcomes
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
- suppressed multi-worker connection notifications now also emit
  `gateway_v2_suppressed_notifications` with notification method and reason
  tags; current reasons are `duplicate` for exact-duplicate connection-state
  notifications and `pending_refresh` for repeated `skills/changed`
  invalidations before the client refreshes `skills/list`, plus
  `hidden_thread` when a thread-scoped downstream notification is outside the
  current gateway request scope
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  gateway request-id translation now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_resolved_replay` and `method=serverRequest/resolved`, so
  server-request replay anomalies are visible in metrics alongside the
  structured warning logs
- downstream server requests that reuse a still-pending gateway request id now
  also emit `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` and the colliding server-request method
  before the gateway fails the session closed, so request-id collision
  anomalies are visible in metrics alongside the close outcome and structured
  warning log
- worker-loss cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` for synthetic thread-scoped
  `serverRequest/resolved` notifications and stranded connection-scoped server
  requests, so disconnect-driven prompt cleanup is visible in metrics alongside
  the structured cleanup logs
- those worker-loss cleanup lifecycle counters are recorded before any
  synthetic `serverRequest/resolved` notification is sent to the northbound
  client, so slow-client send timeouts do not hide the route cleanup event from
  metrics
- client-side connection cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` for rejected thread-scoped
  pending prompts, rejected connection-scoped pending prompts, and
  answered-but-unresolved prompts left behind by client disconnect,
  protocol-violation, or slow-send teardown paths
- client replies for unknown server-request ids now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response`, so protocol-violation
  prompt replies are visible in metrics before the gateway fails the v2
  session closed
- malformed or out-of-order v2 client traffic now also emits
  `gateway_v2_protocol_violations` with `phase` and `reason` tags, covering
  pre-initialize ordering errors, invalid JSON-RPC payloads, invalid UTF-8
  binary frames, and repeated `initialize` requests after the handshake; direct
  northbound websocket regressions pin those metric tags at the transport
  boundary
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
  structured routing logs
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
  `plugin/read`, `config/batchWrite`, `memory/reset`, and `account/logout`
- that same real single-worker remote harness now also covers the
  supporting/configuration methods that some clients and test tools use
  outside the main TUI flow: `config/read`, `configRequirements/read`,
  `experimentalFeature/list`, and `collaborationMode/list`
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
- dedicated northbound multi-worker regression coverage now also verifies that
  `account/login/start` with external-auth `apiKey` and
  `chatgptAuthTokens` is fanned out to every downstream worker session
  instead of updating only the primary worker
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
- the gateway now also has real northbound JSON-RPC regression tests for
  `thread/list` and `thread/loaded/list` scope filtering, verifying that hidden
  threads are stripped from v2 responses at the transport boundary
- hidden-thread downstream server requests that are rejected by gateway scope
  policy now also emit structured warning logs with scope, worker id, request
  id, method, and hidden thread id, with direct northbound regression coverage
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
  `item/completed`, `hookStarted`, and `hookCompleted`, so one shared client
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
  `account/login/completed`, and `feedback/upload`
- that same real multi-worker primary-worker harness now also covers the
  standalone `command/exec` control plane
  (`command/exec`, `command/exec/outputDelta`, `command/exec/write`,
  `command/exec/resize`, and `command/exec/terminate`), so primary-worker
  command execution has baseline drop-in coverage in addition to reconnect
  recovery coverage
- multi-worker steady-state Stage B coverage now spans the main shared-session
  surface: connection-scoped setup mutations such as
  `externalAgentConfig/import`, `config/value/write`, `config/batchWrite`,
  `memory/reset`, and `account/logout` fan out across workers; bootstrap and
  setup discovery such as `account/read`, `model/list`,
  `externalAgentConfig/detect`, and `skills/list` aggregate instead of
  exposing only the primary worker view; and lower-frequency thread-control,
  review, and thread-mutation paths remain sticky to the owning worker,
  including `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, `review/start`, and
  `thread/name/set`
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
  `account/read` and unpaginated `model/list`, verifying that later bootstrap
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
  `fs/watch` / `fs/unwatch`, `account/read`, `account/rateLimits/read`,
  `model/list`, threadless `app/list`, threadless `plugin/list`,
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
  (`thread/status/changed`, `turn/started`, `hookStarted`, `item/started`,
  `item/agentMessage/delta`, reasoning / command / file-change deltas,
  `hookCompleted`, `item/completed`, and `turn/completed`) plus the current
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
  `account/rateLimits/read`, unpaginated `model/list`,
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
  translation; fail-closed handling when a client replies to a server request
  that is no longer pending, including rejection of any still-pending
  downstream prompts; and structured warning logs for both
  protocol-violation cleanup and unresolved server-request saturation
- that same protocol-violation logging now also includes any answered-but-
  unresolved server-request routes when the client replies to an unknown
  request id, and a real northbound regression now pins that ordering-sensitive
  path after one prior server request has already been answered
- downstream app-server event-stream backpressure now also emits a structured
  warning log with scope, worker id, skipped-event count, and any still-pending
  gateway server-request ids before the gateway closes the northbound session,
  so lag-driven teardown is visible without reconstructing the session from
  connection outcomes alone
- that same backpressure path now also includes any answered-but-unresolved
  server-request routes in those warning logs, and a northbound regression now
  pins the fail-closed close frame plus translated gateway/downstream request
  ids after the client has already answered but downstream
  `serverRequest/resolved` never arrives before lag closes the session
- slow-client send timeouts now also emit a structured warning log with scope,
  terminal timeout detail, and any still-pending gateway server-request ids
  before pending downstream prompts are rejected, so `client_send_timed_out`
  outcomes can be tied back to the stranded session state directly from logs
- v2 connection accounting now also records terminal outcome and decrements the
  active connection count even if an early handshake-time close frame or
  JSON-RPC error response cannot be delivered, so `/healthz` does not retain
  stale active-session state after setup-time send failures
- `/healthz` now also captures the last completed connection's pending and
  answered-but-unresolved server-request counts, so partially completed prompt
  lifecycles remain visible even after the gateway has already rejected or
  cleaned up the stranded downstream work
- that slow-client path now also has a real northbound regression where one
  pending downstream server request is left unresolved while large follow-up
  notifications wedge the client send path, verifying the timeout warning,
  `client_send_timed_out` outcome, and downstream prompt rejection together
- those slow-client and connection-end teardown logs now also include any
  answered-but-unresolved server-request routes, with translated gateway ids,
  downstream ids, and worker ids, and that same slow-client path now also has
  a real northbound regression where the client has already replied but
  downstream `serverRequest/resolved` never arrives before timeout
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
- suppressed multi-worker notification dedupe paths now also emit structured
  warning logs: duplicate `skills/changed` invalidations include scope,
  worker id, and params until the client refreshes `skills/list`, and exact-
  duplicate connection-state notifications include the same context when they
  are dropped
- that same exact-duplicate connection-state suppression now also covers
  `mcpServer/oauthLogin/completed`, so one shared northbound session does not
  surface duplicate MCP OAuth completion notifications when more than one
  worker emits the same payload
- the real multi-worker connection-state harnesses now also verify that
  `mcpServer/oauthLogin/completed` is emitted exactly once both in steady
  state and after worker reconnect, so that dedupe path is exercised through
  an unmodified `RemoteAppServerClient` session in addition to dedicated
  northbound regressions
- hidden-thread downstream server requests that are rejected by gateway scope
  policy now also emit structured warning logs with scope, worker id, the
  translated gateway request id, method, and hidden thread id, so approval
  prompts dropped at the transport boundary are visible without reproducing the
  client session
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
- fail-closed handling for a downstream session that reuses a still-pending
  server-request id now also emits a structured warning log with scope, worker
  id, the colliding request id/method, and the gateway request ids already
  pending on the connection
- gateway-owned teardown of still-pending downstream server requests now also
  emits structured warning logs with connection outcome, terminal detail,
  pending gateway request ids, per-scope counts, and worker ids, so
  disconnect-driven cleanup is visible without reconstructing the session from
  downstream errors alone
- multi-worker `thread/list` dedupe now also emits a structured log with scope,
  thread id, selected worker, discarded worker, and snapshot timestamps when
  the gateway chooses one visible thread copy over another, so cross-worker
  snapshot selection is observable without reproducing the merged response path
- missing multi-worker thread routes now also emit structured success/failure
  logs when the gateway probes downstream `thread/read` to recover ownership,
  including scope, thread id, and recovered or attempted worker ids, so lazy
  route recovery no longer depends on inferring behavior from later request
  routing alone
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
  `v2Connections.peakActiveConnectionCount`,
  `v2Connections.totalConnectionCount`,
  `v2Connections.lastConnectionStartedAt`,
  `v2Connections.lastConnectionDurationMs`,
  `v2Connections.lastConnectionPendingServerRequestCount`, and
  `v2Connections.lastConnectionAnsweredButUnresolvedServerRequestCount`, plus
  the latest completed v2 connection outcome/detail/timestamp for quick
  northbound compatibility-session diagnostics
- initialize timeout and downstream disconnects surface as explicit WebSocket
  close frames, not silent socket drops
- tune `--v2-initialize-timeout-seconds` and
  `--v2-client-send-timeout-seconds` when you need stricter or looser
  northbound timeout behavior during rollout
- tune `--v2-max-pending-server-requests` when you need a tighter or looser
  cap on unresolved server-request state for each northbound compatibility
  session

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
- aggregated `thread/list`, `thread/loaded/list`, and threadless `app/list`
  responses plus translated server-request IDs are supported, but broader
  parity and hardening work are still in progress

Example:

```bash
cargo run -p codex-gateway -- \
  --runtime remote \
  --listen 127.0.0.1:8080 \
  --remote-websocket-url ws://127.0.0.1:8081 \
  --remote-websocket-url ws://127.0.0.1:8082
```

Operational notes:

- this profile is valid for the existing HTTP/SSE gateway API
- this profile now admits northbound v2 WebSocket sessions instead of failing
  closed at upgrade time
- current multi-worker v2 routing is still an incremental Stage B transport:
  use it for development and targeted validation, not broad drop-in rollout
- remaining work is broader request/notification parity plus more operational
  hardening for reconnect and load behavior across multiple workers
- `/healthz` exposes the effective v2 transport hardening config
  (`initializeTimeoutSeconds`, `clientSendTimeoutSeconds`,
  `reconnectRetryBackoffSeconds`, and `maxPendingServerRequests`) so operators
  can confirm rollout settings without relying only on startup logs
- `/healthz` also exposes the same `v2Connections` snapshot in this profile,
  and that connection-health view now has real multi-worker regression
  coverage for live, concurrent, and settled northbound client sessions
- `/healthz` remote-worker entries now also expose `lastStateChangeAt` and
  `lastErrorAt`, so operators can distinguish a worker that is actively
  flapping from one that is healthy again but still carries a historical
  `lastError`
- `/healthz` remote-worker entries also expose `reconnecting`,
  `reconnectAttemptCount`, and `nextReconnectAt`, so operators can tell
  whether a remote worker is still in background reconnect backoff, how many
  reconnect attempts have already failed in the current loop, and whether the
  worker is only carrying a stale unhealthy flag
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
  plus the pending and answered-but-unresolved server-request counts, matching
  the `/healthz` v2 connection snapshot and connection metrics used to
  diagnose stranded prompt lifecycles
- saturated and hidden-thread downstream server-request rejections also
  increment the `gateway_v2_server_request_rejections` counter with `method`
  and `reason` tags; current reasons are `pending_limit` and `hidden_thread`
- multi-worker remote reconnect loops increment
  `gateway_v2_worker_reconnects` with `worker_id` and `outcome` tags; current
  outcomes are `attempt`, `success`, `connect_failure`, `replay_failure`, and
  `backoff_suppressed`
- multi-worker remote requests that fail closed because required worker routes
  are unavailable increment `gateway_v2_fail_closed_requests` with `method`
  and `reconnect_backoff_active` tags, matching the structured warning log
  emitted for the same event
- primary-worker-only multi-worker requests now also have method-family
  reconnect-backoff coverage for config requirements, managed login, login
  cancellation, feedback upload, standalone command control, basic filesystem
  operations, fuzzy file search, and Windows sandbox setup, so the fail-closed
  metric and no-fallback behavior are pinned across the full primary route set
- suppressed multi-worker notification dedupe increments
  `gateway_v2_suppressed_notifications` with `method` and `reason` tags,
  matching the structured warning logs emitted when duplicate connection-state
  notifications, repeated `skills/changed` invalidations, or hidden-thread
  notifications are dropped
- duplicate downstream `serverRequest/resolved` replays increment
  `gateway_v2_server_request_lifecycle_events` with `event` and `method`
  tags, matching the structured warning log emitted when the translated route
  has already been drained
- downstream server requests that reuse a still-pending gateway request id also
  increment `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` and the colliding server-request method,
  matching the structured warning log emitted before the gateway closes the
  session
- worker-loss cleanup increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=worker_cleanup_resolved_thread_scoped` /
  `method=serverRequest/resolved` for each synthesized resolution, and
  `event=worker_cleanup_stranded_connection_scoped` /
  `method=connectionScopedServerRequest` for stranded connection-scoped
  prompts; the counters are emitted immediately after route cleanup, before
  any synthetic resolution notification is sent to the client
- client-side connection cleanup increments
  `gateway_v2_server_request_lifecycle_events` with
  `event=client_cleanup_rejected_thread_scoped` /
  `method=serverRequest/pending`,
  `event=client_cleanup_rejected_connection_scoped` /
  `method=serverRequest/pending`, and
  `event=client_cleanup_answered_but_unresolved` /
  `method=serverRequest/resolved`
- client replies for unknown server-request ids increment
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response` and `method=response` or
  `method=error`, matching the fail-closed protocol-violation path
- malformed or out-of-order v2 client traffic increments
  `gateway_v2_protocol_violations` with `phase=pre_initialize` /
  `post_initialize` and reason tags such as `initialize_order`,
  `invalid_jsonrpc`, `invalid_utf8`, and `repeated_initialize`, so protocol
  misuse can be tracked without inferring it from broad connection outcomes
- downstream app-server event stream lag increments
  `gateway_v2_downstream_backpressure_events` with a `worker_id` tag before the
  gateway attempts to send the policy close frame to the northbound client
- slow-client northbound send timeouts increment
  `gateway_v2_client_send_timeouts` in addition to the terminal
  `client_send_timed_out` connection outcome
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
- dedicated northbound notification coverage now also pins `fs/changed`, so
  filesystem watch change delivery is validated at the gateway v2 compatibility
  boundary alongside `fs/watch` and `fs/unwatch`
- dedicated northbound passthrough coverage now also pins
  `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop`, plus the
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  notifications used by streaming file-picker sessions
- dedicated northbound passthrough coverage now also pins the basic filesystem
  operation family: `fs/readFile`, `fs/writeFile`, `fs/createDirectory`,
  `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and `fs/copy`
- dedicated northbound passthrough coverage now also pins the low-frequency
  Windows sandbox setup path: `windowsSandbox/setupStart`,
  `windows/worldWritableWarning`, and `windowsSandbox/setupCompleted`
- multi-worker `windowsSandbox/setupStart` routing now also remains
  primary-worker affine, with reconnect-before-routing coverage for a missing
  primary worker and fail-closed coverage while that worker is still in
  reconnect backoff
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `windows/worldWritableWarning` and
  `windowsSandbox/setupCompleted`, so one shared northbound session does not
  surface duplicate platform setup notices when more than one worker emits the
  same payload
- the real embedded and single-worker remote compatibility harnesses now also
  cover fuzzy file search through unmodified `RemoteAppServerClient` sessions:
  embedded mode exercises one-shot search against a real temporary filesystem
  root, while single-worker remote mode exercises the one-shot request plus the
  streaming session start/update/stop request family
- the real embedded and single-worker remote compatibility harnesses now also
  cover the basic filesystem operation family through unmodified
  `RemoteAppServerClient` sessions, so file helper reads, writes, metadata,
  directory listing, copy, and remove paths are validated in the release-gate
  topologies
- multi-worker `fuzzyFileSearch` routing now also remains primary-worker
  affine, with real northbound WebSocket reconnect coverage for a missing
  primary worker and fail-closed coverage while that worker is still in
  reconnect backoff
- multi-worker basic filesystem operations now also remain primary-worker
  affine, so primary-local filesystem helper state does not silently drift to
  a secondary worker while the primary worker is unavailable
- that filesystem change notification coverage now also pins multi-worker
  fan-in for `fs/changed`, so worker-local watch events from multiple
  downstream sessions reach one shared northbound v2 client session
- multi-worker reconnect coverage now also pins `fs/changed` delivery from a
  lazily re-added worker after the shared `fs/watch` request succeeds, so
  filesystem watch notification fan-in is covered across worker loss and
  recovery
- connection-state reconnect coverage now also pins
  `mcpServer/startupStatus/updated` delivery from a lazily re-added worker
  after `mcpServerStatus/list`, so recovered-worker MCP startup state remains
  visible on the shared northbound v2 session
- dedicated reconnect-backoff coverage now also pins
  `mcpServer/oauth/login` fail-closed behavior while a worker-discovery route
  is unavailable, so gateway routing does not continue from an incomplete MCP
  inventory during retry backoff; that coverage also asserts the fail-closed
  metric carries the active reconnect-backoff tag
- `docs/gateway-v2-method-matrix.md` now explicitly records the current real
  multi-worker steady-state compatibility coverage for `thread/start`,
  aggregated `thread/list` / `thread/loaded/list`, sticky `thread/read` /
  `thread/name/set` / `thread/memoryMode/set`, detached `review/start`, and steady-state
  `turn/start` / `turn/steer` / `turn/interrupt`, so the rollout matrix no
  longer understates the existing Stage B validation surface

## Caveats

Current Stage A compatibility caveats:

- `thread/resume` is supported through the gateway scope policy with
  `threadId`, explicit `history`, or a rollout `path` that is already visible
  in the current tenant/project scope
- `thread/fork` is supported through the gateway scope policy with `threadId`
  or a rollout `path` that is already visible in the current tenant/project
  scope
- release-quality drop-in v2 compatibility currently means embedded runtime or
  remote runtime with exactly one downstream worker
- multi-worker remote runtime is now partially supported for v2, but it has
  broad validated steady-state and reconnect coverage without yet reaching the
  same parity and hardening level as embedded and single-worker remote
  deployments
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

Suggested validation checklist before widening traffic:

1. Verify a client can complete `initialize`, `account/read`, and `model/list`
   over the gateway WebSocket endpoint.
2. Verify normal thread flow: `thread/start`, `thread/read`, `turn/start`, and
   turn completion notifications.
3. Verify one approval or user-input server-request round trip, because that
   exercises the bidirectional request/response path.
4. Verify `/healthz` reflects the expected runtime mode, worker health
   profile, remote-worker health/error timestamps, and reconnecting/backoff
   signals for the chosen deployment shape.
5. If auth is enabled, verify both the WebSocket upgrade and `/v1/*` routes
   reject missing or incorrect Bearer tokens.

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
- approval / user-input round trips

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
