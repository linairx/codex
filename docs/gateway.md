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

Roadmap focus:

- Phase 1-5 are complete; the remaining roadmap is entirely about finishing and
  hardening Phase 6
- keep HTTP/SSE as the operator- and platform-oriented API while v2 WebSocket
  remains the compatibility API
- treat embedded mode and single-worker remote mode as the release-quality
  baseline, and treat multi-worker remote mode as the main remaining expansion
  area

Remaining workstreams:

1. Embedded and single-worker parity closure

- validate a broader set of real Codex client bootstrap and steady-state flows
  against the gateway, not only targeted passthrough fixtures
- close remaining parity gaps in plugin, connector, approval, realtime, and
  long-running turn flows that current clients may exercise opportunistically
- verify that reconnect, session restart, and thread re-entry behavior match
  direct app-server expectations closely enough for drop-in client use

Exit criteria:

- an unmodified Codex client can bootstrap, browse existing state, start and
  control turns, complete server-request round trips, and recover from ordinary
  reconnects through the gateway in both embedded and single-worker remote mode

2. Northbound v2 transport hardening

- continue tightening load, backpressure, timeout, and slow-client behavior so
  one stalled v2 client cannot degrade gateway or worker stability
- expand failure-mode handling around downstream disconnects, malformed frames,
  duplicate or out-of-order protocol traffic, and partially completed
  server-request lifecycles
- add more regression coverage for operational edge cases, especially under
  reconnect churn and concurrent in-flight turn activity
- make rollout behavior obvious to operators through health signals, logs, and
  metrics rather than relying on test-only guarantees

Exit criteria:

- v2 connections fail closed with explicit gateway-owned behavior under overload
  or protocol violations
- unresolved server-request, reconnect, and client-send edge cases are bounded
  and observable in production

3. Multi-worker routing and coordination

- broaden the current multi-worker transport beyond bootstrap and targeted
  fan-in scenarios until normal client flows behave like one logical app-server
  session
- keep thread-affine operations sticky to the owning worker while aggregating
  connection-scoped discovery and setup requests at the gateway boundary
- keep server-request routing correct when multiple workers issue overlapping
  request IDs or concurrent approval / elicitation flows on one northbound
  connection
- continue closing parity gaps in aggregated discovery, thread mutation, turn
  control, notification fan-in, reconnect recovery, and worker-health-driven
  routing changes

Exit criteria:

- one v2 client connection can safely span multiple workers without leaking
  worker-local topology into the northbound contract
- thread ownership, notification delivery, and server-request resolution remain
  correct during ordinary worker failures and recoveries

4. Compatibility definition and rollout gate

- keep the supported v2 compatibility profile explicit in
  [docs/gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md)
  and the Stage A method inventory explicit in
  [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)
- decide when multi-worker mode is documented as full compatibility versus a
  narrower bounded profile with explicit caveats
- treat operator documentation, deployment guidance, and runtime feature
  signaling as part of the release gate rather than follow-up cleanup

Exit criteria:

- the supported deployment profiles are documented precisely enough that
  clients and operators know which runtime topologies qualify as drop-in v2
  compatibility
- the gateway can be rolled out with clear guardrails for embedded,
  single-worker remote, and multi-worker remote environments

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
- dedicated northbound v2 regression coverage now verifies that downstream
  backpressure closes the northbound WebSocket with the expected policy close
  code and gateway-owned reason
- gateway-owned v2 WebSocket close reasons are now clamped to protocol-safe
  length, so downstream transport errors cannot produce oversized close frames
- dedicated northbound v2 regression coverage now also verifies that an
  oversized downstream disconnect reason is truncated before the gateway sends
  its northbound close frame
- that close-reason truncation coverage now also exercises the invalid-payload
  close path, verifying that oversized malformed-payload errors are clamped
  before the gateway emits a protocol close frame
- `docs/gateway-v2-compat.md` now includes operator guidance and rollout
  profiles for embedded, single-worker remote, and multi-worker remote
  deployments
- multi-worker remote runtime now has explicit integration coverage for the
  current Stage B transport boundary: `/healthz` reports
  `RemoteMultiWorker` for v2 compatibility, and northbound v2 WebSocket
  upgrades no longer fail closed with `501 Not Implemented`
