# Gateway Plan

## Conclusion

This repository already contains most of the agent runtime that a gateway needs.
The right boundary is:

`gateway -> app-server protocol -> MessageProcessor -> ThreadManager/CodexThread -> core`

The gateway should wrap `codex-app-server`; it should not expose `codex-core`
directly as the northbound API surface.

## Existing Modules To Reuse

- `codex-rs/app-server-protocol/src/protocol/v2.rs`
  This is the current external contract for threads, turns, filesystem RPC,
  command execution, MCP operations, approvals, and notifications.
- `codex-rs/app-server/src/message_processor.rs`
  This is the existing application-service layer that maps protocol requests to
  runtime behavior.
- `codex-rs/app-server/src/in_process.rs`
  This is the fastest path for a first gateway implementation because it keeps
  app-server semantics without adding a process boundary.
- `codex-rs/core/src/thread_manager.rs`
  This owns thread lifecycle and should remain behind app-server.
- `codex-rs/core/src/codex_thread.rs`
  This exposes the runtime thread abstraction used by app-server.
- `codex-rs/app-server/src/command_exec.rs`
  This already provides the command execution control plane that the gateway can
  surface later.
- `codex-rs/exec-server/README.md`
  This points toward a later execution-plane split if command execution needs to
  run out of process.

## Target Architecture

Add a new crate, `codex-gateway`, with these responsibilities:

- HTTP/WebSocket/SSE northbound API
- authentication and authorization
- tenant and project isolation
- request admission, quotas, and rate limits
- audit and observability
- routing to a local embedded app-server or a remote worker

Keep business execution in the existing stack:

`codex-gateway -> codex-app-server -> codex-core`

## Initial Crate Shape

```text
codex-rs/gateway/
  src/lib.rs
  src/api.rs
  src/error.rs
  src/adapter.rs
  src/runtime.rs
  src/northbound/
    mod.rs
    http.rs
```

Phase 1 should keep the runtime local and embedded through
`codex_app_server_client`.

## Northbound API Shape

Phase 1 started with a deliberately small HTTP surface:

- `GET /healthz`
- `POST /v1/threads`
- `GET /v1/threads/:thread_id`
- `POST /v1/threads/:thread_id/turns`

These endpoints are intentionally thin adapters over existing app-server
requests:

- `POST /v1/threads` -> `thread/start`
- `GET /v1/threads/:thread_id` -> `thread/read`
- `POST /v1/threads/:thread_id/turns` -> `turn/start`

Phase 3 extends that northbound surface with:

- `GET /v1/events`
- `GET /v1/threads`
- `POST /v1/threads/:thread_id/turns/:turn_id/interrupt`
- `POST /v1/server-requests/respond`

The gateway should define its own northbound DTOs even when they are initially
mapped almost one-to-one from app-server responses. That keeps the gateway
boundary stable when app-server evolves internally.

## Roadmap

### Phase 1: Foundation

Status: complete

- add `codex-gateway` crate to the workspace
- define gateway DTOs for thread and turn operations
- define a `GatewayRuntime` trait
- add an `AppServerGatewayRuntime` implementation backed by
  `codex_app_server_client::AppServerRequestHandle`
- add an HTTP router with the first thread and turn endpoints
- keep bootstrap and transport policy minimal

Exit criteria:

- the new crate compiles
- request mapping from gateway DTOs to app-server protocol is covered by tests
- the HTTP router can serve the initial endpoints against a supplied runtime

### Phase 2: Embedded Runtime Bootstrap

Status: complete

- add local gateway startup that initializes an in-process app-server client
- define gateway configuration for bind address, client identity, and runtime
  mode
- add a small binary entrypoint for local development

Exit criteria:

- `codex-gateway` can run as a local HTTP service backed by embedded app-server

### Phase 3: Streaming and Control Plane

Status: complete

- add SSE streaming for server notifications
- add turn interruption
- add thread listing and pagination
- reject unsupported app-server server requests for HTTP clients
- add approval request handling strategy for HTTP clients

Exit criteria:

- northbound clients can create a thread, start a turn, and observe streamed
  progress and completion

### Phase 4: Platform Capabilities

Status: complete

- add authentication middleware
- add tenant and project scoping
- add audit logging and metrics
- add rate limiting and quotas

Exit criteria:

- the gateway can safely serve multiple external clients with isolation and
  observability

### Phase 5: Remote Workers

Status: complete

- add runtime routing to remote app-server workers
- separate execution-plane concerns from the gateway process
- evaluate moving command execution toward `exec-server`

