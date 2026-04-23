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
- that same real embedded harness now also verifies thread re-entry from a
  later northbound v2 client session, so a second client can still
  `thread/resume` and then `thread/read` a previously materialized thread
  after the original client disconnects
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
- northbound v2 transport now also fans out `account/login/start` for the
  external-auth `chatgptAuthTokens` path across multi-worker downstream
  sessions, so gateway-scoped auth state does not drift behind the primary
  worker
- northbound v2 transport now also fails closed if a downstream app-server
  session reuses a still-pending server-request id, preventing silent
  overwrite of the original pending route
- northbound v2 transport now also fails closed if a downstream worker
  disconnects while a connection-scoped server request is still pending or
  awaiting downstream `serverRequest/resolved` on a shared multi-worker
  session, preventing the unresolved prompt from being silently dropped
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
- multi-worker northbound v2 transport now also suppresses exact-duplicate
  `mcpServer/startupStatus/updated` notifications across workers, extending the
  existing connection-scoped dedupe path so one shared client session does not
  surface the same MCP startup state twice
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
  `account/login/start` for external-auth `apiKey` and
  `chatgptAuthTokens`, `externalAgentConfig/import`, `config/value/write`,
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
- dedicated multi-worker northbound regression coverage now also verifies that
  aggregated `plugin/list` preserves merged marketplace state across workers
  and that `plugin/read`, `plugin/install`, and `plugin/uninstall` fall back
  to the first worker that can satisfy the selected plugin request
- single-worker remote runtime now has a northbound v2 reconnect regression
  test, verifying that a later Codex client session can bootstrap and start a
  thread after the gateway reconnects its downstream worker
- that reconnect coverage now also asserts the worker has really returned to a
  healthy single-worker v2 compatibility profile before the later client
  session is admitted
- that reconnect coverage now also verifies the recovered v2 session can still
  complete a bidirectional `item/tool/requestUserInput` server-request round
  trip, not just bootstrap and `thread/start`
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still satisfy `account/read` and `account/rateLimits/read`,
  so bootstrap account state and background rate-limit refresh survive worker
  recovery too
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete a realtime workflow for
  `thread/realtime/listVoices`, `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`, and
  `thread/realtime/stop`, plus the resulting
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session still delivers `account/updated`,
  `account/rateLimits/updated`, `app/list/updated`, and `skills/changed`
- multi-worker remote runtime now also has a northbound v2 reconnect
  regression, verifying that after one worker disconnects and recovers, a
  later client session can again route `thread/start` to the recovered worker
  and still aggregate `thread/list` / `thread/read` with a second healthy
  worker on the same gateway
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
  that same answered-but-unresolved thread-scoped server-request disconnect
  path, validating that one shared northbound client session still sees
  `serverRequest/resolved` and can continue onto later worker-owned threads
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
  `app/list/updated`, and `mcpServer/startupStatus/updated`
- that reconnect regression now also verifies the recovered multi-worker v2
  session still suppresses duplicate `skills/changed` invalidations across
  workers until the client refreshes with `skills/list`, and still emits one
  fresh invalidation after that refresh
- northbound multi-worker v2 connections now also survive loss of one
  downstream worker within the same client session, dropping the failed worker,
  synthesizing `serverRequest/resolved` for that worker's thread-scoped
  in-flight prompts, and continuing to serve `thread/start` plus aggregated
  `thread/list` through the remaining workers
- those same shared multi-worker v2 sessions now also re-add a recovered
  downstream worker lazily on later client requests, so one northbound client
  can resume round-robin `thread/start` and aggregated `thread/list` / sticky
  `thread/read` coverage for the recovered worker without reconnecting
- dedicated northbound multi-worker v2 regression coverage now verifies exact
  duplicate connection-state notifications such as `account/updated` are
  emitted only once on the shared northbound session
- dedicated northbound multi-worker v2 regression coverage now also verifies
  duplicate `skills/changed` invalidations are suppressed until the client
  refreshes with `skills/list`, after which one fresh invalidation is emitted
