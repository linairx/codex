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

## Current Work

Phase 4 has started with:

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

The remaining Phase 4 work is:

- none

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