Exit criteria:

- the gateway can route requests to remote workers without changing the
  northbound API contract

### Phase 6: App-Server v2 Compatibility

Status: in progress

Goal:

- make `codex-gateway` speak the app-server v2 northbound protocol so existing
  Codex clients can target the gateway directly

Principles:

- preserve the existing HTTP/SSE gateway surface; v2 compatibility is additive
- reuse `codex-app-server-protocol` types and wire semantics instead of
  inventing gateway-specific v2 DTOs
- prefer a transport/proxy architecture over re-implementing app-server
  business logic in the gateway
- ship in stages: embedded and single-worker parity first, multi-worker fanout
  second

Planned milestones:

- add a northbound WebSocket JSON-RPC transport that terminates v2 client
  connections at the gateway
- proxy `initialize`, requests, notifications, responses, and server requests to
  downstream app-server sessions while enforcing auth and scope policy at the
  gateway boundary
- reach "drop-in Codex client" parity for embedded mode and single remote worker
  mode first
- add connection coordination and routing for multi-worker remote deployments
  without breaking the v2 contract
- keep HTTP/SSE as the operator- and platform-oriented API while v2 WebSocket
  remains the compatibility API

Exit criteria:

- an unmodified Codex client can connect to `codex-gateway` over the app-server
  v2 WebSocket protocol and complete a normal thread/turn workflow
- gateway auth, scoping, audit, and rate-limit policies are still enforced for
  v2 clients
- remote-worker mode supports either a documented single-worker compatibility
  profile or coordinated multi-worker routing with equivalent behavior

Detailed plan:

- see [docs/gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md)

Recent progress:

- the Stage A required-method matrix is now documented in
  [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)
- dedicated gateway passthrough tests now cover the bootstrap-critical
  `account/read` and `model/list` requests in addition to the previously added
  thread, turn, server-request, and realtime compatibility coverage
- embedded runtime now has a real drop-in v2 client harness using
  `RemoteAppServerClient`, covering bootstrap plus thread read/list/update
  workflow through the gateway's northbound WebSocket surface
- single-worker remote runtime now also has a real northbound v2 client harness
  using `RemoteAppServerClient`, covering bootstrap plus thread
  read/list/update workflow through a gateway-backed remote worker session
- northbound v2 connections now fail closed if the downstream app-server event
  stream reports backpressure, instead of silently continuing after best-effort
  event loss
- gateway-owned v2 WebSocket close reasons are now clamped to protocol-safe
  length, so downstream transport errors cannot produce oversized close frames
- `docs/gateway-v2-compat.md` now includes operator guidance and rollout
  profiles for embedded, single-worker remote, and multi-worker remote
  deployments
- multi-worker remote runtime now has explicit integration coverage for the
  current Stage A boundary: `/healthz` reports
  `RemoteMultiWorkerUnsupported` for v2 compatibility, and northbound v2
  WebSocket upgrades return `501 Not Implemented`
- single-worker remote runtime now has a northbound v2 reconnect regression
  test, verifying that a later Codex client session can bootstrap and start a
  thread after the gateway reconnects its downstream worker
- that reconnect coverage now also asserts the worker has really returned to a
  healthy single-worker v2 compatibility profile before the later client
  session is admitted

## Current Work

Phase 4 is complete with:

- optional Bearer token middleware for `/v1/*`
- anonymous `GET /healthz` for liveness checks outside the auth boundary
- CLI/config wiring for a local static token in embedded mode
- tenant/project scoping via `x-codex-tenant-id` and `x-codex-project-id`
- per-thread access checks so read/turn/control/SSE paths only expose threads in
  the caller's scope
- request audit logs at the HTTP boundary with method/route/status/duration plus
  tenant/project context
- request count and duration metrics for gateway HTTP routes via the existing
  OTEL provider when metrics export is enabled

Phase 4 now also includes:

- per-scope HTTP request rate limiting keyed by `x-codex-tenant-id` and
  `x-codex-project-id`
- a separate per-scope turn-start quota so one client cannot monopolize worker
  capacity
- `429 Too Many Requests` responses with `retry-after` for northbound clients
- CLI/config wiring for local embedded limits via `--requests-per-minute` and
  `--turn-starts-per-minute`

Phase 5 has started with:

- gateway runtime mode selection between embedded and remote app-server workers
- embedded-mode execution-plane routing via `--exec-server-url`, so a local
  gateway/app-server can delegate process and filesystem work to
  `codex-exec-server` without changing the northbound API