- northbound multi-worker v2 transport now also drops duplicate downstream
  `serverRequest/resolved` replays after the first request-id translation is
  consumed, so worker-local server-request ids do not leak onto the shared
  northbound session
- northbound v2 transport now also fails closed if a client sends a
  `JSONRPCResponse` or `JSONRPCError` for a server request that is no longer
  pending, instead of silently ignoring that out-of-order protocol traffic
- that same protocol-violation path now also rejects any other still-pending
  downstream server requests before the gateway tears the northbound
  connection down, so partially completed approval / elicitation flows do not
  remain stranded after a bad client reply
- dedicated northbound multi-worker v2 regression coverage now also verifies
  those duplicate downstream `serverRequest/resolved` replays are dropped while
  the shared northbound session remains usable for follow-up requests
- multi-worker v2 `thread/list` aggregation now also deduplicates repeated
  thread ids across workers, selecting the newest visible thread snapshot for
  the shared northbound response instead of surfacing duplicate logical
  threads
- that deduplicated `thread/list` winner now also backfills sticky worker
  ownership, so follow-up thread-scoped requests route to the worker that
  supplied the visible thread snapshot
- multi-worker v2 routing now also probes downstream ownership to recover a
  missing worker route for already-visible threads before forwarding later
  thread-scoped requests, instead of only recovering that route on
  `thread/read`
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for `thread/resume` and `thread/fork`,
  verifying sticky routing on worker-owned threads and route backfill for
  follow-up `thread/read` on the returned forked thread

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
  `externalAgentConfig/detect`, `externalAgentConfig/import`, `app/list`,
  `skills/list`, `mcpServerStatus/list`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
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
- that same real single-worker remote harness now also covers the
  connection-scoped client notifications `account/updated`,
  `account/rateLimits/updated`, `app/list/updated`, and `skills/changed`
- single-worker remote compatibility coverage now also verifies same-scope
  thread re-entry from a later northbound v2 client session, verifying that a
  second client can still `thread/read` a previously created thread after the
  original client disconnects
- that same real single-worker remote harness now also covers onboarding auth
  and feedback flows for `account/login/start`, `account/login/cancel`,
  `account/login/completed`, and `feedback/upload`
- that same real single-worker remote harness now also covers
  `config/read`, `configRequirements/read`, `experimentalFeature/list`, and
  `collaborationMode/list` through the drop-in client transport
- dedicated northbound v2 passthrough coverage now also includes the current
  TUI's onboarding auth and feedback methods: `account/login/start`,
  `account/login/cancel`, and `feedback/upload`
- dedicated northbound v2 server-request round-trip coverage now also includes
  `account/chatgptAuthTokens/refresh`
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
- dedicated northbound v2 passthrough coverage now also includes current TUI
  bootstrap/setup discovery requests outside the main account/model flow:
  `externalAgentConfig/detect`, `app/list`, `skills/list`, and `plugin/list`
- dedicated northbound v2 passthrough coverage now also includes current TUI
  setup-mutation and plugin-management requests:
  `externalAgentConfig/import`, `plugin/read`, `plugin/install`,
  `plugin/uninstall`, `config/batchWrite`, `memory/reset`, and
  `account/logout`
- dedicated northbound v2 passthrough coverage now also includes the core
  thread/turn transport requests current TUI sessions rely on:
  `thread/start`, `thread/resume`, `thread/fork`, `thread/list`,
  `thread/loaded/list`, `thread/read`, `thread/name/set`,
  `thread/memoryMode/set`, and `turn/start`
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
- dedicated v2 parity coverage now also includes item lifecycle notifications
  current clients use during longer-running turns, including `item/started`
  and `item/completed`
- dedicated v2 parity coverage now also includes hook lifecycle notifications
  current clients render during guarded or instrumented runs, including
  `hookStarted` and `hookCompleted`