- northbound v2 transport now establishes one downstream app-server session per
  configured remote worker in multi-worker mode, with gateway-owned routing for
  aggregated thread-list requests and translated server-request IDs
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering per-worker server-request round trips with identical
  downstream request IDs, including `item/tool/requestUserInput`,
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`, and
  `mcpServer/elicitation/request`, plus the connection-scoped
  `account/chatgptAuthTokens/refresh`, verifying that the gateway translates
  those IDs uniquely at the northbound boundary and routes each resolved
  response back to the owning worker session
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering per-thread `turn/start` routing plus turn-lifecycle
  notification fan-in across two workers on one gateway session, verifying
  that turn control stays sticky to the owning worker and the resulting
  `thread/status/changed`, `turn/started`, `item/agentMessage/delta`, and
  `turn/completed` notifications all reach the shared northbound client
- that same multi-worker turn-routing regression now also verifies fan-in for
  additional streamed turn notifications the current TUI consumes:
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and `item/fileChange/outputDelta`
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering thread mutation routing across two workers, verifying
  that `thread/name/set` stays sticky to the owning worker and the aggregated
  `thread/read` / `thread/list` results reflect each worker's renamed thread
  state on the shared northbound connection
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering tenant/project scope enforcement across worker-owned
  threads, verifying that same-scope clients can re-enter aggregated
  `thread/list` / `thread/loaded/list` / `thread/read` state while other
  tenant/project headers see filtered lists and `thread not found` for hidden
  reads
- multi-worker remote runtime now also fans out connection-scoped state
  mutation requests that current clients use during setup, including
  `externalAgentConfig/import`, `config/value/write`,
  `config/batchWrite`, `memory/reset`, and `account/logout`, so
  worker-local session/config state does not silently drift behind one shared
  northbound v2 connection
- multi-worker remote runtime now also aggregates setup-time discovery requests
  that should reflect more than one worker, including
  `externalAgentConfig/detect`, threadless `app/list`,
  `mcpServerStatus/list`, and `skills/list`,
  deduplicating imported-config items, paginating the merged app inventory at
  the gateway boundary, paginating merged MCP inventory, and merging
  per-`cwd` skill results instead of exposing only the primary worker's local
  view
- multi-worker remote runtime now also aggregates bootstrap-critical
  `account/read`, `account/rateLimits/read`, and `model/list` requests,
  OR-ing `requiresOpenaiAuth` across workers, merging the per-limit
  rate-limit map, and unioning model inventory for the current unpaginated
  startup path instead of exposing only the primary worker's bootstrap view
- multi-worker remote runtime now also deduplicates connection-scoped
  `skills/changed` invalidation notifications across worker sessions until the
  client refreshes with `skills/list`, so one shared northbound v2 connection
  no longer triggers redundant skills reloads when multiple workers observe the
  same local skill change
- multi-worker remote runtime now also suppresses exact-duplicate
  connection-state notifications across worker sessions for
  `account/updated`, `account/rateLimits/updated`, and `app/list/updated`, so
  one shared northbound v2 connection does not receive repeated identical
  account, rate-limit, or connector-state updates when multiple workers emit
  the same current-state payload
- multi-worker remote runtime now also aggregates threadless `plugin/list`
  discovery across worker sessions and routes `plugin/read`,
  `plugin/install`, and `plugin/uninstall` to the worker that can satisfy the
  selected plugin request, so one shared northbound v2 connection can manage
  worker-local plugin state instead of exposing only the primary worker's
  marketplace view
- single-worker remote runtime now has a northbound v2 reconnect regression
  test, verifying that a later Codex client session can bootstrap and start a
  thread after the gateway reconnects its downstream worker
- that reconnect coverage now also asserts the worker has really returned to a
  healthy single-worker v2 compatibility profile before the later client
  session is admitted
- that reconnect coverage now also verifies the recovered v2 session can still
  complete a bidirectional `item/tool/requestUserInput` server-request round
  trip, not just bootstrap and `thread/start`
- multi-worker remote runtime now also has a northbound v2 reconnect
  regression, verifying that after one worker disconnects and recovers, a
  later client session can again route `thread/start` to the recovered worker
  and still aggregate `thread/list` / `thread/read` with a second healthy
  worker on the same gateway
- that reconnect regression now also verifies the recovered multi-worker v2
  session can still complete a bidirectional `item/tool/requestUserInput`
  server-request round trip with the recovered worker, not just bootstrap and
  thread routing
- that reconnect regression now also verifies the recovered multi-worker v2
  session still suppresses exact-duplicate connection-state notifications
  across workers for `account/updated`, `account/rateLimits/updated`, and
  `app/list/updated`
- that reconnect regression now also verifies the recovered multi-worker v2
  session still suppresses duplicate `skills/changed` invalidations across
  workers until the client refreshes with `skills/list`, and still emits one
  fresh invalidation after that refresh
- northbound multi-worker v2 connections now also survive loss of one
  downstream worker within the same client session, dropping the failed worker,
  synthesizing `serverRequest/resolved` for that worker's thread-scoped
  in-flight prompts, and continuing to serve `thread/start` plus aggregated
  `thread/list` through the remaining workers
- dedicated northbound multi-worker v2 regression coverage now verifies exact
  duplicate connection-state notifications such as `account/updated` are
  emitted only once on the shared northbound session
- dedicated northbound multi-worker v2 regression coverage now also verifies
  duplicate `skills/changed` invalidations are suppressed until the client
  refreshes with `skills/list`, after which one fresh invalidation is emitted

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
- multi-worker remote v2 transport support in the current Stage B profile, with
  one downstream app-server session per worker plus gateway-owned routing and
  aggregation where needed
- gateway-owned initialize response shaping plus downstream client identity and
  capability propagation
- transport guards for auth, Origin rejection, and explicit fail-closed
  behavior when v2 compatibility is unavailable in the current runtime
  topology
- health and startup signals that distinguish embedded compatibility,
  single-remote-worker compatibility, and the current partial multi-worker
  remote compatibility profile
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
- the real embedded `RemoteAppServerClient` harness now also covers
  `turn/start` plus the core turn-lifecycle notifications clients rely on
  during normal execution: `thread/status/changed`, `turn/started`,
  `item/agentMessage/delta`, and `turn/completed`
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
- that same real single-worker remote harness now also covers additional
  streamed turn notifications the current TUI consumes during active runs:
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and `item/fileChange/outputDelta`
- expanded single-worker remote compatibility coverage for `turn/start` and the
  core turn-lifecycle notifications clients rely on during normal execution
- dedicated v2 parity coverage for streamed item-delta notifications clients
  consume during active turns, starting with `item/agentMessage/delta`
- dedicated v2 parity coverage for experimental realtime request passthrough,
  including `thread/realtime/start`, `thread/realtime/appendAudio`,
  `thread/realtime/appendText`, and `thread/realtime/stop`
- dedicated v2 parity coverage for experimental realtime notification
  forwarding, starting with `thread/realtime/started`
- dedicated v2 parity coverage now also includes additional experimental
  realtime notification forwarding that current clients consume, including
  `thread/realtime/itemAdded`, `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- northbound multi-worker v2 regression coverage now also verifies fan-in for
  experimental realtime notifications on one shared client session, including
  cross-worker `thread/realtime/transcript/delta` and
  `thread/realtime/outputAudio/delta`