- embedded gateway startup now validates remote `exec-server` connectivity up
  front when `--exec-server-url` is configured, so execution-plane failures are
  caught before the first turn reaches the worker stack
- a first remote worker transport backed by the existing
  `codex_app_server_client` websocket client
- CLI/config wiring for a static remote websocket endpoint and optional bearer
  token
- reuse of the same northbound HTTP/SSE surface and app-server event loop across
  embedded and remote runtimes
- support for a static pool of remote websocket workers instead of only a single
  remote backend
- round-robin worker selection for `POST /v1/threads`, with sticky routing of
  thread reads, turns, interrupts, and server-request responses back to the
  worker that owns the thread
- aggregated thread listing across remote workers with gateway-level pagination
- unhealthy-worker detection from remote disconnect events, failover for new
  thread creation, and degraded-mode thread listing that skips unhealthy
  workers instead of failing the whole request
- `/healthz` now reports embedded vs remote runtime mode and, for remote mode,
  the current per-worker health state plus last observed worker error
- `/healthz` now also reports execution mode so clients can distinguish local
  embedded execution, embedded `exec-server` delegation, and remote
  worker-managed execution
- background reconnect attempts for remote websocket workers after disconnect,
  with automatic return to the worker pool once the app-server session is
  re-established
- SSE `gateway/reconnected` events so northbound clients can observe worker
  recovery

The remaining Phase 5 work is:

- none

Phase 5 evaluation result:

- embedded gateway deployments can move command execution and filesystem access
  out of process via `--exec-server-url`
- gateway startup now fails fast when that remote execution plane is
  unreachable, so the deployment contract is explicit instead of best-effort
- remote gateway deployments should keep execution ownership on the remote
  app-server workers instead of trying to centralize execution in the gateway
  process
- `/healthz` and startup logs now expose enough runtime and execution context
  for operators to distinguish embedded local execution, embedded `exec-server`
  delegation, and remote worker-managed execution

Phase 6 is in progress with:

- a northbound WebSocket JSON-RPC endpoint at `/` for app-server v2 traffic
- embedded-runtime v2 passthrough for `initialize`, typed requests,
  notifications, responses, and server requests
- single-remote-worker v2 passthrough using the same proxy model
- gateway-owned initialize response shaping plus downstream client identity and
  capability propagation
- transport guards for auth, Origin rejection, and explicit `501` behavior when
  v2 compatibility is unavailable in the current runtime topology
- health and startup signals that distinguish embedded compatibility, single
  remote-worker compatibility, and unsupported multi-worker remote topology
- `/healthz` now also exposes the effective v2 transport hardening config
  (`initializeTimeoutSeconds`, `clientSendTimeoutSeconds`, and
  `maxPendingServerRequests`) so rollout validation does not depend only on
  startup logs
- embedded and remote compatibility tests covering initialize, bootstrap
  requests, thread notifications, thread queries, request passthrough, and
  server-request round trips
- expanded embedded compatibility coverage for additional thread control
  requests used during normal client setup, including `thread/list`,
  `thread/name/set`, `thread/memoryMode/set`, and `thread/loaded/list`
- embedded compatibility coverage for app-server's current unmaterialized
  thread semantics, verifying that `thread/resume` and `thread/fork` surface
  the expected `no rollout found` errors before a thread has been materialized
- expanded single-worker remote compatibility coverage for Stage A thread and
  turn passthrough methods used by current clients, including thread
  resume/fork/name/memory updates, loaded-thread discovery, review start, and
  turn steer
- expanded single-worker remote compatibility coverage for the remaining Stage A
  bootstrap passthrough methods, additional thread control passthrough methods,
  and the main approval / elicitation / token-refresh server-request round
  trips
- a real northbound `RemoteAppServerClient` harness now validates single-worker
  remote drop-in parity across bootstrap plus thread read/list/update workflow,
  not just per-method passthrough fixtures
- that real single-worker remote harness now also covers the bootstrap/setup
  methods current clients use beyond account/model discovery, including
  `externalAgentConfig/detect`, `externalAgentConfig/import`, `skills/list`,
  `config/batchWrite`, `memory/reset`, and `account/logout`
- that real single-worker remote harness now also covers a bidirectional
  server-request round trip over the gateway's northbound v2 transport,
  validating that `item/tool/requestUserInput` reaches the client and its typed
  response is proxied back to the owning worker
- that real single-worker remote harness now also covers additional typed
  server-request round trips for `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`,
  `mcpServer/elicitation/request`, and
  `account/chatgptAuthTokens/refresh`, validating that the gateway preserves
  those bidirectional payloads over the northbound v2 transport instead of only
  the `requestUserInput` path