- dedicated v2 parity coverage now also includes guardian review lifecycle
  notifications for approval auto-review flows, including
  `item/guardianApprovalReviewStarted` and
  `item/guardianApprovalReviewCompleted`
- dedicated v2 parity coverage now also includes richer turn notifications the
  current TUI consumes during active sessions, including `plan/delta`,
  `reasoning/summaryPartAdded`, `terminalInteraction`, `turn/diffUpdated`,
  `turn/planUpdated`, `thread/tokenUsage/updated`, `mcpToolCall/progress`,
  `contextCompacted`, and `model/rerouted`
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
- single-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for realtime workflow parity, covering
  `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`, and
  `thread/realtime/listVoices`, plus delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- single-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for turn-control parity, covering
  `turn/steer` and `turn/interrupt` plus the resulting
  `item/agentMessage/delta`, `turn/completed`, and
  `thread/status/changed` notifications
- single-worker remote runtime now also has a broader real northbound
  `RemoteAppServerClient` regression for lower-frequency thread-control and
  review flows, covering `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- northbound multi-worker v2 regression coverage now also verifies fan-in for
  experimental realtime notifications on one shared client session, including
  cross-worker `thread/realtime/transcript/delta` and
  `thread/realtime/outputAudio/delta`
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for realtime request routing and
  notification fan-in across two workers on one gateway session, verifying
  sticky `thread/realtime/start` / `thread/realtime/appendText` /
  `thread/realtime/stop` routing plus shared-session delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, and `thread/realtime/closed`
- that same real multi-worker realtime harness now also covers
  `thread/realtime/appendAudio` plus shared-session fan-in for the additional
  lower-frequency realtime notifications `thread/realtime/outputAudio/delta`,
  `thread/realtime/sdp`, and `thread/realtime/error`
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for turn-control parity across two
  workers on one gateway session, verifying sticky `turn/steer` and
  `turn/interrupt` routing plus shared-session fan-in for the resulting
  `item/agentMessage/delta`, `turn/completed`, and `thread/status/changed`
  notifications
- multi-worker remote runtime now also has a broader real northbound
  `RemoteAppServerClient` bootstrap/setup regression that exercises one shared
  session through aggregated `account/read`, `account/rateLimits/read`,
  `model/list`, `externalAgentConfig/detect`, `app/list`, `skills/list`, and
  `mcpServerStatus/list`, instead of validating those setup paths only through
  separate targeted passthrough-style regressions
- that same real multi-worker bootstrap/setup coverage now also verifies that
  thread-scoped `app/list` stays sticky to the owning worker instead of
  accidentally reusing the threadless aggregation path
- that same real multi-worker bootstrap/setup coverage now also exercises the
  cwd-aware threadless `config/read`, primary-worker
  `configRequirements/read`, plus aggregated capability discovery for
  `experimentalFeature/list` and `collaborationMode/list`, verifying they
  remain usable through one shared v2 client session without needing
  per-worker client awareness
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for primary-worker onboarding and
  feedback flows, covering `account/login/start`, `account/login/cancel`,
  `account/login/completed`, and `feedback/upload`
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` setup-mutation regression that exercises one shared
  session through `externalAgentConfig/import`, `config/batchWrite`,
  `config/value/write`, `memory/reset`, and `account/logout`, verifying that
  each mutation still fans out to both worker sessions under the same client
  connection
