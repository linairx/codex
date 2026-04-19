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
- Milestone 2 is in progress for the embedded topology
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

Status: in progress

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
- the embedded harness now also verifies app-server's current unmaterialized
  thread semantics, preserving the expected `no rollout found for thread id ...`
  errors for `thread/resume` and `thread/fork` over the gateway's northbound v2
  transport

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
  `thread/status/changed`, `turn/started`, `item/agentMessage/delta`, and
  `turn/completed`

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
  `externalAgentConfig/import`, `skills/list`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
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
- the real embedded `RemoteAppServerClient` harness now also covers the same
  `item/tool/requestUserInput` server-request round trip, validating that the
  gateway proxies the typed response back into the in-process app-server
  session and forwards `serverRequest/resolved` before turn completion
- the real embedded `RemoteAppServerClient` harness now also covers
  `item/permissions/requestApproval`, validating that the gateway preserves the
  in-process approval round trip and `serverRequest/resolved` ordering for the
  request-permissions flow too
- the real embedded `RemoteAppServerClient` harness now also covers
  `mcpServer/elicitation/request`, validating that the gateway preserves the
  in-process MCP elicitation round trip and `serverRequest/resolved` ordering
  for connector-driven tool flows too
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
- the gateway now has real northbound JSON-RPC regression tests for the current
  scope-policy hardening on `thread/resume` and `thread/fork`, verifying that
  history/path-based resume and path-based fork are rejected before they can
  bypass thread ownership checks
- the gateway now also has real northbound JSON-RPC regression tests for
  `thread/list` and `thread/loaded/list` scope filtering, verifying that hidden
  threads are stripped from v2 responses at the transport boundary
- the gateway now also has dedicated northbound passthrough coverage for the
  remaining Stage A thread/turn control methods that are less common in normal
  sessions, including `thread/unsubscribe`, `thread/compact/start`,
  `thread/shellCommand`, `thread/backgroundTerminals/clean`,
  `thread/rollback`, `review/start`, `turn/interrupt`, and `turn/steer`
- the embedded compatibility harness now also covers additional bootstrap/setup
  requests that current clients use during startup and settings flows:
  `externalAgentConfig/detect`, `externalAgentConfig/import`, `skills/list`,
  `config/batchWrite`, `memory/reset`, and `account/logout`
- northbound v2 connections now fail closed when the downstream app-server
  event stream reports backpressure via `Lagged`, closing the WebSocket with an
  explicit gateway-owned reason instead of silently continuing after best-effort
  event loss
- gateway-owned WebSocket close reasons are now clamped to protocol-safe
  length, so downstream disconnect errors cannot turn into invalid oversized
  control frames at the northbound client
- multi-worker remote runtime now has explicit integration coverage for the
  current unsupported Stage A profile, verifying both the `/healthz`
  compatibility signal and the `501 Not Implemented` response on northbound v2
  WebSocket upgrade attempts
- single-worker remote runtime now also has a northbound reconnect regression
  test, verifying that after the gateway reconnects its downstream worker, a
  later v2 client session can still bootstrap and complete `thread/start`
- that reconnect regression now waits for `/healthz` to report a recovered
  single-worker compatibility profile, so it covers both worker recovery and
  subsequent northbound v2 admission

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
- northbound v2 compatibility is not supported yet
- the gateway returns `501 Not Implemented` for northbound v2 WebSocket
  connections in this profile

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
- do not point unmodified Codex v2 clients at this profile yet
- the missing piece is Milestone 5 connection coordination: sticky per-thread
  routing, request/server-request correlation per worker, and safe notification
  fan-in on one northbound socket

## Caveats

Current Stage A compatibility caveats:

- `thread/resume` is only supported through the gateway scope policy when it
  uses `threadId`; history-based and path-based resume are rejected
- `thread/fork` is only supported through the gateway scope policy when it uses
  `threadId`; path-based fork is rejected
- multi-worker remote runtime is intentionally outside the current v2 support
  envelope
- backpressure on the downstream app-server event stream fails the northbound
  socket closed instead of silently degrading the session
- gateway-owned close reasons are truncated to WebSocket-safe length; operators
  should expect concise reasons rather than full downstream error payloads

## Rollout Guidance

Recommended rollout order:

1. Start with embedded mode for initial drop-in parity validation.
2. Move to single-worker remote mode if execution ownership must stay outside
   the gateway process.
3. Keep multi-worker remote deployments on HTTP/SSE clients until Milestone 5
   connection coordination lands.

Suggested validation checklist before widening traffic:

1. Verify a client can complete `initialize`, `account/read`, and `model/list`
   over the gateway WebSocket endpoint.
2. Verify normal thread flow: `thread/start`, `thread/read`, `turn/start`, and
   turn completion notifications.
3. Verify one approval or user-input server-request round trip, because that
   exercises the bidirectional request/response path.
4. Verify `/healthz` reflects the expected runtime mode and worker health
   profile for the chosen deployment shape.
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