- single-worker remote compatibility coverage now also verifies app-server's
  current unmaterialized thread semantics, preserving the expected
  `no rollout found for thread id ...` errors for `thread/resume` and
  `thread/fork` over the gateway's northbound v2 transport
- that real single-worker remote harness now also covers `turn/start` plus the
  core turn-lifecycle notifications clients rely on during normal execution:
  `thread/status/changed`, `turn/started`, `item/agentMessage/delta`, and
  `turn/completed`
- expanded single-worker remote compatibility coverage for `turn/start` and the
  core turn-lifecycle notifications clients rely on during normal execution
- dedicated v2 parity coverage for streamed item-delta notifications clients
  consume during active turns, starting with `item/agentMessage/delta`
- dedicated v2 parity coverage for experimental realtime request passthrough,
  including `thread/realtime/start`, `thread/realtime/appendAudio`,
  `thread/realtime/appendText`, and `thread/realtime/stop`
- dedicated v2 parity coverage for experimental realtime notification
  forwarding, starting with `thread/realtime/started`
- v2 scope enforcement using the same tenant/project headers as HTTP, including
  gateway-owned thread visibility checks, list filtering, and server-request
  rejection for hidden threads
- v2 admission enforcement using the same per-scope request rate limits and
  turn-start quota policy as HTTP
- per-request v2 metrics and audit logs for northbound JSON-RPC traffic,
  including `initialize` and post-handshake client requests
- a Stage A v2 method coverage matrix derived from current Codex client usage
  in [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)
- explicit northbound WebSocket close signaling when the downstream app-server
  session disconnects, so clients see a concrete close reason instead of a
  silent drop
- an explicit northbound initialize handshake timeout, so v2 connections that
  never complete bootstrap are closed with a concrete gateway-owned reason
- an explicit northbound client-send timeout, so stalled WebSocket writes fail
  closed instead of leaving the gateway blocked on a slow or wedged client
- explicit northbound close signaling for malformed client WebSocket payloads,
  so invalid JSON-RPC text frames and invalid UTF-8 binary frames terminate
  with a concrete gateway-owned close reason instead of only surfacing as
  transport errors in logs
- an explicit per-connection cap on unresolved northbound v2 server requests,
  so a slow or non-responsive client cannot let server-request state grow
  without bound inside one compatibility session
- CLI/config wiring for that unresolved server-request cap via
  `--v2-max-pending-server-requests`, so operators can tune the per-connection
  slow-client bound during rollout instead of relying on a fixed compiled
  default
- dedicated northbound v2 regression tests for gateway-owned scope rejection of
  `thread/resume` history/path bypasses and `thread/fork` path bypasses, so the
  policy is enforced at the actual JSON-RPC boundary instead of only in unit
  tests
- dedicated northbound v2 regression tests for `thread/list` and
  `thread/loaded/list` scope filtering, so hidden threads are removed from
  real JSON-RPC responses instead of only from unit-test fixtures
- dedicated northbound v2 passthrough tests for the remaining Stage A
  thread/turn control requests that current clients issue less frequently,
  including `thread/unsubscribe`, `thread/compact/start`,
  `thread/shellCommand`, `thread/backgroundTerminals/clean`,
  `thread/rollback`, `review/start`, `turn/interrupt`, and `turn/steer`
- the real embedded `RemoteAppServerClient` compatibility harness now also
  covers bootstrap/setup methods current clients use beyond account/model
  discovery, including `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `skills/list`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that real embedded harness now also covers a bidirectional server-request
  round trip over the gateway's northbound v2 transport, validating that
  `item/tool/requestUserInput` reaches the client, the typed response is
  proxied back into the in-process app-server session, and
  `serverRequest/resolved` is forwarded before turn completion
- that real embedded harness now also covers
  `item/permissions/requestApproval`, validating that the gateway preserves the
  in-process approval round trip and `serverRequest/resolved` ordering for the
  request-permissions flow too
- that real embedded harness now also covers
  `mcpServer/elicitation/request`, validating that the gateway preserves the
  in-process MCP elicitation round trip and `serverRequest/resolved` ordering
  for connector-driven tool flows too

The remaining Phase 6 work is:

- validate drop-in client parity more broadly in embedded and single-worker
  remote mode
- continue hardening around load, reconnect, and slow-client operational
  behavior for the northbound v2 transport
- design multi-worker connection coordination without breaking current v2
  semantics