- multi-worker remote runtime now also has a real northbound
  `RemoteAppServerClient` regression for lower-frequency thread-control and
  review routing across two workers on one shared gateway session, covering
  sticky `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- dedicated northbound v2 regression coverage now also verifies forwarding for
  lower-frequency user-visible notifications, including `warning`,
  `configWarning`, `deprecationNotice`, and
  `mcpServer/startupStatus/updated`
- that same dedicated northbound connection-notification coverage now also
  includes `account/login/completed` for onboarding auth completion flows
- the real single-worker remote compatibility harness now also exercises
  `account/login/completed` during an end-to-end onboarding auth flow
- dedicated northbound v2 passthrough coverage now also verifies the
  unsupported-but-transparent `item/tool/call` server-request round trip, so
  the gateway transport is covered even though the current TUI still marks
  that request family unsupported
- v2 scope enforcement using the same tenant/project headers as HTTP, including
  gateway-owned thread visibility checks, list filtering, and server-request
  rejection for hidden threads
- v2 admission enforcement using the same per-scope request rate limits and
  turn-start quota policy as HTTP
- per-request v2 metrics and audit logs for northbound JSON-RPC traffic,
  including `initialize` and post-handshake client requests
- per-connection v2 metrics and audit logs for northbound WebSocket session
  outcomes, including initialize timeouts, downstream disconnects, protocol
  violations, and normal client disconnect/close paths
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
  `externalAgentConfig/import`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `plugin/list`, `plugin/read`,
  `config/batchWrite`, `memory/reset`, and `account/logout`
- that same real embedded compatibility harness now also covers
  `config/read`, `configRequirements/read`, `experimentalFeature/list`, and
  `collaborationMode/list`
- that same real embedded compatibility harness now also covers
  `account/rateLimits/read`, so the client's background rate-limit refresh
  path is exercised through the in-process gateway transport instead of only
  through targeted passthrough fixtures
- that same real embedded compatibility harness now also covers the
  external-auth onboarding flow for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating the
  client-supplied `chatgptAuthTokens` path plus the resulting
  `account/updated` notification
- that same real embedded compatibility harness now also covers
  `feedback/upload`, so operator-facing feedback submission is exercised
  through the in-process gateway transport instead of only through dedicated
  passthrough fixtures
- that same real embedded compatibility harness now also validates that the
  client-supplied `chatgptAuthTokens` are carried through to the downstream
  model request path after onboarding, instead of stopping at account-state
  notifications alone
- that same real single-worker remote compatibility harness now also covers
  the external-auth onboarding flow for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating the
  client-supplied `chatgptAuthTokens` path plus the resulting
  `account/updated` notification through the remote transport
- that same real single-worker remote compatibility harness now also covers
  `account/rateLimits/read`, so the client's background rate-limit refresh
  path is exercised through the gateway-backed remote worker session instead of
  only through targeted passthrough fixtures or aggregated multi-worker tests
- that same real multi-worker remote compatibility harness now also covers
  `account/rateLimits/read`, preserving the primary worker's historical
  top-level snapshot while merging the per-limit map across workers for the
  current Stage B transport
- that same real multi-worker remote compatibility harness now also covers
  the primary-worker external-auth onboarding flow for `account/login/start`,
  `account/login/cancel`, and `account/login/completed`, validating the
  client-supplied `chatgptAuthTokens` path plus the resulting
  `account/updated` notification through the current Stage B transport
- that same real multi-worker remote compatibility harness now also covers
  connection-scoped realtime voice discovery for `thread/realtime/listVoices`,
  aggregating distinct voice inventory across workers while keeping the
  primary-worker defaults stable
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
- that same real embedded compatibility harness now also covers realtime
  request workflow for `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`,
  `thread/realtime/stop`, and `thread/realtime/listVoices`, validating that
  the in-process gateway reaches the realtime sideband transport for start and
  append requests
- that real embedded harness now also covers
  `mcpServer/elicitation/request`, validating that the gateway preserves the
  in-process MCP elicitation round trip and `serverRequest/resolved` ordering
  for connector-driven tool flows too
- that real embedded harness now also covers lower-frequency thread-control
  flows current clients issue less often, including `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- that same real embedded harness now also covers turn-control parity for
  `turn/steer` and `turn/interrupt`, verifying an active turn can be steered
  and later completed as `interrupted` through the gateway transport

The remaining Phase 6 work is:

- validate drop-in client parity more broadly in embedded and single-worker
  remote mode
- continue hardening around load, reconnect, and slow-client operational
  behavior for the northbound v2 transport
- broaden and harden the new multi-worker v2 coordination path until it reaches
  the same parity level as embedded and single-worker remote mode