- dedicated northbound v2 regression coverage now also verifies forwarding for
  lower-frequency user-visible notifications, including `warning`,
  `configWarning`, `deprecationNotice`, and
  `mcpServer/startupStatus/updated`
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
- dedicated northbound v2 regression coverage now exercises that malformed
  payload handling on both sides of the handshake boundary: invalid JSON-RPC
  text frames before and after initialize, plus invalid UTF-8 binary frames
  before and after initialize
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
- an explicit per-connection cap on unresolved northbound v2 server requests,
  so a slow or non-responsive client cannot let server-request state grow
  without bound inside one compatibility session
- CLI/config wiring for that unresolved server-request cap via
  `--v2-max-pending-server-requests`, so operators can tune the per-connection
  slow-client bound during rollout instead of relying on a fixed compiled
  default
- dedicated northbound v2 regression coverage for that unresolved
  server-request cap, verifying that once one forwarded server request is still
  pending, the gateway rejects a second downstream server request with a
  rate-limited JSON-RPC error while keeping the compatibility session usable
- northbound v2 connections now also reject any still-pending downstream
  server requests when a northbound connection ends mid-flow, including client
  disconnects and gateway-owned protocol closes for malformed client payloads,
  plus other terminal northbound I/O failures such as client-send timeouts or
  broken pipes,
  so approval or user-input requests fail closed instead of being silently left
  unresolved in the downstream app-server session
- dedicated northbound v2 regression tests for gateway-owned scope rejection of
  `thread/resume` history/path bypasses and `thread/fork` path bypasses, so the
  policy is enforced at the actual JSON-RPC boundary instead of only in unit
  tests
- dedicated northbound v2 regression tests for `thread/list` and
  `thread/loaded/list` scope filtering, so hidden threads are removed from
  real JSON-RPC responses instead of only from unit-test fixtures
- multi-worker northbound v2 aggregation now also backfills missing
  worker-route ownership for already visible threads discovered through
  `thread/list` and `thread/loaded/list`, so follow-up `thread/read` and other
  sticky thread operations do not fall back to the primary worker when the
  gateway only knew scope ownership before discovery
- dedicated northbound v2 passthrough tests for the remaining Stage A
  thread/turn control requests that current clients issue less frequently,
  including `thread/unsubscribe`, `thread/compact/start`,
  `thread/shellCommand`, `thread/backgroundTerminals/clean`,
  `thread/rollback`, `review/start`, `turn/interrupt`, and `turn/steer`
- the real embedded `RemoteAppServerClient` compatibility harness now also
  covers bootstrap/setup methods current clients use beyond account/model
  discovery, including `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `app/list`, `skills/list`, `plugin/list`,
  `plugin/read`, `config/batchWrite`, `memory/reset`, and `account/logout`
- that same embedded compatibility harness now also covers post-bootstrap
  plugin management flows current clients use in the TUI, including
  `plugin/install` and `plugin/uninstall`, and verifies the resulting
  `plugin/list` installed-state transitions
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
  `item/commandExecution/requestApproval` and
  `item/fileChange/requestApproval`, validating that the gateway preserves the
  in-process approval round trips and `serverRequest/resolved` ordering for
  shell and patch approval flows too
- that real embedded harness now also covers
  `mcpServer/elicitation/request`, validating that the gateway preserves the
  in-process MCP elicitation round trip and `serverRequest/resolved` ordering
  for connector-driven tool flows too

The remaining Phase 6 work is:

- validate drop-in client parity more broadly in embedded and single-worker
  remote mode
- continue hardening around load, reconnect, and slow-client operational
  behavior for the northbound v2 transport
- broaden and harden the new multi-worker v2 coordination path until it reaches
  the same parity level as embedded and single-worker remote mode
