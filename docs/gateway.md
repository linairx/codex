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

- preserve the current release-quality baseline for embedded and
  single-worker remote mode as more real Codex client flows are exercised
- keep extending broad end-to-end client validation beyond targeted
  passthrough fixtures so regressions in bootstrap, approvals, command
  execution, realtime, and longer-running turn flows are caught at the
  compatibility boundary
- continue verifying reconnect, session restart, and thread re-entry behavior
  against direct app-server expectations closely enough for drop-in client use

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

- broaden the current multi-worker Stage B transport until normal client flows
  behave like one logical app-server session instead of a bounded validation
  profile
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
- keep embedded and single-worker remote documented as the release-quality
  drop-in baseline until multi-worker remote reaches equivalent parity and
  hardening
- decide when multi-worker mode is ready to move from a bounded Stage B profile
  with explicit caveats to full compatibility guidance
- treat operator documentation, deployment guidance, and runtime feature
  signaling as part of the release gate rather than follow-up cleanup

Exit criteria:

- the supported deployment profiles are documented precisely enough that
  clients and operators know which runtime topologies qualify as drop-in v2
  compatibility
- the gateway can be rolled out with clear guardrails for embedded,
  single-worker remote, and multi-worker remote environments

5. Project-aware account routing and quota failover

Status: same-project affinity, account-capacity tracking, quota-aware
new-thread selection, and bounded resumable-thread handoff are implemented;
release-quality multi-worker rollout guidance and arbitrary live
active-context migration remain planned

Goal:

- let multiple Codex clients share one gateway while routing work by project
  and account capacity instead of only by worker health or round-robin order

Required behavior:

- identify whether incoming clients belong to the same logical project using
  the gateway request scope, explicit project metadata, or a future configured
  project identity resolver
- keep same-project sessions sticky to the same account or account-backed
  worker when possible, so model-side and worker-local caches have the highest
  chance of being reused
- distribute different projects across different account-backed workers when
  capacity allows, so unrelated projects do not unnecessarily contend for one
  account quota
- track account capacity and quota state separately from worker process health,
  including rate-limit, billing, authentication, and temporary exhaustion
  signals
- when an account becomes exhausted, select an eligible replacement account and
  transfer the active context by using protocol-visible state such as thread
  resume, thread fork, or serialized conversation history rather than assuming
  in-memory worker state can be moved directly
- preserve gateway scope, audit, approval, server-request, and notification
  semantics across any account handoff, including a clear user/operator-visible
  event when work moves to a new account-backed worker

Design constraints:

- account routing must not weaken tenant/project isolation; a project may only
  be routed to accounts that policy allows for that tenant and project
- cache affinity is an optimization, not a reason to keep using an exhausted or
  disallowed account
- context transfer must be explicit and recoverable; if a live turn or pending
  server request cannot be moved safely, the gateway should fail closed or ask
  the client to restart from a resumable state instead of silently dropping
  context
- account failover must be observable through `/healthz`, audit logs, and
  metrics, including the original account-backed worker, the replacement, the
  reason for handoff, and whether context restoration succeeded

Exit criteria:

- same-project clients consistently route to the configured preferred account
  or account-backed worker while it is healthy and has quota
- different projects can be spread across different account-backed workers by
  policy
- quota exhaustion on one account triggers a bounded, observable handoff to an
  eligible replacement account when the current context is safely resumable
- if no eligible account exists, or if the current context cannot be safely
  restored, the gateway returns an explicit fail-closed error instead of
  silently retrying on an unrelated account

Overall release criteria:

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
- project-scoped gateway requests can now record a project-to-worker affinity
  route after a successful remote `thread/start`, and later same-project
  `thread/start` requests prefer that worker when it is still available; this
  cache-affinity foundation has since been extended with account-capacity
  tracking, quota-aware new-thread selection, and bounded handoff surfaces for
  explicitly resumable thread state, while arbitrary live active-context
  migration remains out of scope
- remote `/healthz` now exposes the current project-to-worker affinity table as
  `projectWorkerRoutes`, including whether the pinned worker is currently
  healthy and the configured `accountId` for that worker when one is supplied,
  giving operators a concrete view of which tenant/project scopes are pinned
  to which account-backed worker and whether that account-backed route remains
  eligible for quota-aware routing or bounded restoration
- remote worker configuration now accepts per-worker account identity via
  `--remote-account-id`, and remote `/healthz` carries that identity through
  both `remoteWorkers[].accountId` and `projectWorkerRoutes[].accountId`; those
  labels now feed project-aware distribution, account-capacity health, and
  bounded new-thread failover rather than serving only as identity metadata
- project-scoped remote `thread/start` selection now uses configured
  `accountId` labels as an initial distribution signal: once one project is
  pinned to an account-backed worker, a different project in the same tenant
  prefers a less-loaded account when one is healthy and eligible, while
  existing same-project affinity still wins
- remote worker health now tracks account capacity separately from worker
  process health: `/healthz` exposes `accountCapacity`,
  `accountCapacityReason`, and `accountCapacityLastChangedAt` for each worker,
  and `projectWorkerRoutes` carries the current `accountCapacity` for the
  pinned route
- HTTP `thread/start` now treats downstream quota, rate-limit, billing, credits,
  and 429 JSON-RPC failures as account-capacity exhaustion signals instead of
  worker-health failures; later project-scoped selection avoids exhausted
  account-backed workers and retries another healthy account when one is
  available, but this still does not transfer active in-memory context
- HTTP `thread/start` quota failover now emits
  `gateway_account_capacity_events` metrics for exhausted workers and
  successful replacement routes, and writes structured logs with
  tenant/project scope, exhausted worker/account identity, replacement
  worker/account identity, and the downstream quota reason, matching the v2
  new-thread failover observability path
- HTTP `thread/start` quota failover also publishes `/v1/events` operator
  events for `gateway/accountCapacityExhausted` and
  `gateway/accountFailoverSucceeded`, so account-backed worker handoff is
  visible to event-stream consumers in addition to `/healthz`, metrics, and
  logs
- HTTP `turn/start` now fails closed when the visible thread is pinned to an
  account-backed worker already marked exhausted, emits
  `gateway_account_capacity_events` with `active_thread_handoff_failure`, and
  publishes `gateway/accountActiveThreadHandoffFailed` instead of silently
  continuing active work on an exhausted account or moving live context to
  another worker
- HTTP `turn/start` now also treats downstream quota-like failures as
  account-capacity exhaustion signals, marking the owning account-backed
  worker exhausted and publishing `gateway/accountCapacityExhausted` so later
  active-thread requests fail closed before attempting more work on that
  account
- HTTP `turn/interrupt` now uses the same active-thread account-capacity guard
  as `turn/start`, so an interrupt for a thread pinned to an exhausted
  account-backed worker fails closed and publishes
  `gateway/accountActiveThreadHandoffFailed` instead of continuing turn control
  on an account known to be out of capacity
- HTTP `serverRequest/respond` now records the pending request thread id and
  applies that same exhausted-account fail-closed guard before forwarding
  approval or user-input responses to the owning worker, keeping active
  server-request round trips from silently continuing on an account already
  known to be out of capacity
- a real remote HTTP regression now covers that active server-request response
  fail-closed path end to end: a pending `serverRequest/requested` is left on
  one account-backed worker, another worker with the same `accountId` reports a
  429 quota failure, and `/v1/server-requests/respond` returns the explicit
  exhausted-account error plus `gateway/accountActiveThreadHandoffFailed`
- HTTP `serverRequest/respond` now keeps the pending server request registered
  until the downstream response is successfully forwarded, so a missing worker
  route or transport failure does not silently drop a partially completed
  approval or user-input lifecycle
- HTTP `serverRequest/respond` delivery failures now emit
  `gateway_server_request_lifecycle_events` for answered and delivery-failed
  stages plus `gateway_server_request_answer_delivery_failures`, and write
  structured logs with tenant/project scope, response kind, worker route,
  request id, thread id, and error details when the downstream handoff cannot
  complete; embedded and remote runtime paths now both keep the pending
  request registered until the downstream response is accepted
- HTTP `serverRequest/respond` now checks exhausted account capacity from the
  pending server-request worker route itself, so a partially completed approval
  or user-input round trip still fails closed even if the thread route table no
  longer has the owning worker entry
- that exhausted-account HTTP `serverRequest/respond` fail-closed branch now
  also records the answered and delivery-failed server-request lifecycle
  metrics plus the direct answer-delivery-failure counter, so quota-driven
  refusal is observable the same way as missing-route and transport delivery
  failures
- HTTP `serverRequest/respond` now records
  `client_server_request_invalid_response` lifecycle metrics and structured
  tenant/project logs when a client answers a pending server request with the
  wrong response type, keeping the pending request registered while making the
  client-side mismatch visible to operators
- HTTP `thread/read` now treats a visible thread id as a bounded account
  restoration surface: if the cached worker route points at an exhausted
  account-backed worker, the gateway skips that account, attempts the read on
  another healthy worker with available account capacity, updates the sticky
  thread route on success, and publishes
  `gateway/accountThreadHandoffSucceeded`; if no replacement can restore the
  read, or if a replacement returns a different thread id than the requested
  one, it emits `thread_read_handoff_failure` account-capacity metrics and
  publishes `gateway/accountThreadHandoffFailed` instead of silently reading
  through the exhausted account or accepting the wrong context
- `/healthz` now exposes `pendingServerRequestCount` and
  `pendingServerRequestKindCounts` for the gateway HTTP/SSE surface, so
  partially completed approval and user-input lifecycles are visible while
  they are still waiting for a successful downstream delivery or explicit
  resolution, without exposing individual request ids
- `/healthz` now also exposes `pendingServerRequestRouteCounts`, grouping those
  HTTP/SSE pending server requests by owning worker route plus response kind,
  so operators can see whether approval or user-input buildup is concentrated
  on one remote worker or on the embedded route without exposing individual
  request ids
- `/healthz` now also exposes `pendingServerRequestOldestAt`, giving operators
  the Unix timestamp of the oldest partially completed HTTP/SSE server-request
  lifecycle so buildup age is visible without exposing individual request ids
- the HTTP remote account-capacity failover regression now opens the real
  `/v1/events` SSE stream before triggering a quota-like `thread/start`
  failure, and verifies both account-exhaustion and replacement-route events
  are delivered to operators
- northbound v2 multi-worker `thread/start` now carries the configured
  per-worker `accountId` labels into the connection-local router, so
  project-scoped v2 clients use the same less-loaded-account preference for
  new projects that HTTP `thread/start` uses; active in-memory context still
  only moves through explicit protocol-visible restoration surfaces, not
  transparent live migration
- northbound v2 multi-worker `thread/start` now also consults the shared
  remote-worker health registry before applying project affinity, so an
  existing same-project route to an account currently marked exhausted is
  bypassed in favor of a worker with available account capacity; v2 still does
  not transfer active context after a quota failure
- northbound v2 multi-worker `thread/start` now marks quota-like downstream
  JSON-RPC failures as account-capacity exhaustion and retries the same new
  thread request against another available account-backed worker when one is
  eligible; this is bounded failover for new thread creation only and still
  does not transfer active in-memory context
- northbound v2 multi-worker account capacity now also synchronizes from
  successful `account/rateLimits/read` responses and
  `account/rateLimits/updated` notifications, so operator-visible capacity can
  recover to available after fresh rate-limit state shows no exhausted bucket
  instead of only changing on `thread/start` failure or success paths
- northbound v2 `thread/start` quota failover now emits
  `gateway_v2_account_capacity_events` metrics for exhausted workers and
  successful replacement routes, and writes structured logs with
  tenant/project scope, exhausted worker/account identity, replacement
  worker/account identity, and the downstream quota reason; it also publishes
  `/v1/events` operator events for `gateway/accountCapacityExhausted` and
  `gateway/accountFailoverSucceeded`, so the bounded new-thread failover path
  is visible outside `/healthz`
- `/healthz.v2Connections.accountCapacityEventCounts` now also exposes
  cumulative v2 account-capacity event counts by event name, and
  `accountCapacityEventWorkerCounts` breaks those same events down per worker,
  while `lastAccountCapacityEvent`, `lastAccountCapacityEventWorkerId`,
  `lastAccountCapacityEventTenantId`, `lastAccountCapacityEventProjectId`,
  `lastAccountCapacityEventReason`, and `lastAccountCapacityEventAt` identify
  the newest event, affected worker, tenant/project scope, reason, and Unix
  timestamp, so operators can see whether quota exhaustion, bounded handoff
  success, or fail-closed no-handoff decisions have occurred recently without
  relying only on metrics export or the live `/v1/events` stream
- northbound v2 thread-scoped requests now fail closed when the thread is
  pinned to a worker whose account is already marked exhausted, rather than
  silently moving active context to a different account-backed worker without a
  protocol-visible resume or handoff path
- northbound v2 explicit `thread/resume` by thread id now treats that
  protocol-visible resume request as a bounded account-handoff surface: if the
  visible thread is pinned to an exhausted account-backed worker, the gateway
  skips that worker, attempts the resume on another available account, accepts
  only a response for the requested thread id, updates sticky thread routing on
  success, and fails closed if no replacement restores the thread
- that explicit thread-id resume handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_resume_handoff_success` or `thread_resume_handoff_failure`, writes
  structured tenant/project/worker/account logs, and publishes `/v1/events`
  operator events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- northbound v2 `thread/fork` by thread id now uses the same bounded
  account-handoff surface as explicit `thread/resume`: if the visible source
  thread is pinned to an exhausted account-backed worker, the gateway attempts
  the fork on another available account, updates sticky routing for the forked
  thread on success, and fails closed when no replacement can restore the
  request
- that explicit thread-id fork handoff path emits
  `gateway_v2_account_capacity_events` as `thread_fork_handoff_success` or
  `thread_fork_handoff_failure`, writes structured tenant/project/worker/account
  logs, and publishes `/v1/events` operator events as
  `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- the real multi-worker legacy compatibility harness now also verifies that an
  explicit `thread/resume` by thread id can restore a visible thread through a
  replacement account-backed worker after the cached owner account is marked
  exhausted, and that `/v1/events` publishes
  `gateway/accountThreadHandoffSucceeded` with the old and replacement
  worker/account identities; the same harness now also verifies a follow-up
  `thread/read` stays sticky to the replacement worker after the successful
  handoff
- that same real multi-worker legacy compatibility harness now also verifies
  the successful explicit `thread/fork` by thread id restoration path through
  an unmodified `RemoteAppServerClient` session, including the
  `gateway/accountThreadHandoffSucceeded` operator event with the old and
  replacement worker/account identities
- the same real multi-worker legacy compatibility harness now also covers the
  no-replacement explicit `thread/resume` branch: when every eligible
  account-backed worker is marked exhausted, the gateway fails closed and
  publishes `gateway/accountThreadHandoffFailed` instead of retrying on an
  exhausted account or silently moving active state
- that real no-replacement harness now also covers explicit `thread/fork` by
  thread id: when every eligible account-backed worker is exhausted, the
  gateway fails closed, keeps the existing source-thread route unchanged, and
  publishes `gateway/accountThreadHandoffFailed` with method `thread/fork`
- that exhausted-account fail-closed path is now covered by the same
  `gateway_v2_fail_closed_requests` metric and structured route diagnostic log
  as unavailable-worker fail-closed requests, including the no-reconnect case
  where all worker sessions are present but the owning account lacks capacity
- that same active-thread fail-closed path now also emits a
  `gateway_v2_account_capacity_events` metric with
  `active_thread_handoff_failure` and publishes a `/v1/events` operator event
  as `gateway/accountActiveThreadHandoffFailed`, so clients and operators can
  distinguish a deliberate no-handoff decision from an ordinary worker-route
  failure
- active thread-scoped v2 requests that do not have a protocol-visible restore
  surface are now explicitly regression-covered as fail-closed when the owning
  account-backed worker is exhausted, including `turn/start`,
  `turn/steer`, `turn/interrupt`, thread-scoped `app/list`, `thread/unsubscribe`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/realtime/start`,
  `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`,
  `mcpServer/resource/read`, `mcpServer/tool/call`, and `review/start`, so the
  gateway does not silently replay live or side-effecting work on another
  account
- northbound v2 path-based `thread/resume`, `thread/fork`, and
  `getConversationSummary` now treat protocol-visible rollout paths as the
  first bounded account-handoff surface: if the cached path route points at an
  exhausted account-backed worker, the gateway skips that worker, tries to
  restore the request on another available worker, updates the visible route on
  success, and emits `gateway_v2_account_capacity_events` plus structured
  tenant/project/worker/account logs for the replacement route; if no
  replacement restores the path, the gateway emits a
  `path_thread_handoff_failure` account-capacity event with structured
  tenant/project/worker/account logs and fails closed instead of silently using
  the exhausted account
- path-based v2 `thread/resume` and `getConversationSummary.rolloutPath` now
  also reject replacement-worker responses that point at a different rollout
  path, leaving the cached path route unchanged and publishing the same
  fail-closed account path handoff event instead of accepting the wrong
  restored context
- those v2 path-based account handoff success and failure paths now also
  publish `/v1/events` operator events as
  `gateway/accountPathHandoffSucceeded` and
  `gateway/accountPathHandoffFailed`, carrying the tenant/project scope,
  method, rollout path, exhausted worker/account identity, and replacement
  worker/account identity when restoration succeeds
- legacy v2 `getConversationSummary` by `conversationId` now also treats that
  visible thread id as a bounded account-handoff surface: if the cached worker
  account is exhausted, the gateway attempts the same summary request on
  another available account, accepts only a summary for the requested
  conversation id, updates the returned conversation/path route on success, and
  fails closed if no replacement can restore the summary
- northbound v2 `thread/read` now also treats the visible thread id as a
  bounded account-handoff surface: if the cached worker account is exhausted,
  the gateway attempts the read on another available account, accepts only a
  response for the requested thread id, updates sticky thread routing on
  success, and fails closed without mutating the existing route when no
  replacement restores the read
- northbound v2 `thread/name/set` now uses that same bounded account-handoff
  surface for visible thread renames: if the cached worker account is
  exhausted, the gateway attempts the rename on another available account,
  updates sticky thread routing on success from the requested thread id, and
  fails closed without mutating the existing route when no replacement
  restores the rename
- northbound v2 `thread/memoryMode/set` now uses that same bounded
  account-handoff surface for memory-mode changes: if the cached worker
  account is exhausted, the gateway attempts the update on another available
  account, updates sticky thread routing on success from the requested thread
  id, and fails closed without mutating the existing route when no replacement
  restores the memory-mode update
- northbound v2 `thread/unarchive` now uses the same bounded account-handoff
  surface as `thread/read`: if the cached worker account is exhausted, the
  gateway attempts the unarchive on another available account, accepts only a
  response for the requested thread id, updates sticky thread routing on
  success, and fails closed without mutating the existing route when no
  replacement restores the unarchive
- northbound v2 `thread/archive` now uses the same bounded account-handoff
  surface as `thread/unarchive`: if the cached worker account is
  exhausted, the gateway attempts the archive on another available account,
  updates sticky thread routing on success from the requested thread id, and
  fails closed without mutating the existing route when no replacement
  restores the archive
- northbound v2 `thread/turns/list` now uses the same bounded account-handoff
  surface for history pagination: if the cached worker account is exhausted,
  the gateway attempts the turns-list request on another available account,
  updates sticky thread routing on success from the requested thread id, and
  fails closed without mutating the existing route when no replacement
  restores the history page
- northbound v2 `thread/increment_elicitation` now uses the same bounded
  account-handoff surface for elicitation counter updates: if the cached
  worker account is exhausted, the gateway attempts the increment on another
  available account, updates sticky thread routing on success from the
  requested thread id, and fails closed without mutating the existing route
  when no replacement restores the counter update
- northbound v2 `thread/decrement_elicitation` now uses the same bounded
  account-handoff surface for elicitation counter updates: if the cached
  worker account is exhausted, the gateway attempts the decrement on another
  available account, updates sticky thread routing on success from the
  requested thread id, and fails closed without mutating the existing route
  when no replacement restores the counter update
- northbound v2 `thread/inject_items` now uses the same bounded
  account-handoff surface for visible thread item injection: if the cached
  worker account is exhausted, the gateway attempts the injection on another
  available account, updates sticky thread routing on success from the
  requested thread id, and fails closed without mutating the existing route
  when no replacement restores the injection
- northbound v2 `thread/metadata/update` now also uses that bounded
  account-handoff surface: if the cached worker account is exhausted, the
  gateway attempts the metadata update on another available account, accepts
  only a response for the requested thread id, updates sticky thread routing
  on success, and fails closed without mutating the existing route when no
  replacement restores the update
- northbound v2 `thread/rollback` now also uses that bounded account-handoff
  surface: if the cached worker account is exhausted, the gateway attempts the
  rollback on another available account, accepts only a response for the
  requested thread id, updates sticky thread routing on success, and fails
  closed without mutating the existing route when no replacement restores the
  rollback
- that conversation-summary thread-id handoff path emits
  `gateway_v2_account_capacity_events` as
  `conversation_summary_handoff_success` or
  `conversation_summary_handoff_failure`, writes structured
  tenant/project/worker/account logs, and publishes `/v1/events` operator
  events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/read` handoff path emits
  `gateway_v2_account_capacity_events` as `thread_read_handoff_success` or
  `thread_read_handoff_failure` and publishes `/v1/events` operator events as
  `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`, matching the other explicit thread-id
  restoration surfaces
- that v2 `thread/name/set` handoff path emits
  `gateway_v2_account_capacity_events` as `thread_name_set_handoff_success`
  or `thread_name_set_handoff_failure` and publishes `/v1/events` operator
  events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/memoryMode/set` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_memory_mode_set_handoff_success` or
  `thread_memory_mode_set_handoff_failure` and publishes `/v1/events`
  operator events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/unarchive` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_unarchive_handoff_success` or
  `thread_unarchive_handoff_failure` and publishes `/v1/events` operator
  events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/archive` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_archive_handoff_success` or
  `thread_archive_handoff_failure` and publishes `/v1/events` operator events
  as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/turns/list` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_turns_list_handoff_success` or
  `thread_turns_list_handoff_failure` and publishes `/v1/events` operator
  events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/increment_elicitation` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_increment_elicitation_handoff_success` or
  `thread_increment_elicitation_handoff_failure` and publishes `/v1/events`
  operator events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/decrement_elicitation` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_decrement_elicitation_handoff_success` or
  `thread_decrement_elicitation_handoff_failure` and publishes `/v1/events`
  operator events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/inject_items` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_inject_items_handoff_success` or
  `thread_inject_items_handoff_failure` and publishes `/v1/events` operator
  events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/metadata/update` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_metadata_update_handoff_success` or
  `thread_metadata_update_handoff_failure` and publishes `/v1/events`
  operator events as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- that v2 `thread/rollback` handoff path emits
  `gateway_v2_account_capacity_events` as
  `thread_rollback_handoff_success` or
  `thread_rollback_handoff_failure` and publishes `/v1/events` operator events
  as `gateway/accountThreadHandoffSucceeded` or
  `gateway/accountThreadHandoffFailed`
- the real multi-worker v2 compatibility harness now also drives direct
  `thread/read` through an unmodified `RemoteAppServerClient` after
  `account/rateLimits/read` marks the cached owner account exhausted, verifying
  replacement-account restoration plus the matching
  `gateway/accountThreadHandoffSucceeded` operator event
- the real no-replacement thread-id handoff harness now also covers direct
  `thread/read`: when every eligible account-backed worker is exhausted, the
  gateway fails closed, keeps the cached thread route unchanged, and publishes
  `gateway/accountThreadHandoffFailed` with method `thread/read`
- the real multi-worker v2 compatibility harness now also drives direct
  `thread/rollback` through an unmodified `RemoteAppServerClient` after
  `account/rateLimits/read` marks the cached owner account exhausted,
  verifying replacement-account restoration plus the matching
  `gateway/accountThreadHandoffSucceeded` operator event
- the real no-replacement thread-id handoff harness now also covers direct
  `thread/rollback`: when every eligible account-backed worker is exhausted,
  the gateway fails closed, keeps the cached thread route unchanged, and
  publishes `gateway/accountThreadHandoffFailed` with method
  `thread/rollback`
- the real multi-worker legacy compatibility harness now also drives
  `account/rateLimits/read` to mark the cached rollout-path worker exhausted,
  then verifies `getConversationSummary.rolloutPath` restores through another
  account-backed worker on an unmodified `RemoteAppServerClient` session; that
  harness now also watches the real `/v1/events` SSE stream and verifies the
  resulting `gateway/accountPathHandoffSucceeded` operator event
- that same real multi-worker legacy compatibility harness now also verifies
  `getConversationSummary.conversationId` restores through a replacement
  account-backed worker after the cached owner account is marked exhausted,
  including the `gateway/accountThreadHandoffSucceeded` operator event
- the same real multi-worker legacy compatibility harness now also covers the
  no-replacement rollout-path branch: when every eligible account-backed worker
  is marked exhausted, `getConversationSummary.rolloutPath` fails closed and
  publishes `gateway/accountPathHandoffFailed` instead of silently using an
  exhausted cached route
- direct gateway regressions now also cover wrong-path replacement responses
  for `thread/resume.path` and `getConversationSummary.rolloutPath`, ensuring
  those bounded restoration paths only succeed when the restored rollout path
  matches the client-visible request path
- that real no-replacement harness now also covers
  `getConversationSummary.conversationId`: when every eligible account-backed
  worker is exhausted, the gateway fails closed and publishes
  `gateway/accountThreadHandoffFailed` instead of retrying on an exhausted
  account or changing the cached thread route
- the remote HTTP runtime now has regression coverage showing that same-project
  affinity is only an optimization: if the cached project worker becomes
  unhealthy, the next same-project `thread/start` routes to another healthy
  worker and updates the visible `projectWorkerRoutes` entry
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
- the real embedded, single-worker remote, and multi-worker remote harnesses
  now also verify path-based `thread/resume` and path-based `thread/fork`
  through the gateway's scope policy, so those rollout-path flows are
  exercised through a real `RemoteAppServerClient` session instead of only raw
  JSON-RPC regression fixtures
- the real embedded harness now also exercises the standalone
  `command/exec` baseline, covering `command/exec` plus
  `command/exec/outputDelta` through the in-process gateway transport while
  dedicated passthrough coverage continues to own the lower-level control-plane
  requests
- the real embedded harness now also exercises a plan-mode turn through
  `RemoteAppServerClient`, verifying proposed-plan `item/started` /
  `item/completed` lifecycle delivery through the in-process gateway transport
- the real single-worker remote harness now also exercises that same plan-mode
  turn path, verifying proposed-plan `item/started` / `item/completed`
  lifecycle delivery through a gateway-backed remote worker session
- the real multi-worker remote harness now also exercises that same plan-mode
  turn path on a worker-owned thread, verifying proposed-plan
  `item/started` / `item/completed` lifecycle fan-in on one shared
  `RemoteAppServerClient` session
- single-worker remote runtime now also has a real northbound v2 client harness
  using `RemoteAppServerClient`, covering bootstrap plus thread
  read/list/update workflow through a gateway-backed remote worker session
- the real embedded and single-worker remote compatibility harnesses now also
  cover connection-scoped filesystem watch setup and teardown, exercising
  `fs/watch` and `fs/unwatch` through the gateway's northbound v2 surface
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
- that same multi-worker server-request coverage now also pins the concurrent
  overlap path where two workers keep the same downstream request id pending
  at once for `item/tool/requestUserInput`,
  `item/permissions/requestApproval`, and
  `mcpServer/elicitation/request`, verifying that the gateway still assigns
  distinct northbound ids and routes both replies back to the correct worker
  sessions
- multi-worker northbound v2 transport now also suppresses exact-duplicate
  `mcpServer/startupStatus/updated` notifications across workers, extending the
  existing connection-scoped dedupe path so one shared client session does not
  surface the same MCP startup state twice
- that same exact-duplicate suppression now also covers
  `externalAgentConfig/import/completed`, so one shared client session does
  not surface duplicate import-finished notifications when a fanout import
  completes on more than one worker
- that same exact-duplicate suppression now also covers
  `account/login/completed`, so one shared client session does not surface
  duplicate onboarding-complete notifications when a fanout login flow
  completes on more than one worker
- that same exact-duplicate suppression now also covers connection-scoped
  `warning`, `configWarning`, and `deprecationNotice`, so one shared client
  session does not surface duplicate visible notices when more than one worker
  emits the same payload
- the real multi-worker connection-state notification regressions now also
  cover that `account/login/completed` dedupe path in both steady state and
  after worker reconnect, so onboarding-complete notifications stay
  single-delivery on one shared northbound session
- the real multi-worker setup-mutation regressions now also cover
  `externalAgentConfig/import/completed` dedupe in both steady state and
  after worker reconnect, so fanout import completion notifications also stay
  single-delivery on one shared northbound session
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering per-thread `turn/start` routing plus turn-lifecycle
  notification fan-in across two workers on one gateway session, verifying
  that turn control stays sticky to the owning worker and the resulting
  `thread/status/changed`, `turn/started`, `item/started`,
  `item/agentMessage/delta`, `item/completed`, and `turn/completed`
  notifications all reach the shared northbound client
- that same multi-worker turn-routing regression now also verifies fan-in for
  additional streamed turn notifications the current TUI consumes:
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and `item/fileChange/outputDelta`
- that same multi-worker turn-routing regression now also verifies fan-in for
  longer-running turn lifecycle notifications, including `item/started`,
  `item/completed`, `hook/started`, and `hook/completed`, so one shared client
  session still sees item and hook progress from the owning worker
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering thread mutation routing across two workers, verifying
  that `thread/name/set` stays sticky to the owning worker and the aggregated
  `thread/read` / `thread/list` results reflect each worker's renamed thread
  state on the shared northbound connection; that same harness now also
  observes `thread/name/updated` from both worker-owned threads, so steady-state
  rename notification fan-in is covered by real Stage B client traffic
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering lower-frequency thread-control and detached review
  routing across two workers, verifying that `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` all stay sticky to the owning worker, including follow-up
  `thread/read` of worker-owned review threads on the shared northbound
  connection
- dedicated northbound notification coverage now also pins
  `thread/name/updated`, and the real embedded, single-worker remote, and
  multi-worker remote compatibility harnesses now also observe that rename
  notification during `thread/name/set` flows through unmodified
  `RemoteAppServerClient` sessions
- dedicated northbound notification coverage now also pins the streamed item
  update notifications `item/reasoning/summaryTextDelta`,
  `item/reasoning/textDelta`, `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta`, so the visible-thread forwarding path covers
  the lower-level turn progress deltas the TUI consumes
- dedicated northbound notification coverage now also pins the thread-scoped
  `error` turn-failure notification and internal
  `rawResponseItem/completed` completion notification, so failure signals and
  raw response item replay both stay covered by the same visible-thread
  forwarding path as lifecycle and item-progress events
- the real embedded compatibility harness now also covers `configWarning`
  during startup, and the real single-worker remote compatibility harness now
  also covers `warning`, `configWarning`, `deprecationNotice`,
  `account/login/completed`, `mcpServer/oauthLogin/completed`,
  `mcpServer/startupStatus/updated`, and `windows/worldWritableWarning` in
  both steady state and after worker reconnect, so those visible notification
  paths are exercised through unmodified `RemoteAppServerClient` sessions in
  addition to dedicated northbound coverage
- the real multi-worker connection-state notification regressions now also
  cover `account/login/completed`, `warning`, `configWarning`, and
  `deprecationNotice` dedupe in both steady state and after worker reconnect,
  so onboarding-complete and other visible connection-state notifications stay
  single-delivery on one shared northbound session
- those same real multi-worker connection-state regressions now also cover
  `windows/worldWritableWarning` dedupe in steady state and after worker
  reconnect, so Windows sandbox visibility warnings stay single-delivery on
  one shared northbound session
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
- the real multi-worker external-auth onboarding harness now also observes
  deduplicated `account/login/completed` and `account/updated` notifications
  after `chatgptAuthTokens` fanout, so token-login notification fan-in is
  pinned alongside the existing follow-up aggregated `account/read`
- the real multi-worker API-key onboarding harness now also observes the
  `account/updated` notification after `account/login/start` fanout and
  verifies duplicate worker emissions stay suppressed before the follow-up
  aggregated `account/read`
- multi-worker remote runtime now also aggregates setup-time discovery requests
  that should reflect more than one worker, including
  `externalAgentConfig/detect`, threadless `app/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, and `skills/list`,
  deduplicating imported-config items, paginating the merged app inventory at
  the gateway boundary, paginating merged MCP inventory, and merging
  per-`cwd` skill results instead of exposing only the primary worker's local
  view
- multi-worker remote runtime now also aggregates bootstrap-critical
  `account/read`, deprecated `getAuthStatus`, `account/rateLimits/read`, and
  `model/list` requests, OR-ing `requiresOpenaiAuth` across workers, merging
  the per-limit rate-limit map, and unioning model inventory with
  gateway-owned pagination instead of exposing only the primary worker's
  bootstrap view
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
- dedicated plugin-merge coverage now also pins that an installed worker copy
  of a repeated plugin remains the selected summary even if later workers
  report the same plugin as merely available, keeping aggregated
  `plugin/list` installed state stable across worker ordering
- single-worker remote runtime now has a northbound v2 reconnect regression
  test, verifying that a later Codex client session can bootstrap and start a
  thread after the gateway reconnects its downstream worker
- that reconnect coverage now also asserts the worker has really returned to a
  healthy single-worker v2 compatibility profile before the later client
  session is admitted
- that reconnect coverage now also verifies the recovered v2 session can still
  complete a bidirectional `item/tool/requestUserInput` server-request round
  trip, not just bootstrap and `thread/start`
- the real single-worker remote reconnect harness now also covers lower-
  frequency thread-control and detached review flows, verifying that a later
  client session can still `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`, and
  detached `review/start` through the recovered worker
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete an `item/tool/call` dynamic-tool round trip
  with `serverRequest/resolved` ordering plus `item/started` /
  `item/completed` forwarding before `turn/completed`
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still satisfy `account/read` and `account/rateLimits/read`,
  so bootstrap account state and background rate-limit refresh survive worker
  recovery too
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still satisfy `model/list`, `externalAgentConfig/detect`,
  threadless `app/list`, `skills/list`, threadless `plugin/list`,
  threadless `config/read`, `configRequirements/read`,
  `experimentalFeature/list`, and `collaborationMode/list`, so the broader
  bootstrap/setup discovery surface used by real clients survives worker
  recovery too
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still apply low-frequency setup mutations:
  `externalAgentConfig/import`, `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `config/batchWrite`, `config/value/write`, `memory/reset`, and
  `account/logout`, so Stage A release-gate traffic covers both discovery
  refresh and post-reconnect setup writes
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete `account/sendAddCreditsNudgeEmail`, so the
  one-shot account quota nudge path survives ordinary worker recovery too
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete `feedback/upload`, so in-product feedback
  submission stays usable after ordinary worker recovery
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete the basic filesystem operation family:
  `fs/createDirectory`, `fs/writeFile`, `fs/readFile`, `fs/getMetadata`,
  `fs/readDirectory`, `fs/copy`, and `fs/remove`
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete the standalone command-execution control
  plane: `command/exec`, `command/exec/outputDelta`,
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`
- the real embedded compatibility harness now also exercises the standalone
  command-execution control plane beyond initial output streaming, covering
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate` against a live PTY-backed process through an
  unmodified `RemoteAppServerClient` session
- northbound v2 now keeps long-running `command/exec` requests pending in the
  background while the shared WebSocket session continues processing follow-up
  control requests, so live `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate` traffic can reach the active embedded app-server
  process before the final `command/exec` response is available
- that background `command/exec` path now also has a gateway-owned pending
  client-request limit, so long-running standalone commands cannot bypass the
  existing per-connection pending-request bound under overload
- dedicated northbound v2 regression coverage now verifies that a second
  `command/exec` request receives a gateway-owned rate-limit error while an
  earlier long-running `command/exec` remains pending, and that the same
  WebSocket connection stays open for the original response
- that same overload regression now also pins the v2 request metrics for both
  the rejected `command/exec` (`outcome="rate_limited"`) and the original
  pending `command/exec` (`outcome="ok"`), so dashboards can distinguish
  bounded command saturation from successful long-running command completion
- saturated pending client requests now also emit
  `gateway_v2_client_request_rejections{method,reason="pending_limit"}`, so
  background command-exec overload can be tracked separately from ordinary
  request outcomes
- that saturated pending client-request path now also has direct audit-log
  regression coverage for the per-request `rate_limited` outcome, so rejected
  long-running `command/exec` calls stay traceable at both the rejection-counter
  and request-audit boundaries
- dedicated log coverage now also pins the structured warning emitted for
  saturated pending client requests, including tenant/project scope, request
  id, method, current pending count, and limit
- `/healthz` now also exposes pending v2 client-request counts for active and
  last-completed WebSocket sessions, so operators can distinguish long-running
  background `command/exec` saturation from pending server-request prompt
  lifecycles
- `/healthz.v2Transport` now also exposes `maxPendingClientRequests` next to
  `maxPendingServerRequests`, making the independent per-connection
  saturation limits explicit for background client requests and downstream
  prompt state
- that same single-worker reconnect coverage now also re-exercises the broader
  typed server-request surface after worker recovery:
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`,
  `mcpServer/elicitation/request`, and
  `account/chatgptAuthTokens/refresh`, so the real release-gate client path
  now verifies approval, elicitation, and token-refresh payload families in
  both steady state and recovered-session mode
- that same single-worker reconnect coverage now also verifies the recovered
  v2 session can still complete the post-bootstrap plugin-management path:
  `plugin/read`, `plugin/install`, and `plugin/uninstall`, including the
  resulting installed-state refreshes through `plugin/list`
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
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, `mcpServer/startupStatus/updated`, and
  `skills/changed`
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
  `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`,
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
- that same reconnect-hardening slice now also covers connection-scoped setup
  mutations `externalAgentConfig/import`, `config/batchWrite`,
  `config/value/write`, `memory/reset`, and `account/logout`, verifying that a
  later fanout request reconnects a recovered worker before applying the
  shared mutation so worker-local setup state does not drift after a transient
  disconnect
- dedicated northbound request-routing regression coverage now also verifies
  that same reconnect-before-fanout behavior directly in `handle_client_request`,
  so this setup-mutation recovery path is pinned below the higher-level
  embedded compatibility harness too
- dedicated northbound request-routing regression coverage now also verifies
  reconnect-before-routing for primary-worker-only requests in
  `handle_client_request`, covering `configRequirements/read`, managed
  `account/login/start`, `account/login/cancel`, and `feedback/upload` so
  primary-owned setup and feedback flows do not silently drift onto a
  surviving secondary worker after reconnect
- that same primary-worker reconnect-routing coverage now also includes the
  standalone `command/exec` control plane:
  `command/exec`, `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`, so recovered primary workers reclaim standalone
  command execution before multi-worker routing falls back
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
  downstream worker lazily on later client requests, so one northbound client
  can resume round-robin `thread/start`, aggregated `thread/list` /
  `thread/loaded/list`, and sticky `thread/read` coverage for the recovered
  worker without reconnecting
- that same-session recovery path now also verifies sticky `thread/resume` and
  `thread/fork` on the recovered worker, plus a follow-up `thread/read` of the
  returned forked thread to confirm worker ownership is re-registered for the
  new thread id after reconnect
- that same-session recovery path now also verifies visible path-based
  `thread/resume` and `thread/fork` on the recovered worker, so rollout-path
  routing stays sticky after reconnect on one shared northbound session
- legacy `getConversationSummary` requests that use `rolloutPath` now also use
  the same visible-path scope key and multi-worker path-discovery route as
  path-based `thread/resume` / `thread/fork`; when the downstream summary
  returns a `conversationId` and `path`, the gateway records that worker
  ownership for later sticky thread and path routing
- the real multi-worker legacy compatibility harness now also exercises
  `getConversationSummary` by `conversationId` after worker ownership is
  registered, so both legacy summary entry points are covered through an
  unmodified `RemoteAppServerClient` session
- legacy `gitDiffToRemote` requests now also use multi-worker worker-discovery
  routing instead of falling through to the primary worker by default, so
  cwd-local git diff requests can be served by the worker that owns the
  requested checkout while still failing closed during reconnect backoff;
  dedicated northbound WebSocket regressions now pin both steady-state
  first-successful-worker routing and lazy recovered-worker routing
- deprecated `getAuthStatus` requests now also use the same multi-worker
  aggregation guard as `account/read`, preserving the primary worker's
  reported auth method/token while OR-ing `requiresOpenaiAuth` across workers
  and failing closed while required worker routes are unavailable
- that same-session recovery path now also verifies sticky
  `thread/name/set` on the recovered worker, plus a follow-up `thread/read`
  that returns the renamed state over the same northbound session
- that same-session recovery path now also verifies sticky
  `thread/memoryMode/set` on the recovered worker, so thread-scoped mutation
  requests do not fall back to the surviving worker after reconnect
- that same-session recovery path now also verifies sticky
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`, and
  `review/start` on the recovered worker, plus a follow-up `thread/read` of
  the returned review thread to confirm worker ownership is re-registered for
  newly materialized thread ids after reconnect
- that same-session recovery path now also verifies sticky
  `thread/unsubscribe`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback` on the recovered
  worker, so lower-frequency thread-control requests keep their worker
  affinity after reconnect too
- that same-session recovery path now also verifies recovered-worker
  participation in `plugin/list`, `plugin/read`, `plugin/install`, and
  `plugin/uninstall`, so fallback plugin management can re-include a dropped
  worker on one shared northbound session without forcing the client to
  reconnect
- same-session recovery coverage now spans the broader shared-session surface:
  recovered workers are re-added into connection-scoped fanout and discovery
  paths such as `fs/watch` / `fs/unwatch`, `account/read`,
  `account/rateLimits/read`, `model/list`, threadless `app/list`,
  threadless `plugin/list`, `skills/list`, `mcpServerStatus/list`,
  `thread/realtime/listVoices`, threadless and cwd-aware `config/read`,
  `experimentalFeature/list`, and `collaborationMode/list`; recovered
  primary-worker flows stay usable for `configRequirements/read`, managed
  `account/login/start` / `account/login/cancel`,
  `account/login/completed`, `feedback/upload`, and the standalone
  `command/exec` control plane; and sticky turn / realtime control plus
  translated `item/tool/requestUserInput`,
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`,
  `item/permissions/requestApproval`,
  `mcpServer/elicitation/request`, and
  `account/chatgptAuthTokens/refresh` server requests continue routing back
  through the re-added worker on one shared northbound session without
  leaking stale worker-local request ids
- that same recovery surface now also keeps notification fan-in intact after a
  worker returns, covering recovered-worker turn lifecycle notifications
  (`thread/status/changed`, `turn/started`, `hook/started`, `item/started`,
  `item/agentMessage/delta`, reasoning / command / file-change deltas,
  `hook/completed`, `item/completed`, and `turn/completed`) plus the current
  full realtime notification set, and dedicated regressions now also verify
  reconnect-on-demand for both a thread-scoped `item/tool/requestUserInput`
  round trip and a connection-scoped
  `account/chatgptAuthTokens/refresh` round trip
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
- dedicated northbound request-routing regressions now also pin those
  reconnect-backoff fail-closed warning logs on real `command/exec` and
  cwd-aware `config/read` request paths, so the operator-facing worker-route
  diagnostics stay tied to the actual degraded-session behavior rather than
  only helper-level log formatting tests
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
- primary-worker reconnect-on-demand now also has dedicated northbound
  websocket coverage for `configRequirements/read`, managed
  `account/login/start` / `account/login/cancel`, `feedback/upload`, and the
  standalone `command/exec` control plane, pinning that one shared client
  session routes those requests back onto the recovered primary worker instead
  of silently falling through to a surviving secondary worker
- connection-scoped setup mutation fanout now also has dedicated northbound
  websocket reconnect coverage for `externalAgentConfig/import`,
  `config/batchWrite`, `config/value/write`, `memory/reset`,
  `account/logout`, and external-auth `account/login/start`, pinning that one
  shared client session re-adds a recovered worker before those shared writes
  fan back out across the downstream set
- connection-scoped filesystem watch fanout now also has dedicated northbound
  websocket reconnect coverage for `fs/watch` and `fs/unwatch`, pinning that
  one shared client session re-adds a recovered worker before shared watch
  setup or teardown is applied across the downstream set
- aggregated thread discovery now also has dedicated northbound websocket
  reconnect coverage for `thread/list` and `thread/loaded/list`, pinning that
  one shared client session re-adds a recovered worker into thread discovery
  and backfills sticky worker ownership for the visible threads it contributes
- sticky thread-routed control now also has dedicated northbound websocket
  reconnect coverage for `thread/unsubscribe`, `thread/archive`,
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
- the real multi-worker same-session recovery harness now also verifies that
  `mcpServer/oauth/login` routes back to a lazily re-added worker and still
  forwards the resulting `mcpServer/oauthLogin/completed` notification on the
  shared northbound client session
- the real single-worker and multi-worker remote compatibility harnesses now
  also exercise `mcpServer/oauth/login` plus the follow-up
  `mcpServer/oauthLogin/completed` notification over the gateway's northbound
  v2 client transport
- the real single-worker reconnect harness now also verifies that later
  threadless `plugin/list`, `mcpServerStatus/list`, and
  `mcpServer/oauth/login` requests still succeed after worker recovery,
  including the follow-up `mcpServer/oauthLogin/completed` notification on
  the recovered session
- that same real single-worker reconnect harness now also re-exercises the
  lower-frequency thread-control path after worker recovery:
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- protocol and dedupe hardening now covers exact-duplicate suppression for
  connection-state notifications, `warning`, `configWarning`,
  `deprecationNotice`, `externalAgentConfig/import/completed`, and
  `skills/changed`; dropping duplicate downstream `serverRequest/resolved`
  replays after request-id translation; fail-closed handling when a client
  replies to a server request that is no longer pending, including rejection
  of any still-pending downstream prompts; and structured warning logs for both
  protocol-violation cleanup and unresolved server-request saturation
- that same protocol-violation logging now also includes any answered-but-
  unresolved server-request routes when the client replies to an unknown
  request id, and real northbound regressions now pin that ordering-sensitive
  path for both `JSONRPCResponse` and `JSONRPCError` after one prior server
  request has already been answered
- dedicated northbound regressions now also pin the scope / pending-request-id
  warning fields for both `JSONRPCResponse` and `JSONRPCError` when a client
  replies to an unknown server-request id before the gateway closes the socket,
  including pending downstream server-request ids, affected thread ids, and
  pending worker ids for northbound / downstream prompt correlation
- malformed post-handshake client payloads now also have dedicated northbound
  regression coverage for the same teardown warning fields, pinning scope and
  pending-request ids before the gateway closes the socket
- that same malformed-payload teardown path now also pins any answered-but-
  unresolved server-request routes after the client has already replied but
  downstream `serverRequest/resolved` has not arrived yet
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
  session; the warning log now also includes the affected worker websocket URL
- slow-client send timeouts now also emit a structured warning log with scope,
  terminal timeout detail, any still-pending gateway/downstream server-request
  ids, and affected thread and worker ids before pending downstream prompts are
  rejected, so `client_send_timed_out` outcomes can be tied back to the
  stranded session state directly from logs
- that same slow-client warning now also includes pending background
  client-request ids and methods, so a wedged `command/exec` response can be
  correlated with the gateway-owned `client_send_timed_out` outcome without
  relying only on aggregate `/healthz` counts; a dedicated northbound
  WebSocket regression now pins that pending `command/exec` diagnostic path
- that same pending background request diagnostic now also includes downstream
  worker ids and WebSocket URLs, so multi-worker `command/exec` stalls can be
  tied directly to the owning worker session from slow-client and teardown logs
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
- that answered-but-unresolved slow-client regression now also pins the
  dedicated `gateway_v2_client_send_timeouts` counter on the same real
  northbound WebSocket path, so timeout metrics cover both pending and
  already-answered prompt lifecycle variants
- that same connection-end teardown path now also has a real northbound
  regression for client disconnects after one server request has already been
  answered but before downstream `serverRequest/resolved` arrives, pinning the
  answered-but-unresolved route ids that should still appear in the teardown
  warning log
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  request-id translation now also emit structured warning logs with scope,
  worker id, worker websocket URL, the replayed downstream request id, and any
  still-buffered translated gateway ids, downstream ids, and worker ids
- that duplicate-replay path now also has direct northbound regression
  coverage, pinning the warning log fields emitted when a multi-worker
  downstream session replays `serverRequest/resolved` after the translated
  route has already been drained
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
- history-based `thread/resume` now also passes through gateway v2 scope
  enforcement in embedded, single-worker remote, and multi-worker remote
  mode, treating that app-server flow as connection-scoped instead of
  rejecting it up front just because the placeholder request `threadId` is not
  yet visible
- legacy v2 `execCommandApproval` and `applyPatchApproval` server requests now
  also participate in gateway thread visibility checks via their
  `conversationId`, so hidden-thread legacy prompts are rejected instead of
  bypassing scope enforcement; dedicated northbound regressions now also pin
  the legacy approval round-trip transport for both methods
- legacy `getConversationSummary` requests now also participate in gateway
  path visibility checks via `rolloutPath`, so summary reads for hidden rollout
  files cannot bypass the same scope policy used by path-based re-entry flows
- legacy `gitDiffToRemote` requests now also avoid primary-worker-only routing
  in multi-worker mode; the gateway probes available workers for the requested
  `cwd` and requires the full configured worker set to be present before
  selecting a successful downstream diff response, with real northbound
  WebSocket coverage for both the steady-state and recovered-worker paths
- deprecated `getAuthStatus` requests now also behave like a legacy
  `account/read` compatibility path in multi-worker mode: primary-worker
  auth details remain authoritative, `requiresOpenaiAuth` reflects all
  workers, and reconnect-backoff sessions fail closed before returning partial
  auth state
- fail-closed handling for a downstream session that reuses a still-pending
  server-request id now also emits a structured warning log with scope, worker
  id, worker websocket URL, the colliding request id/method, and the gateway
  request ids already pending on the connection; that collision log now also
  includes pending downstream request ids, affected thread ids, and pending
  worker ids
- gateway-owned teardown of still-pending downstream server requests now also
  emits structured warning logs with connection outcome, terminal detail,
  pending gateway request ids, per-scope counts, worker ids, and worker
  websocket URLs, so disconnect-driven cleanup is visible without
  reconstructing the session from downstream errors alone
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
  `thread/resume` and `thread/fork`, including sticky routing plus route
  backfill for follow-up `thread/read`
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
- dedicated northbound log regression coverage now also pins the structured
  multi-worker `thread/list` dedupe and visible-thread route recovery logs, so
  operator diagnostics for snapshot selection and downstream `thread/read`
  ownership probes stay stable as Phase 6 transport hardening continues;
  dedupe and route-recovery success/miss logs now also include worker
  websocket URLs so operators can identify the affected downstream sessions
  directly
- multi-worker `thread/list` dedupe and visible-thread route recovery now also
  emit dedicated v2 metrics, so snapshot-selection churn and lazy ownership
  probe misses are observable from dashboards instead of only structured logs

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
  the current per-worker health state, last observed worker error, and the
  latest worker health / error timestamps so operators can distinguish stale
  historical errors from a fresh outage
- `/healthz` remote-worker entries now also expose whether the gateway is
  actively reconnecting that worker, how many reconnect attempts have already
  failed in the current loop, the next scheduled reconnect time, and the
  remaining reconnect backoff seconds, so transient worker loss is visible
  without inferring from logs alone
- `/healthz` now also reports execution mode so clients can distinguish local
  embedded execution, embedded `exec-server` delegation, and remote
  worker-managed execution
- background reconnect attempts for remote websocket workers after disconnect,
  with automatic return to the worker pool once the app-server session is
  re-established
- SSE `gateway/reconnecting` and `gateway/reconnected` events so northbound
  clients can observe both entry into the reconnect loop and eventual worker
  recovery
- dedicated remote-runtime SSE regression coverage now pins those
  `gateway/reconnecting` and `gateway/reconnected` events end to end, so the
  operator-facing reconnect signal stays validated alongside `/healthz` in
  both single-worker and multi-worker remote topologies

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

- core transport is in place across the supported topologies:
  northbound WebSocket JSON-RPC at `/`, embedded and single-remote-worker
  passthrough for `initialize`, typed requests, notifications, responses, and
  server requests, plus the current multi-worker Stage B transport with one
  downstream app-server session per worker and gateway-owned routing /
  aggregation where needed
- handshake and boundary policy are gateway-owned:
  initialize response shaping, downstream client identity and capability
  propagation, auth and Origin guards, explicit fail-closed behavior when v2 is
  unavailable for the current runtime topology, and the same scope, admission,
  audit, and metrics policies that already apply to HTTP traffic
- northbound v2 admission coverage now also pins the per-scope
  `turn/start` quota path over WebSocket, verifying that the first turn can
  pass through to the downstream app-server while the next turn in the same
  quota window receives the gateway-owned rate-limit JSON-RPC error and emits
  the expected v2 request metric and audit log with `outcome=rate_limited`
- that same v2 admission coverage now also asserts the general per-scope
  request-rate-limit metric path when scope enforcement rejects the first
  request and the next request in the same quota window is rate-limited, so
  policy and quota outcomes stay distinct in `gateway_v2_requests`
- post-initialize protocol-violation coverage now also asserts request-level
  metrics for repeated `initialize` requests, so operators can correlate the
  `gateway_v2_protocol_violations` counter with the corresponding
  `gateway_v2_requests{method="initialize", outcome="invalid_request"}` path
- that repeated-`initialize` coverage now also pins the matching gateway v2
  audit log with tenant/project scope and `outcome="invalid_request"`, so
  post-handshake startup protocol mistakes are visible through metrics,
  protocol-violation counters, and audit logs together
- pre-initialize ordering-violation coverage now also asserts request-level
  metrics for requests sent before `initialize`, so operators can correlate
  `gateway_v2_protocol_violations{phase="pre_initialize",
  reason="initialize_order"}` with the offending method's
  `gateway_v2_requests{outcome="invalid_request"}` path
- that pre-initialize ordering-violation coverage now also pins the matching
  gateway v2 audit log with tenant/project scope and
  `outcome="invalid_request"`, so startup ordering mistakes are visible
  through metrics, protocol-violation counters, and audit logs together
- pre-initialize malformed-payload coverage now also asserts the paired
  connection-level metrics for text JSON-RPC parse failures and invalid UTF-8
  binary frames, so malformed startup traffic is tied to the terminal
  `invalid_client_payload` connection outcome before any downstream app-server
  session is opened
- pre-initialize malformed text-payload coverage now also pins the matching
  gateway v2 connection audit record with tenant/project scope, terminal
  detail, and `outcome="invalid_client_payload"`, so parse failures before
  `initialize` are visible through both metrics and audit logs
- pre-initialize invalid UTF-8 binary-frame coverage now also pins the matching
  gateway v2 connection audit record with tenant/project scope, terminal
  detail, and `outcome="invalid_client_payload"`, so binary startup payload
  failures have the same audit visibility as malformed text payloads
- post-initialize malformed-payload coverage now also asserts the paired
  connection-level metrics for text JSON-RPC parse failures and invalid UTF-8
  binary frames, so those protocol violations stay tied to the terminal
  `invalid_client_payload` connection outcome
- post-initialize malformed text-payload coverage now also pins the matching
  gateway v2 connection audit record with tenant/project scope, terminal
  detail, and `outcome="invalid_client_payload"`, so JSON-RPC parse failures
  remain audit-visible after initialize completes
- post-initialize invalid UTF-8 binary-frame coverage now also pins the
  matching gateway v2 connection audit record with tenant/project scope,
  terminal detail, and `outcome="invalid_client_payload"`, so malformed binary
  frames stay audit-visible after the downstream app-server session is already
  established
- unknown server-request reply coverage now also asserts the paired connection
  metrics for unexpected northbound `Response` and `Error` frames, so
  `unexpected_client_server_request_response` lifecycle events stay tied to the
  terminal `protocol_violation` connection outcome
- the unknown server-request `Response` warning-log regression now also pins
  the response kind plus pending downstream request and worker ids, matching
  the existing `Error`-frame coverage so both unexpected reply forms keep
  enough route context for production diagnosis
- downstream protocol-violation coverage now also asserts the paired
  connection-level metrics for malformed JSON-RPC frames, non-text frames, and
  unexpected downstream response/error IDs after initialization, so
  `gateway_v2_protocol_violations{phase="downstream"}` stays correlated with
  the terminal `downstream_protocol_violation` connection outcome
- the malformed downstream JSON-RPC regression now also pins the structured
  warning log on the real northbound WebSocket path, including tenant/project
  scope, worker route context, violation reason, downstream error text, and
  active worker count
- initialize-time downstream protocol-violation coverage now also asserts the
  paired `gateway_v2_requests{method="initialize",
  outcome="downstream_protocol_violation"}` metric for malformed initialize
  responses, wrong initialize response/error IDs, and non-text initialize
  frames, so startup compatibility failures are visible at the request,
  protocol, connection, and health layers together
- malformed downstream initialize-response coverage now also pins the matching
  gateway v2 audit record with tenant/project scope and
  `outcome="downstream_protocol_violation"`, so startup compatibility failures
  are visible through metrics, protocol-violation counters, connection health,
  and audit logs together
- downstream connection-failure coverage now also asserts the paired
  `gateway_v2_requests{method="initialize",
  outcome="downstream_connect_error"}` metric when the gateway cannot open the
  app-server session after a valid northbound initialize request, so ordinary
  worker reachability failures stay distinguishable from downstream protocol
  violations at the request-metrics layer
- that downstream connection-failure coverage now also pins the gateway v2
  audit record with tenant/project scope and
  `outcome="downstream_connect_error"`, so worker reachability failures are
  visible through both metrics and audit logs
- initialize timeout coverage now also asserts the paired
  `gateway_v2_requests{method="initialize", outcome="timed_out"}` metric
  alongside the terminal `initialize_timed_out` connection metric, so startup
  stalls are visible through both request-level and connection-level telemetry
- that initialize timeout coverage now also pins the gateway v2 audit record
  with tenant/project scope and `outcome="timed_out"`, so startup stalls are
  visible through both metrics and audit logs
- invalid initialize-params coverage now also asserts the paired
  `gateway_v2_requests{method="initialize", outcome="invalid_request"}`
  metric, so malformed but parseable startup requests are visible at the same
  request-metrics boundary as ordering and repeated-initialize violations
- that invalid initialize-params coverage now also pins the gateway v2 audit
  record with tenant/project scope and `outcome="invalid_request"`, so
  malformed startup requests stay visible through both metrics and audit logs
- scope-policy coverage now also asserts the paired
  `gateway_v2_requests{method="thread/resume",
  outcome="invalid_params"}` metric for unknown path-based rollout re-entry,
  so gateway-owned v2 path checks remain visible as request outcomes instead
  of only JSON-RPC error payloads
- that same scope-policy metric coverage now also covers unknown path-based
  `thread/fork`, so both rollout-path entrypoints report
  `outcome="invalid_params"` before any downstream routing attempt
- path-based `thread/resume` / `thread/fork` scope-policy tests now also pin
  that placeholder `threadId` values are ignored for route affinity while the
  rollout `path` remains the scope-check key
- rollout visibility is operator-facing instead of test-only:
  startup and `/healthz` distinguish embedded, single-worker remote, and the
  current partial multi-worker profile; `/healthz` exposes
  `initializeTimeoutSeconds`, `clientSendTimeoutSeconds`,
  `reconnectRetryBackoffSeconds`, `maxPendingServerRequests`, and
  `maxPendingClientRequests`; remote-worker entries expose `lastStateChangeAt`,
  `lastErrorAt`, `reconnecting`,
  `reconnectAttemptCount`, `nextReconnectAt`, and
  `reconnectBackoffRemainingSeconds`; `/healthz` now also exposes
  `v2Connections.activeConnectionCount`,
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
  the latest completed v2 connection outcome/detail/timestamp; health, metrics,
  audit logs, and structured logs now all carry the last v2 connection
  duration needed to diagnose slow-client and short-lived failure paths
- `/healthz` now also rolls up pending and answered-but-unresolved
  server-request counts across currently active v2 connections, so operators
  can spot live prompt lifecycle buildup before waiting for a connection to
  close and become the latest completed snapshot
- that operator-facing `v2Connections` health snapshot now also has direct
  embedded, single-worker remote, and multi-worker remote regression coverage,
  pinning active, peak, and total connection counts plus the last-started and
  last-outcome fields across live and settled client sessions; single-worker
  and multi-worker remote `/healthz` coverage now also pin the gateway-owned
  `client_send_timed_out` outcome/detail, duration, plus the last completed
  connection's pending and answered-but-unresolved server-request counts after
  a slow-client teardown via a real northbound WebSocket session
- remote-worker reconnect-backoff visibility now also has direct runtime and
  HTTP health regression coverage, pinning `reconnectBackoffRemainingSeconds`
  alongside `nextReconnectAt` for reconnecting workers and `null` for healthy
  workers
- v2 connection accounting now also records terminal outcome and decrements the
  active connection count even when an early handshake-time close frame or
  JSON-RPC error response cannot be delivered, so `/healthz` does not retain
  stale `activeConnectionCount` under setup-time send failures
- those operator-facing v2 connection and reconnect logs now also have direct
  regression coverage, pinning the fields and outcome mapping that rollout
  debugging relies on
- v2 connection completion and audit logs now also include terminal detail plus
  pending client-request, pending server-request, answered-but-unresolved
  server-request, and combined server-request backlog counts, so ordinary
  connection outcome logs carry the same background-command and stranded-prompt
  summary that `/healthz` and metrics expose
- those v2 connection completion and audit logs now also include pending
  client-request worker and method count summaries, so terminal logs can be
  correlated directly with the matching `/healthz.v2Connections` fields when
  background `command/exec` or other long-running client requests remain
  in-flight during teardown
- v2 connection metrics now also emit
  `gateway_v2_connection_pending_client_requests{outcome}`, aligning
  connection-level telemetry with `/healthz` and structured logs so background
  command saturation is visible after connection teardown without relying only
  on the latest health snapshot
- v2 connection metrics now also include
  `gateway_v2_connection_pending_client_requests_by_method`, tagged by
  connection outcome and client request method, so exported dashboards can
  split terminal background client-request buildup by `command/exec` or other
  long-running request families instead of relying only on the aggregate
  pending-client count
- v2 connection metrics now also include
  `gateway_v2_connection_pending_client_requests_by_worker`, tagged by
  connection outcome and downstream worker id, so terminal background
  client-request buildup can be correlated with the same worker route exposed
  through `/healthz.v2Connections` and structured completion logs
- `/healthz.v2Connections` now also groups active-session and last-completed
  pending client requests by downstream worker and client method via
  `activeConnectionPendingClientRequestWorkerCounts`,
  `activeConnectionPendingClientRequestMethodCounts`,
  `lastConnectionPendingClientRequestWorkerCounts`, and
  `lastConnectionPendingClientRequestMethodCounts`, so long-running
  `command/exec` or other background request buildup can be tied directly to
  the app-server session and method family causing pressure
- `/healthz.v2Connections` now also exposes
  `activeConnectionMaxPendingClientRequestCount`, so operators can distinguish
  broad background client-request pressure across many active sessions from a
  single stalled session accumulating most of the pending `command/exec` or
  other long-running client requests
- `/healthz.v2Connections` now also exposes
  `activeConnectionPeakPendingClientRequestCount` and
  `lastConnectionMaxPendingClientRequestCount`, so short-lived
  background-client buildup remains visible even if the queue drains before
  the active snapshot or the completed connection's terminal pending count
- `/healthz.v2Connections` now also exposes
  `activeConnectionPendingClientRequestStartedAt` and
  `lastConnectionPendingClientRequestStartedAt`, so operators can tell whether
  background `command/exec` or other long-running client-request buildup is
  fresh or has persisted across a meaningful part of the connection lifetime
- the real northbound WebSocket slow-client `command/exec` regression now also
  captures the active pending-client health snapshot while the downstream
  request is still in flight, pinning the aggregate count, max-per-connection
  count, buildup timestamp, worker split, and method split before the terminal
  timeout path records the completed connection snapshot
- last-completed v2 pending-client timestamps now also get a conservative
  completion-time fallback when the terminal pending count is first observed
  during teardown, so `/healthz.v2Connections` does not report nonzero
  completed background client-request buildup with a missing
  `lastConnectionPendingClientRequestStartedAt`
- v2 connection teardown now also emits a structured warning when pending
  background client requests are aborted, including tenant/project scope,
  terminal outcome/detail, request ids, methods, and downstream worker ids /
  WebSocket URLs, so partially completed `command/exec` lifecycles are visible
  without reconstructing them only from aggregate terminal counts
- if downstream app-server shutdown also fails after a gateway v2 connection
  error, teardown now emits a structured warning with tenant/project scope,
  terminal outcome/detail, pending client-request count, pending client
  request ids / methods / worker routes, pending server-request count,
  answered-but-unresolved server-request count, backlog worker and method
  count summaries, and the shutdown error detail, and increments
  `gateway_v2_downstream_shutdown_failures{outcome}` so cleanup failures are
  visible as a direct rollout signal
- completed background client requests are now settled before their final
  northbound JSON-RPC response is sent, so a slow-client failure while
  delivering a finished `command/exec` response does not leave that request in
  active pending counts or aborted-request teardown diagnostics; direct
  regression coverage now pins the pending-count and active-route cleanup
  invariant
- if that final background `command/exec` response cannot be delivered because
  the northbound client times out or disconnects, the gateway now records the
  request with the connection-owned failure outcome instead of losing the
  per-method request metric after active-route cleanup
- the connection teardown path now also drains already completed background
  responses before logging aborted pending client requests, so a `command/exec`
  that finished just as the northbound session ended is not misreported as
  still active
- aborted background client requests now also emit
  `gateway_v2_requests{method,outcome}` using the connection terminal outcome,
  so interrupted long-running `command/exec` calls remain visible in
  per-method request metrics rather than only connection-level telemetry
- aborted background `command/exec` requests now also have direct audit-log
  coverage for the per-request terminal outcome, so client disconnect,
  protocol-violation, and slow-client teardown paths stay visible at the same
  request boundary as successful and rejected command requests
- northbound v2 now fails closed when a client reuses any still-pending
  background client-request id, even for a different follow-up method,
  preventing the active pending-client route from being overwritten; dedicated
  coverage pins the protocol close, violation metric, and structured log
  fields for the duplicate id plus original worker route
- that duplicate pending client-request path now also records the offending
  follow-up request as `gateway_v2_requests{method,outcome="protocol_violation"}`,
  and emits the matching request audit log, so dashboards and audit trails can
  correlate the protocol close with the method that reused the in-flight id
- primary-worker fail-closed routing for background `command/exec` now also
  records the rejected request as `gateway_v2_requests{method="command/exec",
  outcome="internal_error"}` and emits the matching request audit log before
  the connection ends, so standalone command route failures are visible at the
  per-method boundary instead of only as connection failures
- primary-worker route failures for ordinary request/response methods now also
  have real northbound WebSocket coverage for the matching
  `gateway_v2_requests{method,outcome="internal_error"}` metric and request
  audit log, so degraded primary-worker setup reads are visible at the same
  request boundary as background command execution
- v2 server-request saturation and scope-policy rejection now also emit a
  dedicated rejection counter tagged by server-request method and reason
  (`pending_limit` or `hidden_thread`), so overload and policy dashboards can
  distinguish bounded prompt-state pressure or hidden-thread prompt drops from
  ordinary connection outcomes; the matching rejection logs now also include
  the affected worker websocket URL, and saturated-connection logs include the
  pending gateway/downstream server-request ids plus affected thread and worker
  ids that caused the bounded prompt-state pressure
- normal client answers to forwarded v2 server requests now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=client_server_request_answered` and `method=response` or
  `method=error`, so the answered-but-unresolved lifecycle stage is visible
  before the downstream `serverRequest/resolved` notification arrives
- delivery of those client answers back to the owning downstream worker now also
  emits `event=client_server_request_delivered` or
  `event=client_server_request_delivery_failed`, so partially completed prompt
  lifecycles distinguish a client answer that only reached the gateway from one
  that was successfully handed back to app-server; direct regression coverage
  now pins the delivery-failure branch when the owning worker route is no longer
  available
- that delivery-failure branch now also emits a structured warning log with
  tenant/project scope, response kind, worker id, worker websocket URL,
  gateway request id, downstream request id, thread id, and error detail, so
  prompts that are answered northbound but cannot be handed back downstream
  have the same operator-visible route context as other partially completed
  server-request lifecycles
- that same answered-but-not-delivered branch now also increments
  `gateway_v2_server_request_answer_delivery_failures{response_kind}`, so
  prompt / elicitation replies that reached the gateway but failed on the
  downstream handoff have a direct alerting counter alongside the broader
  lifecycle event stream
- pending server-request routes now snapshot the worker websocket URL at
  forwarding time, so later cleanup and answered-but-undeliverable warning logs
  preserve the original downstream route even if the worker handle is no longer
  available when the gateway emits diagnostics
- pending server-request log summaries now also emit
  `pending_worker_websocket_urls` alongside `pending_worker_ids` for unexpected
  client replies, saturated prompt rejection, downstream backpressure,
  slow-client timeout, downstream protocol violation, and duplicate downstream
  request-id diagnostics, so operator logs identify the original worker routes
  without reconstructing them from route state
- answered-but-unresolved server-request log summaries now also emit
  `answered_but_unresolved_worker_websocket_urls`, including duplicate
  `serverRequest/resolved` replay diagnostics, so partially completed prompt
  lifecycles retain the original downstream route after the pending route has
  already been drained
- downstream v2 server requests that pass gateway scope and saturation checks
  now also emit `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_forwarded` and the forwarded request method,
  so operators can distinguish prompts that reached the northbound client from
  prompts rejected at the gateway boundary
- downstream v2 server requests rejected by gateway saturation or scope policy
  now also emit `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_rejected_pending_limit` or
  `event=downstream_server_request_rejected_hidden_thread`, so rejected prompts
  remain visible in the same lifecycle counter as forwarded, answered, and
  resolved prompts
- delivery of those gateway-owned downstream rejections now also emits
  `event=downstream_server_request_rejection_delivered` or
  `event=downstream_server_request_rejection_delivery_failed`, with warning-log
  route context on failure, so a prompt rejected at the gateway boundary is not
  mistaken for one that the owning worker definitely observed
- normal downstream `serverRequest/resolved` notifications now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=downstream_server_request_resolved`, so successful prompt lifecycles
  have the same operator-visible stage marker as duplicate replay and cleanup
  paths
- the real northbound WebSocket server-request error regression now also
  carries the rejected prompt through the follow-up `serverRequest/resolved`
  notification, so error replies are covered through the same forwarded,
  answered, delivered, and resolved lifecycle telemetry as successful replies
- the real northbound WebSocket pending-limit regression now also pins that
  saturated server-request warning log end to end, including the rejected
  request id, tenant/project scope, configured pending limit, affected worker
  URL, and still-pending gateway/downstream request and thread ids, while
  verifying the connection can continue serving follow-up v2 requests after the
  bounded prompt rejection
- the hidden-thread server-request policy regression now also verifies the
  rejection metric and structured warning log in the same real northbound
  WebSocket flow, pinning tenant/project scope, affected worker URL, rejected
  request id, method, and hidden thread id at the transport boundary instead
  of only through helper-level log formatting coverage
- multi-worker v2 reconnect activity now also emits a
  `gateway_v2_worker_reconnects` counter tagged by worker id and outcome
  (`attempt`, `success`, `connect_failure`, `replay_failure`, or
  `backoff_suppressed`), so reconnect churn can be tracked from metrics instead
  of only logs and `/healthz`; retry-backoff suppression now also emits a
  structured warning log with worker id, worker websocket URL, replay-state
  context, configured retry backoff seconds, and remaining backoff seconds so
  skipped reconnect attempts are visible from logs too
- those reconnect outcome counters now also have direct reconnect-path
  regression coverage for successful reconnects, connection failures, replay
  failures, and retry-backoff suppression, so the metric contract is pinned at
  the transport boundary rather than only at the observability helper
- multi-worker v2 fail-closed requests now also emit a
  `gateway_v2_fail_closed_requests` counter tagged by JSON-RPC method and
  whether any required worker route is currently held in reconnect backoff, so
  degraded-session protection can be measured separately from generic
  `internal_error` request outcomes; the matching structured logs include
  available, unavailable, and reconnect-backoff worker websocket URLs plus
  reconnect-backoff remaining seconds, including paired worker id / URL /
  remaining-second route diagnostics for each backoff worker
- multi-worker v2 upstream request failures that occur while worker routes are
  unavailable now also emit a `gateway_v2_upstream_request_failures` counter
  tagged by JSON-RPC method and active reconnect backoff, so ordinary
  downstream/request errors stay distinguishable from gateway-owned
  fail-closed policy decisions
- that upstream-failure metric is now recorded once at the real
  `handle_client_request` routing boundary, so unavailable-worker
  observability is exercised without double-counting the outer WebSocket
  error handling path
- primary-worker-only multi-worker requests now also have method-family
  reconnect-backoff regression coverage for config requirements, managed
  login, login cancellation, add-credits nudge email, feedback upload,
  standalone command control, basic filesystem operations, streaming fuzzy
  file-search sessions, and Windows sandbox setup, so degraded-session
  fail-closed behavior is pinned across the whole primary route set instead of
  only a representative command request
- suppressed multi-worker connection notifications now also emit a
  `gateway_v2_suppressed_notifications` counter tagged by notification method
  and suppression reason (`duplicate`, `pending_refresh`, or
  `hidden_thread`), so rollout dashboards can distinguish healthy
  gateway-owned dedupe and scope filtering from missing worker events or
  client-side notification loss; hidden-thread notification suppression logs
  now also include the affected worker websocket URL, and exact-duplicate
  connection-state suppression logs include both the dropped worker route and
  the original forwarded worker route
- forwarded downstream v2 notifications now also emit
  `gateway_v2_forwarded_notifications{method}`, with direct visible-thread
  forwarding coverage, multi-worker fan-in count coverage, plus opt-out and
  hidden-thread negative coverage, so notification fan-in dashboards can
  compare delivered traffic against duplicate, opt-out, pending-refresh, and
  hidden-thread suppression
- downstream v2 notifications that fail during northbound delivery now emit
  `gateway_v2_notification_send_failures{method,outcome}`, so slow-client
  notification stalls can be attributed to the notification family that hit the
  terminal send failure instead of only the broad connection outcome
- those notification send failures now also emit structured warning logs with
  tenant/project scope, worker id, worker websocket URL, method, outcome, and
  error detail, so failed notification fan-in is diagnosable without relying
  only on aggregate metrics
- downstream v2 server requests that pass gateway policy but fail during
  northbound delivery now emit
  `event=downstream_server_request_forward_delivery_failed` on the
  server-request lifecycle counter, with structured warning logs carrying
  tenant/project scope, worker route, method, outcome, and send error detail
- those downstream server-request forward delivery failures now also increment
  `gateway_v2_server_request_forward_send_failures{method,outcome}`, so
  dashboards can alert directly on prompt / elicitation delivery failures
  without filtering the broader lifecycle event stream
- client answers to forwarded server requests that fail during downstream
  delivery now also increment
  `gateway_v2_server_request_answer_delivery_failures{response_kind}`, so
  answered-but-not-delivered prompt / elicitation replies are directly
  measurable without filtering the broader lifecycle event stream
- gateway-owned server-request rejections that fail during downstream delivery
  now also increment
  `gateway_v2_server_request_rejection_delivery_failures{method}`, so prompt /
  elicitation rejections that never reach the owning worker have a direct
  alerting counter alongside the broader lifecycle event stream
- client-side cleanup rejections for still-pending server requests now also
  increment
  `gateway_v2_server_request_rejection_delivery_failures{method}` with the
  original server-request method when the downstream handoff fails or the
  worker route is already unavailable, so teardown-time prompt cleanup failures
  show up in the same direct alerting channel as ordinary gateway-owned
  rejection delivery failures and remain distinguishable by prompt family
- gateway v2 client request responses that fail during northbound delivery now
  emit structured warning logs with tenant/project scope, JSON-RPC request id,
  method, outcome, and send error detail, so response-path slow-client
  failures can be diagnosed at request granularity alongside the existing
  request outcome metrics
- failed northbound delivery of gateway v2 client request responses now also
  increments `gateway_v2_client_response_send_failures{method,outcome}` for
  initialize handshake responses, ordinary request responses, and background
  `command/exec` completions, so response-path slow-client failures are
  measurable without filtering the broader per-request outcome counter
- dedicated northbound regression coverage now also pins ordinary success and
  error client-response send-failure paths over a real WebSocket, so this
  metric is exercised at the compatibility boundary instead of only by direct
  helper tests
- failed delivery of synthesized worker-cleanup `serverRequest/resolved`
  notifications now also increments
  `gateway_v2_notification_send_failures{method="serverRequest/resolved",outcome}`
  and records
  `gateway_v2_server_request_lifecycle_events{event="worker_cleanup_resolution_send_failed",method}`
  with the original server-request method; the structured cleanup warning also
  includes tenant/project scope and the prompt method, so prompt-dismissal
  failures during worker loss are visible through notification telemetry and
  remain distinguishable by prompt family in lifecycle dashboards
- failed delivery of gateway-owned v2 WebSocket close frames now increments
  `gateway_v2_close_frame_send_failures{code,outcome}`, so operators can
  distinguish slow-client failures that prevent a terminal policy/protocol
  close frame from reaching the client from ordinary response or notification
  send failures; those failures now also emit structured warning logs with
  tenant/project scope, the close code, protocol-safe reason, outcome, and send
  error detail
- dedicated northbound regression coverage now also pins gateway-owned close
  frame send failures over a real WebSocket, including the
  `gateway_v2_close_frame_send_failures{code,outcome}` counter and structured
  warning log fields, so terminal close delivery failures are exercised at the
  compatibility boundary instead of only through observability helper tests
- the real northbound WebSocket exact-duplicate `warning` suppression
  regression now also pins that structured suppression log end to end,
  including tenant/project scope, dropped and original worker routes, worker
  websocket URLs, notification method, and payload context alongside the
  existing `duplicate` suppression metric
- the real northbound WebSocket hidden-thread notification regression now also
  pins that structured suppression log end to end, including tenant/project
  scope, affected worker URL, notification method, hidden thread id, and
  payload context alongside the existing `hidden_thread` suppression metric
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `windows/worldWritableWarning` and
  `windowsSandbox/setupCompleted`, so one shared northbound session does not
  surface duplicate platform setup notices when more than one worker emits the
  same payload
- duplicate `externalAgentConfig/import/completed` suppression is now also
  pinned through the `gateway_v2_suppressed_notifications` metric, so fanout
  import completion anomalies are visible through the same operator-facing
  counter as other duplicate connection notifications
- degraded multi-worker thread discovery regression coverage now also asserts
  one gateway-owned warning log and metric emission per `thread/list` or
  `thread/loaded/list` request, so reconnect-backoff dashboards do not get
  inflated by duplicate operator signals from a single degraded refresh
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  gateway request-id translation now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_resolved_replay`, so worker replay anomalies are measurable
  from metrics instead of only visible in structured logs
- that duplicate-replay path now also has real northbound WebSocket coverage
  for the lifecycle metrics, pinning both the initial
  `downstream_server_request_resolved` event and the later
  `duplicate_resolved_replay` event on the same translated server-request
  route
- downstream server requests that reuse a still-pending gateway request id now
  also emit `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` before the gateway fails the session
  closed, so request-id collision anomalies are visible in metrics alongside
  the close outcome and structured warning log; that warning log includes the
  pending gateway/downstream request ids, affected thread ids, and pending
  worker ids that were on the connection when the collision occurred
- worker-loss cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` counters for synthetic
  thread-scoped `serverRequest/resolved` notifications and stranded
  connection-scoped server requests, tagged by the original server-request
  method, so disconnect-driven prompt cleanup is measurable in addition to the
  existing structured cleanup logs and remains distinguishable by prompt family
- those worker-loss cleanup counters are recorded as soon as the gateway drains
  the affected routes, before any synthetic `serverRequest/resolved`
  notifications are sent, so slow-client send timeouts do not hide the cleanup
  event from metrics
- client-side connection cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` counters for rejected
  thread-scoped pending prompts, rejected connection-scoped pending prompts,
  and answered-but-unresolved prompts left behind by client disconnect,
  protocol-violation, or slow-send teardown paths; the matching cleanup log now
  includes pending downstream server-request ids and affected thread ids in
  addition to gateway request ids, worker ids, and worker websocket URLs, so
  partially completed prompt lifecycles can be correlated across the northbound
  and downstream sessions; answered-but-unresolved route logs also include
  affected thread ids
- client-side cleanup now also records lifecycle events when pending
  server-request rejection delivery fails or must be skipped because the
  owning downstream worker route is already unavailable, so prompt cleanup
  loss is visible instead of disappearing behind the terminal connection
  outcome; the matching warning logs include the affected worker websocket URL
  for both failed and skipped rejection delivery
- client-side cleanup now also records a lifecycle event when pending
  server-request rejection delivery succeeds, so cleanup dashboards can
  distinguish prompts the worker observed from prompts whose cleanup delivery
  failed or was skipped
- that successful client-side cleanup rejection event now also has real
  northbound WebSocket disconnect coverage, so the ordinary client-close path
  is pinned alongside the slow-client timeout and direct helper coverage
- worker-loss cleanup now also records a lifecycle event and structured
  warning when delivery of synthesized `serverRequest/resolved` notifications
  fails, so thread-scoped prompt cleanup failures are visible on the same
  observability path as rejected pending prompts; that lifecycle event is
  tagged by the original server-request method, and the send-failure warning
  now also includes the affected worker websocket URL and method
- successful delivery of those synthesized `serverRequest/resolved`
  notifications now also records a lifecycle event on the real northbound
  WebSocket path, tagged by the original server-request method, so worker-loss
  cleanup dashboards can distinguish prompts that were resolved and delivered
  from prompts whose synthetic resolution failed during client send
- worker-loss cleanup warning logs now also include the affected thread ids for
  thread-scoped prompts that the gateway resolves synthetically after a
  downstream worker disappears, so operators can correlate those cleanup events
  back to visible thread state without reconstructing the route table
- those cleanup-delivery failures now preserve the connection's pending and
  answered-but-unresolved server-request counts in the terminal health,
  metrics, and audit records instead of falling through to a zero-count outer
  connection error fallback
- client replies for unknown server-request ids now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response`, so protocol-violation
  prompt replies are visible in metrics before the gateway closes the
  northbound v2 connection; the matching warning logs include pending
  gateway/downstream server-request ids plus affected pending and
  answered-but-unresolved thread ids plus pending worker ids
- malformed or out-of-order v2 client traffic now also emits
  `gateway_v2_protocol_violations` with `phase` and `reason` tags, covering
  pre-initialize ordering errors, invalid JSON-RPC payloads, invalid UTF-8
  binary frames, and repeated `initialize` requests after the handshake; direct
  northbound websocket regressions pin those metric tags at the transport
  boundary
- downstream app-server event stream lag/backpressure now also emits
  `gateway_v2_downstream_backpressure_events` with the affected worker id
  before the gateway sends the close frame, so slow-client close-send failures
  do not hide downstream lag from metrics; the matching structured warning log
  includes pending gateway/downstream server-request ids plus affected pending
  and answered-but-unresolved thread ids plus pending worker ids for prompt
  correlation
- slow-client northbound send timeouts now also emit a dedicated
  `gateway_v2_client_send_timeouts` counter in addition to the terminal
  connection outcome and server-request cleanup metrics, so dashboards can
  track client backpressure directly without filtering broad connection
  outcomes; the matching structured warning log includes pending
  gateway/downstream server-request ids plus affected pending and
  answered-but-unresolved thread ids plus pending worker ids for prompt
  correlation before cleanup rejection is attempted
- gateway startup now rejects zero-valued v2 transport hardening settings for
  initialize timeout, client-send timeout, reconnect retry backoff, and max
  pending server-request / client-request counts, so misconfigured rollouts
  fail before accepting northbound compatibility traffic
- multi-worker thread routing diagnostics now also emit
  `gateway_v2_thread_list_deduplications` tagged by selected worker and
  `gateway_v2_thread_route_recoveries` tagged by success or miss, so
  operators can quantify cross-worker duplicate snapshots and failed lazy
  route probes during Stage B rollout; the real aggregate `thread/list`
  regression now also pins the deduplication metric on the same path that
  selects the visible worker snapshot and backfills sticky ownership
- lazy visible-thread route recovery coverage now also asserts the success and
  miss `gateway_v2_thread_route_recoveries` metrics while verifying those
  ordinary probe outcomes do not emit fail-closed or upstream-failure metrics,
  so operator dashboards distinguish route probing from real request failures
- degraded multi-worker thread discovery now also emits
  `gateway_v2_degraded_thread_discovery` tagged by request method and active
  reconnect backoff, so the intentionally partial `thread/list` /
  `thread/loaded/list` survival path is visible in metrics as well as
  structured warning logs; those warning logs now also include available,
  unavailable, and reconnect-backoff worker websocket URLs plus
  reconnect-backoff remaining seconds, including paired worker id / URL /
  remaining-second route diagnostics, so operators can identify the affected
  downstream sessions and retry timing without cross-referencing deployment
  config
- embedded and single-worker remote now have real `RemoteAppServerClient`
  drop-in harnesses that cover bootstrap and setup discovery, thread lifecycle
  and control, approvals and other server-request round trips,
  onboarding/feedback, standalone command execution, realtime workflows,
  reconnect and thread re-entry, plus app-server's current unmaterialized
  `thread/resume` / `thread/fork` semantics
- dedicated passthrough and regression coverage now spans the broader Stage A
  method inventory, including lower-frequency thread/control/setup methods,
  user-visible notifications, scope enforcement, malformed payload handling,
  initialize ordering, Ping/Pong behavior, the bounded pending server-request
  policy, longer-running turn and review lifecycle notifications, dynamic-tool
  `item/tool/call`, standalone command execution, and the current realtime
  request / notification set that the TUI consumes
- dedicated northbound notification coverage now also includes lower-frequency
  thread lifecycle notifications for `thread/archived`, `thread/unarchived`,
  and `thread/closed`, so archive and teardown state changes are pinned at the
  same v2 compatibility boundary as the existing thread and turn lifecycle
  streams
- the real multi-worker thread-control harness now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from both
  worker-owned threads on one shared `RemoteAppServerClient` session, so these
  lifecycle notifications are covered by Stage B client traffic as well as
  targeted northbound fixtures
- dedicated northbound notification coverage now also includes streamed item
  deltas for reasoning summaries, reasoning text, command output, and file
  changes, pinning `item/reasoning/summaryTextDelta`,
  `item/reasoning/textDelta`, `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta` on the visible-thread forwarding path
- dedicated northbound notification coverage now also includes the thread-
  scoped `error` turn-failure notification, so ordinary turn failure delivery
  is pinned at the gateway v2 compatibility boundary
- dedicated northbound notification coverage now also includes connection-
  scoped filesystem change delivery for `fs/changed`, so the watch setup /
  teardown surface has a pinned notification path in addition to the existing
  `fs/watch` and `fs/unwatch` request coverage
- the real single-worker remote compatibility harness now also observes
  `fs/changed` after `fs/watch`, so filesystem watch notification delivery is
  part of the release-quality remote baseline instead of only targeted
  northbound fixtures and multi-worker Stage B coverage
- the real single-worker remote reconnect harness now also re-exercises
  `fs/watch` and observes recovered-worker `fs/changed` delivery after the
  worker is back online, so filesystem watch notifications remain covered
  across ordinary remote worker recovery
- that same single-worker remote reconnect harness now also exercises
  `windowsSandbox/setupStart` and observes the recovered worker's
  `windowsSandbox/setupCompleted` notification, so the Windows sandbox setup
  surface is covered across ordinary remote worker recovery as well
- dedicated northbound passthrough coverage now also includes the
  `fuzzyFileSearch` request family plus `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` notifications, so the file-picker search
  surface is explicit in the Stage A compatibility gate
- the real embedded compatibility harness now also exercises the streaming
  `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, and
  `fuzzyFileSearch/sessionStop` request family against a real temporary
  filesystem root, including the `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` notifications through an unmodified
  `RemoteAppServerClient` session
- the real single-worker remote reconnect harness now also exercises
  `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop` after
  worker recovery, including the follow-up
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  notifications on the recovered northbound client session
- dedicated northbound passthrough coverage now also includes the basic
  filesystem operation family (`fs/readFile`, `fs/writeFile`,
  `fs/createDirectory`, `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and
  `fs/copy`), so file helper requests are explicit in the Stage A
  compatibility gate instead of only covering watch setup / teardown
- dedicated northbound passthrough coverage now also includes the low-frequency
  Windows sandbox setup surface: `windowsSandbox/setupStart`,
  `windows/worldWritableWarning`, and `windowsSandbox/setupCompleted`, so
  platform-specific setup prompts are pinned at the v2 compatibility boundary
- dedicated northbound passthrough coverage now also includes additional
  low-frequency v2 client requests that were previously only implicitly
  covered by generic forwarding: `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `mcpServer/resource/read`, `mcpServer/tool/call`, and
  `account/sendAddCreditsNudgeEmail`
- the real single-worker remote `RemoteAppServerClient` setup harness now also
  exercises those same low-frequency setup/config/MCP/account paths, including
  a real `thread/start` before the thread-scoped MCP resource and tool calls,
  so Stage A single-worker parity no longer relies only on targeted raw
  JSON-RPC passthrough fixtures for that method family
- the real single-worker remote setup harness now also observes
  `externalAgentConfig/import/completed` after `externalAgentConfig/import`,
  so release-quality remote client validation covers import completion delivery
  before the multi-worker duplicate-suppression path is involved
- multi-worker setup mutation routing now also fans out `marketplace/add`,
  `skills/config/write`, `experimentalFeature/enablement/set`, and
  `config/mcpServer/reload` across worker sessions, with real
  `RemoteAppServerClient` coverage, so worker-local marketplace, skill,
  feature, and MCP config state does not drift behind one shared connection
- multi-worker `account/sendAddCreditsNudgeEmail` now stays primary-worker
  affine, with real `RemoteAppServerClient` coverage, so one-shot account
  nudge emails are not duplicated across downstream workers
- multi-worker `windowsSandbox/setupStart` routing now also stays
  primary-worker affine, including reconnect-before-routing coverage plus
  fail-closed behavior while the primary worker remains in reconnect backoff
- the real embedded and single-worker remote compatibility harnesses now also
  exercise the fuzzy-file-search surface through unmodified
  `RemoteAppServerClient` sessions: embedded mode validates one-shot search
  against a real temporary filesystem root, and single-worker remote mode
  validates the one-shot plus streaming session start/update/stop requests and
  the resulting `fuzzyFileSearch/sessionUpdated` /
  `fuzzyFileSearch/sessionCompleted` notifications
- those same real embedded and single-worker remote compatibility harnesses
  now also exercise the basic filesystem operation family through unmodified
  `RemoteAppServerClient` sessions, with embedded mode validating real local
  file creation / read / metadata / directory / copy / remove behavior and
  single-worker remote mode validating the gateway-backed passthrough surface
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
- multi-worker basic filesystem operations now also stay primary-worker
  affine, so local file helper state does not silently fall through to a
  secondary worker while the primary worker is unavailable
- the real multi-worker `RemoteAppServerClient` filesystem harness now also
  exercises that primary-worker route for `fs/readFile`, `fs/writeFile`,
  `fs/createDirectory`, `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and
  `fs/copy`, verifying that one shared northbound session does not spread
  local file helper state across secondary workers
- that filesystem change notification coverage now also verifies multi-worker
  fan-in, so one shared northbound v2 session receives worker-local
  `fs/changed` events from more than one downstream app-server session
- the multi-worker filesystem-watch reconnect coverage now also verifies that
  a reconnected worker can emit `fs/changed` after a shared `fs/watch` is
  applied, so watch notification fan-in survives worker loss and lazy re-add
- multi-worker connection-state reconnect coverage now also verifies
  `mcpServer/startupStatus/updated` from a lazily re-added worker after
  `mcpServerStatus/list`, so MCP startup state remains visible on the shared
  northbound v2 session after worker recovery
- dedicated reconnect-backoff coverage now also pins the
  `mcpServer/oauth/login` worker-discovery fail-closed path, so OAuth login
  routing cannot silently select from an incomplete MCP inventory while a
  downstream worker remains unavailable; that coverage also asserts the
  fail-closed metric is tagged with active reconnect backoff
- multi-worker exact-duplicate connection-notification suppression now
  remembers a bounded history of forwarded payloads and source workers per
  method instead of only the most recent payload, so interleaved worker-local
  warning or setup-state payloads cannot allow an older cross-worker duplicate
  to leak back to the shared northbound client while long-lived sessions still
  have capped dedupe memory; same-worker repeats refresh their history position
  without being suppressed, and dedicated regressions pin the interleaved
  `account/updated`, `account/rateLimits/updated`,
  `account/login/completed`, `app/list/updated`, `warning`, `configWarning`,
  `deprecationNotice`, `mcpServer/oauthLogin/completed`,
  `mcpServer/startupStatus/updated`, `windows/worldWritableWarning`, and
  `windowsSandbox/setupCompleted` paths, same-worker repeat behavior across
  the same broader connection-state notification set, refreshed-history
  behavior, empty-payload import-completion repeats, the per-method history
  bound, and the absence of suppressed-notification metrics for legitimate
  same-worker repeats
- multi-worker `skills/changed` suppression now keeps the pending-refresh gate
  active when a client `skills/list` refresh fails, so repeated worker-local
  invalidations do not leak back to one shared northbound session until a
  refresh has actually succeeded; dedicated websocket coverage pins the failed
  refresh path and the existing successful-refresh reopen behavior
- that same `skills/changed` hardening now also covers a failed
  `skills/list` refresh followed by a later successful refresh on the same
  shared session, so the pending-refresh gate stays closed across the failure
  but still reopens after the client receives fresh aggregated skills state
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
  behind the gateway boundary and returning stable `model-offset:` cursors
  instead of falling back to the primary worker when clients request a page
  size or continuation cursor
- aggregated worker-local pagination now also fails closed when a downstream
  worker repeats a page cursor, so malformed paginated discovery responses
  cannot leave one shared northbound v2 request looping indefinitely
- the real multi-worker `RemoteAppServerClient` bootstrap harness now also
  exercises that paginated `model/list` path through the gateway's northbound
  WebSocket surface, including downstream worker-local page draining and
  follow-up `model-offset:` continuation requests
- the real multi-worker `RemoteAppServerClient` app and MCP status aggregation
  harnesses now also exercise downstream worker-local page draining before
  gateway-owned `apps-offset:` and `mcp-status-offset:` continuation cursors
  are returned to the shared northbound session
- dedicated multi-worker aggregation coverage now also verifies
  `experimentalFeature/list` drains worker-local pages before applying
  gateway-owned `experimental-feature-offset:` pagination, so capability
  discovery pagination follows the same Stage B boundary as model, app, and
  MCP inventory
- the real multi-worker `RemoteAppServerClient` capability harness now also
  exercises that paginated `experimentalFeature/list` path through the
  gateway's northbound WebSocket surface, including follow-up
  `experimental-feature-offset:` continuation requests after worker-local page
  draining
- the real embedded and single-worker remote compatibility harnesses now also
  exercise `windowsSandbox/setupStart` and the follow-up
  `windowsSandbox/setupCompleted` notification through unmodified
  `RemoteAppServerClient` sessions, extending the release-gate client path for
  a low-frequency platform setup flow that previously relied on targeted
  northbound passthrough coverage
- the real multi-worker `RemoteAppServerClient` filesystem/primary-worker
  harness now also exercises `windowsSandbox/setupStart` and the follow-up
  `windowsSandbox/setupCompleted` notification, verifying that the shared
  northbound session keeps that platform setup helper primary-worker affine
- the real multi-worker `RemoteAppServerClient` primary-worker harness now
  also exercises `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, and `fuzzyFileSearch/sessionStop`, plus
  the follow-up `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` notifications, so streaming file-search
  helper state stays primary-worker affine in the Stage B profile
- the real multi-worker same-session recovery harness now also verifies
  `fs/changed` fan-in after `fs/watch` reconnects and fans back out to a
  recovered worker, so worker-local watch events remain visible without a
  northbound reconnect
- the real multi-worker same-session bootstrap recovery harness now also
  observes `mcpServer/startupStatus/updated` after `mcpServerStatus/list`
  re-adds a recovered worker, so MCP startup-state notification fan-in is
  exercised through an unmodified `RemoteAppServerClient` session
- the real multi-worker `RemoteAppServerClient` thread-routing harness now
  also exercises `mcpServer/resource/read` and `mcpServer/tool/call` against
  worker-owned visible threads, so thread-scoped MCP resource and tool calls
  stay sticky to the owning worker in the shared-session Stage B profile
- that same multi-worker same-session recovery harness now also re-exercises
  `mcpServer/resource/read` and `mcpServer/tool/call` after the owning worker
  is lazily re-added, extending recovered-worker sticky routing coverage to
  thread-scoped MCP requests
- that same multi-worker same-session recovery harness now also re-exercises
  `account/sendAddCreditsNudgeEmail` after the primary worker is re-added, so
  the one-shot account nudge path stays primary-worker affine after recovery
- that same primary-worker recovery harness now also re-exercises
  `windowsSandbox/setupStart` plus the follow-up
  `windowsSandbox/setupCompleted` notification after the primary worker is
  re-added, so platform setup remains primary-worker affine on one shared
  northbound session
- that same primary-worker recovery harness now also re-exercises
  `fs/createDirectory`, `fs/writeFile`, `fs/readFile`, `fs/getMetadata`,
  `fs/readDirectory`, `fs/copy`, and `fs/remove` after the primary worker is
  re-added, so local filesystem helpers route back to the recovered primary
  worker without a northbound reconnect
- that same recovery harness now also re-exercises one-shot
  `fuzzyFileSearch` after the recovered worker is re-added, verifying the
  shared session can aggregate file-search results from both workers again
  after lazy reconnect
- that same primary-worker recovery harness now also re-exercises
  `fuzzyFileSearch/sessionStart`, `fuzzyFileSearch/sessionUpdate`, and
  `fuzzyFileSearch/sessionStop` after the primary worker is re-added, so
  streaming file-search helper sessions also stay primary-worker affine on one
  shared northbound session
- that same recovery path now also observes the follow-up
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`
  notifications from the recovered primary worker, so streaming file-search
  notification fan-in is covered without requiring a northbound reconnect
- the real multi-worker same-session recovery harness now also re-exercises
  `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, and `config/mcpServer/reload` after a
  recovered worker is re-added, so low-frequency setup mutations continue to
  fan out across the full worker set without a northbound reconnect
- the real multi-worker same-session bootstrap recovery harness now also
  re-exercises external-auth `account/login/start` with
  `chatgptAuthTokens` and `apiKey` after a recovered worker is re-added,
  observing deduplicated `account/login/completed` / `account/updated` fan-in
  for the token flow plus deduplicated `account/updated` delivery and follow-up
  aggregated `account/read` state for the API-key flow through the same
  `RemoteAppServerClient` session
- that same bootstrap recovery harness now also verifies duplicate recovered
  connection-state notifications remain suppressed before later login traffic,
  so restored worker fan-in cannot leak stale duplicate account, rate-limit,
  app, warning, or sandbox setup state onto the shared session
- the steady-state multi-worker external-auth harness now also explicitly
  verifies duplicate `account/login/completed` / `account/updated` emissions
  from token-login fanout are suppressed on the shared
  `RemoteAppServerClient` session
- the steady-state multi-worker setup-mutation harness now also observes the
  `skills/changed` invalidation emitted after `skills/config/write` fanout and
  verifies duplicate worker emissions stay suppressed on the shared
  `RemoteAppServerClient` session
- that same steady-state setup-mutation harness now also observes the
  `account/updated` notification emitted after `account/logout` fanout and
  verifies duplicate worker emissions stay suppressed on the shared
  `RemoteAppServerClient` session
- that same recovered setup-mutation harness now also observes the
  `skills/changed` invalidation emitted after recovered-worker
  `skills/config/write` fanout and verifies the duplicate worker emission is
  still suppressed on the shared `RemoteAppServerClient` session
- that same recovered setup-mutation harness now also observes the
  `externalAgentConfig/import/completed` notification emitted after
  recovered-worker `externalAgentConfig/import` fanout and verifies duplicate
  worker completions stay suppressed on the shared `RemoteAppServerClient`
  session
- that same recovered setup-mutation harness now also observes the
  `account/updated` notification emitted after recovered-worker
  `account/logout` fanout and verifies duplicate worker emissions stay
  suppressed on the shared `RemoteAppServerClient` session
- multi-worker remote mode now has a broad real shared-session Stage B harness
  for aggregated bootstrap/setup discovery, sticky thread and turn routing,
  translated server-request ids, plugin-management fallback, primary-worker
  onboarding / feedback / account-nudge / command-exec flows, fanout setup
  mutations, steady-state and reconnect realtime routing / fan-in, lazy worker
  re-add, and same-session recovery after worker loss
- multi-worker northbound initialization now also has direct coverage verifying
  client identity, experimental capability, and `optOutNotificationMethods`
  propagation to every downstream worker session, plus lazy-reconnect coverage
  verifying that a re-added worker receives the same initialized capability
  parameters, so shared-session capability negotiation stays aligned with the
  embedded and single-worker release baselines
- northbound v2 notification forwarding now also enforces exact
  `optOutNotificationMethods` suppression at the gateway boundary, with
  regression coverage showing an opted-out downstream notification is not
  forwarded and is counted as a suppressed notification
- opted-out notification suppression now also emits a structured gateway log
  with tenant/project scope, worker identity, method, and payload context, so
  operators can distinguish client-requested notification drops from duplicate
  suppression or hidden-thread policy filtering
- the real northbound WebSocket opt-out notification regression now also pins
  that structured suppression log end to end, including tenant/project scope,
  affected worker URL, notification method, and payload context alongside the
  existing `opted_out` suppression metric
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
  multi-worker sessions that survive one worker's protocol violation still
  leave an operator-visible diagnostic trail
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
  remote app-server transport, extending malformed-frame hardening beyond
  invalid text payloads
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
- downstream server requests received during the app-server initialize
  handshake now also have direct remote transport coverage for the unsupported
  request path: unsupported setup-time requests are rejected with a JSON-RPC
  method-not-found error while the initialize handshake can still complete
- the gateway northbound WebSocket boundary now also has direct regression
  coverage for a valid downstream server request received during the
  app-server initialize handshake, verifying that it is forwarded after
  northbound initialize completes and that the client response routes back to
  the owning worker
- that same northbound handshake boundary now also verifies unsupported
  downstream server requests received during app-server initialize are rejected
  with method-not-found at the worker transport while the gateway session still
  completes initialize and serves a follow-up client request
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
- that lazy-reconnect protocol-violation regression now also pins the paired
  worker reconnect metrics, verifying malformed recovered-worker handshakes
  record both the reconnect `attempt` and terminal `connect_failure` outcome
  alongside the downstream `invalid_jsonrpc` protocol-violation metric
- multi-worker notification fan-in coverage now also pins
  `rawResponseItem/completed` across worker-owned visible threads, so raw
  response item replay uses the same shared-session forwarding path as the
  existing turn lifecycle and item-progress notifications
- the real multi-worker turn-routing harness now also observes additional
  lower-frequency visible turn notifications from both worker-owned threads,
  including `item/plan/delta`, `item/reasoning/summaryPartAdded`,
  `item/commandExecution/terminalInteraction`, `turn/diff/updated`,
  `turn/plan/updated`, `thread/tokenUsage/updated`,
  `item/mcpToolCall/progress`, `thread/compacted`, `model/rerouted`, and
  `rawResponseItem/completed`, so those forwarding paths are exercised through
  an unmodified `RemoteAppServerClient` on one shared multi-worker session
- the real multi-worker thread-control harness now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from both
  worker-owned threads, extending broad Stage B notification fan-in beyond
  turn-scoped streams into thread lifecycle state changes
- the real multi-worker same-session recovery harness now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from a lazily
  re-added worker, so recovered-worker thread lifecycle notification fan-in is
  covered alongside the existing recovered turn and realtime streams
- that same recovery harness now also observes `thread/name/updated` after
  routing `thread/name/set` back to the lazily re-added worker, so recovered
  thread rename notification fan-in is covered through an unmodified client
  session
- the real multi-worker same-session recovery harness now also verifies that
  those lower-frequency turn notifications still fan in from a lazily re-added
  worker after reconnect, so recovered-worker turn replay no longer relies
  only on the basic lifecycle and streaming-delta notification set
- that same real multi-worker turn coverage now also observes
  `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed` through unmodified
  `RemoteAppServerClient` sessions in both steady state and after lazy worker
  re-add, so guardian approval auto-review UX notifications are covered by the
  broad Stage B client harness instead of only targeted northbound fixtures
- the real single-worker remote turn workflow harness now also observes
  `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed`, so guardian approval auto-review
  notification forwarding is pinned in the release-quality remote baseline as
  well as in targeted gateway fixtures and the multi-worker Stage B harness
- that same real multi-worker turn coverage now also observes turn-scoped
  `error` notifications in both steady state and after lazy worker re-add, so
  opportunistic model/tool warning surfaces are exercised through the broad
  Stage B client harness
- the real single-worker remote turn workflow harness now also observes
  turn-scoped `error` notifications, so opportunistic model/tool warning
  forwarding is pinned in the release-quality remote baseline as well as in
  targeted gateway fixtures and the multi-worker Stage B harness
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
- the real embedded turn workflow harness now also observes
  `item/reasoning/summaryPartAdded` from Responses summary-part events and
  `thread/tokenUsage/updated` after a token-bearing Responses completion, so
  those low-frequency notification forwarding paths are pinned through the
  in-process release-quality baseline rather than only through remote or
  targeted notification coverage
- the real embedded plan-mode harness now also observes `item/plan/delta`
  before the proposed-plan item completes, so streamed plan text is pinned
  through the in-process release-quality baseline as well
- the real multi-worker same-session bootstrap recovery harness now also
  observes `account/updated`, `account/rateLimits/updated`,
  `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`, and
  `windows/worldWritableWarning` from the lazily re-added worker after the
  corresponding discovery refreshes, so core connection-state and visible
  notice fan-in are covered through an unmodified `RemoteAppServerClient`
  session
- the real connection-state notification harnesses now also exercise
  `windowsSandbox/setupCompleted` alongside
  `windows/worldWritableWarning` through unmodified `RemoteAppServerClient`
  sessions, including multi-worker duplicate suppression and recovered-worker
  same-session delivery
- multi-worker reconnect hardening now includes route backfill,
  connection-state replay, reconnect retry backoff, fail-closed handling for
  degraded fanout / config / plugin / primary-worker requests, duplicate
  notification suppression, synthesized or deduplicated
  `serverRequest/resolved` behavior where needed, and preservation of
  multi-worker routing semantics while only one downstream worker is
  temporarily live
- the rollout docs now track the actual validation surface instead of only the
  early target shape:
  [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)
  explicitly records the current steady-state Stage B compatibility coverage,
  and [docs/gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md)
  keeps the deployment caveats and rollout guidance current
- the v2 rollout guidance now also includes account-aware multi-worker
  guardrails: all workers should be labeled with `--remote-account-id`,
  same-project affinity and cross-project distribution should be validated
  before widening traffic, bounded resumable-thread handoff should be verified
  only on explicit restore surfaces, and active live-context requests should
  be expected to fail closed when the owning account is exhausted
- remote multi-worker startup now warns when any worker is missing
  `--remote-account-id`, including the unlabeled worker ids and WebSocket URLs,
  so account-aware validation does not silently proceed with routes that cannot
  be tied back to account-capacity health or handoff evidence; blank account
  labels are rejected by the CLI and treated as unlabeled for programmatic
  remote runtime configuration, and blank remote worker WebSocket URLs are
  rejected before remote runtime startup whether they come from the CLI or
  programmatic gateway configuration; the normal remote startup log also
  reports `remote_account_labels_complete` and
  `remote_unlabeled_account_worker_count`, so complete and incomplete rollout
  configurations are visible from startup logs without waiting for `/healthz`
- `/healthz` now also exposes `remoteAccountLabelsComplete`,
  `remoteUnlabeledAccountWorkerCount`, `remoteUnlabeledAccountWorkerIds`, and
  `remoteUnlabeledAccountWorkers` for remote runtimes, so multi-worker rollout
  checks can verify account-aware routing labels, alert on the direct
  unlabeled-worker count, and inspect affected worker URLs from the health
  snapshot instead of reconstructing the same guardrail from individual
  `remoteWorkers[]` entries
- remote multi-worker startup now also emits
  `gateway_remote_account_label_events` metrics tagged by worker id and
  `event=labeled` or `event=unlabeled` for every configured worker, giving
  rollout dashboards the same account-label guardrail that startup logs and
  `/healthz` expose while distinguishing a fully labeled pool from missing
  metric data
- the v2 method matrix now also treats account-label completeness as an
  explicit Stage B rollout guardrail, and the real remote multi-worker
  `/healthz` regression pins that a two-worker validation profile without
  account labels reports both unlabeled worker routes through
  `remoteUnlabeledAccountWorkers`
- remote multi-worker `/healthz` coverage now also pins the fully labeled
  account-route branch, verifying `remoteAccountLabelsComplete=true`,
  `remoteUnlabeledAccountWorkerCount=0`, and empty unlabeled-worker lists when
  every worker has a configured `accountId`
- the v2 method matrix now also defines the Stage B rollout gate for
  multi-worker remote mode: steady-state, reconnect, degraded-route,
  slow-client, account-capacity, and fail-closed behavior must be validated in
  the target deployment shape before multi-worker remote can be documented as
  release-quality equivalent to embedded or single-worker remote
- the v2 rollout checklist now also calls out the concrete transport
  dashboards to inspect for notification fan-in, suppression, send failures,
  close-frame delivery failures, downstream server-request forward send
  failures, server-request answer and rejection delivery failures, and
  server-request lifecycle events, so operators validate the new observability
  surface during staged rollout rather than relying only on compatibility-flow
  success
- that rollout checklist now also calls out remote / multi-worker degraded
  topology counters for reconnect attempts, fail-closed requests, upstream
  failures while worker routes are unavailable, downstream backpressure, and
  slow-client timeouts, tying those dashboard checks back to `/healthz` worker
  state during staged validation
- `/healthz.v2Connections` now also reports cumulative worker reconnect
  outcome counts as `workerReconnectEventCounts`, per-worker splits as
  `workerReconnectEventWorkerCounts`, and the newest reconnect outcome through
  `lastWorkerReconnectEvent`, `lastWorkerReconnectEventWorkerId`, and
  `lastWorkerReconnectEventAt`, so degraded-topology rollout evidence can
  correlate the exported `gateway_v2_worker_reconnects` metric with the
  current health snapshot instead of relying only on logs
- the rollout guidance now also defines the promotion evidence required before
  multi-worker remote can be treated as release-quality: gateway and worker
  configuration, `/healthz` snapshots, `/v1/events` captures, metric exports,
  client transcripts, and documented fail-closed outcomes must agree on the
  affected worker, account, tenant/project scope, and handoff result
- the multi-worker promotion evidence now also requires an explicit method
  routing expectation for the validated build, covering which methods aggregate,
  fan out, stay primary-worker affine, use worker discovery, remain thread
  sticky, or use a bounded account-handoff surface, and keeps promotion scoped
  to the exact topology, account labels, timeout settings, pending-request
  limits, and v2 method families that were exercised
- the multi-worker rollout guidance now points operators back to the method
  matrix as the source of truth for each method's route class, and requires the
  same route-class expectations to be validated under steady-state, reconnect,
  degraded-route, slow-client, overload, and account-capacity conditions before
  a specific deployment shape is described as release-quality
- v2 client replies to thread-scoped approval, user-input, and elicitation
  server requests now also fail closed when the pending request's owning worker
  account is exhausted, emitting the same active-thread no-handoff
  account-capacity metric, structured log, and
  `gateway/accountActiveThreadHandoffFailed` operator event instead of
  delivering the answer to an exhausted account-backed worker
- that v2 server-request answer fail-closed path now also has real northbound
  WebSocket regression coverage: a multi-worker gateway forwards a
  thread-scoped user-input request, the owning worker account is marked
  exhausted before the client answers, the gateway closes the shared client
  session without delivering the answer downstream, and the account-capacity
  plus server-request lifecycle metrics identify the same worker and request
  class
- `/healthz.v2Connections` now also reports
  `activeConnectionServerRequestBacklogCount`,
  `activeConnectionMaxServerRequestBacklogCount`,
  `activeConnectionPeakServerRequestBacklogCount`,
  `activeConnectionServerRequestBacklogStartedAt`,
  `activeConnectionServerRequestBacklogWorkerCounts`,
  `activeConnectionServerRequestBacklogMethodCounts`,
  `lastConnectionServerRequestBacklogCount`,
  `lastConnectionMaxServerRequestBacklogCount`, and
  `lastConnectionServerRequestBacklogStartedAt`, plus
  `lastConnectionServerRequestBacklogWorkerCounts` and
  `lastConnectionServerRequestBacklogMethodCounts`, giving operators the total
  active backlog, the largest current backlog on any one active connection,
  the active and last-completed lifecycle peaks, the timestamp for the current
  active v2 server-request backlog, and per-worker plus per-method splits of
  pending versus answered-but-unresolved prompt buildup without exposing
  individual request ids
- `/healthz.v2Connections` now also reports those v2 server-request backlog
  method counts for active and last-completed connections, so approval,
  user-input, elicitation, and ChatGPT token-refresh buildup can be identified
  directly in health output and then correlated with lifecycle metrics and
  worker-route logs
- last-completed v2 server-request backlog timestamps now also get a
  conservative completion-time fallback when the terminal pending counts are
  first observed during teardown, so `/healthz.v2Connections` does not report a
  nonzero completed prompt backlog with a missing
  `lastConnectionServerRequestBacklogStartedAt`
- v2 teardown and failure diagnostics now include pending and
  answered-but-unresolved server-request method lists alongside the existing
  gateway ids, downstream ids, worker ids, worker WebSocket URLs, and thread
  ids, so slow-client, backpressure, protocol-violation, duplicate downstream
  request-id, saturation, and cleanup logs can be correlated with the same
  prompt method families shown in `/healthz.v2Connections`
- v2 slow-client, downstream-backpressure, unexpected client-reply, and
  client-side cleanup logs now also include `server_request_backlog_count`
  alongside pending and answered-but-unresolved prompt counts, so teardown
  diagnostics can be compared directly with
  `/healthz.v2Connections.*ServerRequestBacklogCount`
- worker-loss cleanup diagnostics now also include the method families for
  synthesized thread-scoped `serverRequest/resolved` notifications and stranded
  connection-scoped prompts, so worker disconnect logs identify whether the
  affected prompt lifecycle was approval, user-input, elicitation, or
  token-refresh without reconstructing it from request payloads
- v2 connection completion and audit logs now also include the summarized
  server-request backlog worker and method count fields, so normal connection
  teardown records the same prompt buildup dimensions exposed through
  `/healthz.v2Connections`
- v2 downstream-shutdown failure logs now also include the summarized pending
  client-request worker and method count fields alongside individual request
  ids, methods, worker ids, and worker WebSocket URLs, so shutdown failures can
  be correlated with the same background-command buildup dimensions exposed
  through `/healthz.v2Connections`
- v2 downstream-shutdown failure logs now also include
  `server_request_backlog_count` alongside pending and answered-but-unresolved
  prompt counts, so that failure path can be compared directly with
  `/healthz.v2Connections.*ServerRequestBacklogCount`
- v2 pending-client abort logs now also include summarized worker and method
  count fields alongside individual background request ids, methods, worker
  ids, and worker WebSocket URLs, so aborted `command/exec` or other
  long-running requests leave the same route and method dimensions as health
  snapshots and connection metrics
- v2 connection metrics now also include
  `gateway_v2_connection_pending_client_requests_by_worker`,
  `gateway_v2_connection_pending_client_requests_by_method`,
  `gateway_v2_connection_pending_server_requests_by_worker`,
  `gateway_v2_connection_answered_but_unresolved_server_requests_by_worker`,
  `gateway_v2_connection_pending_server_requests_by_method` and
  `gateway_v2_connection_answered_but_unresolved_server_requests_by_method`,
  tagged by connection outcome plus worker or method, so exported dashboards
  can split terminal background client-request and prompt backlog by worker
  route, command, approval, user-input, elicitation, and token-refresh family
  without relying only on health snapshots or logs
- the multi-worker rollout evidence checklist now also requires validating
  those v2 server-request backlog worker counts during approval, user-input,
  elicitation, and ChatGPT token-refresh flows, so pending and
  answered-but-unresolved prompt buildup can be tied back to the same worker
  route reported by lifecycle metrics and logs before a deployment shape is
  promoted
- the real multi-worker concurrent server-request harness now leaves
  user-input, permissions approval, MCP elicitation, and ChatGPT token-refresh
  prompts pending on both workers long enough to verify
  `/healthz.v2Connections` reports the matching per-worker and per-method
  backlog counts; the same regression also verifies earlier
  answered-but-unresolved prompts remain visible by worker and prompt family
  while the next prompt class is pending, and now checks the completed
  connection's last-backlog health snapshot so active and terminal rollout
  evidence stay aligned for all four prompt families
- v2 worker-loss cleanup now also publishes a `/v1/events` operator event as
  `gateway/v2ServerRequestCleanup` whenever cleanup resolves thread-scoped
  prompts or finds stranded connection-scoped prompts, carrying the affected
  worker route, remaining worker count, disconnect message, prompt counts,
  prompt method families, resolved thread ids, and the gateway / downstream
  server-request ids for the cleaned-up prompts; rollout evidence for pending
  server-request cleanup can now be collected from the event stream in addition
  to lifecycle metrics, health snapshots, and structured logs, and real
  northbound WebSocket regressions now pin that event payload for both a
  direct cleanup helper plus the pending and answered-but-unresolved
  worker-loss paths that strand connection-scoped prompts; the cleanup log and
  operator event are emitted as soon as the gateway removes the affected
  routes, before synthesized `serverRequest/resolved` notifications are sent
  to the northbound client, so a later slow-client send failure does not hide
  the cleanup from `/v1/events`
- the v2 rollout guidance now also includes concrete startup examples for the
  embedded baseline, single-worker remote baseline, and account-labeled
  multi-worker Stage B validation profile, including the auth and
  `--remote-account-id` constraints operators must satisfy before collecting
  promotion evidence
- `/healthz.v2Connections` now also reports v2 protocol-violation health as
  `protocolViolationCounts`, `protocolViolationWorkerCounts`,
  `lastProtocolViolationPhase`, `lastProtocolViolationReason`,
  `lastProtocolViolationWorkerId`, and `lastProtocolViolationAt`, mirroring
  `gateway_v2_protocol_violations{phase,reason}` while adding the affected
  downstream worker split for rollout evidence, so operators can identify
  malformed pre-initialize traffic, post-initialize client protocol
  violations, and worker-specific downstream app-server protocol regressions
  from health snapshots without relying only on metrics export or logs
- `/healthz.v2Connections` now also mirrors v2 fail-closed route protection as
  `failClosedRequestCounts`, `lastFailClosedRequestMethod`,
  `lastFailClosedRequestReconnectBackoffActive`, and
  `lastFailClosedRequestAt`, so degraded multi-worker rollout evidence can
  reconcile `gateway_v2_fail_closed_requests{method,reconnect_backoff_active}`
  with health snapshots when required worker routes are unavailable
- `/healthz.v2Connections` now also mirrors v2 request outcomes as
  `requestCounts`, `lastRequestMethod`, `lastRequestOutcome`,
  `lastRequestDurationMs`, `maxRequestDurationMs`, and `lastRequestAt`, so
  rollout evidence can reconcile ordinary success, policy, quota,
  protocol-violation, and internal-error request counts with
  `gateway_v2_requests{method,outcome}`, and compare the latest / largest
  observed gateway request latency with `gateway_v2_request_duration` when
  operators need a health-only latency sanity check
- `/healthz.v2Connections` now also mirrors completed v2 connection outcomes
  as `connectionOutcomeCounts` and tracks `maxConnectionDurationMs`, so
  rollout evidence can reconcile `gateway_v2_connections{outcome}` and
  `gateway_v2_connection_duration` with the same health snapshot that already
  carries the latest connection outcome, detail, duration, and pending
  request/backlog counts
- the real embedded and remote v2 healthz regressions now also pin
  `connectionOutcomeCounts` and `maxConnectionDurationMs` on actual
  connection teardown paths: normal client shutdown records the
  `client_closed` outcome in embedded, single-worker remote, and multi-worker
  remote modes, and the real single-worker slow-client timeout harness records
  the `client_send_timed_out` outcome alongside the gateway-owned timeout
  detail and terminal server-request backlog
- `/healthz.v2Connections` now also mirrors v2 request rejection diagnostics as
  `clientRequestRejectionCounts`, `lastClientRequestRejectionMethod`,
  `lastClientRequestRejectionReason`, `lastClientRequestRejectionAt`,
  `serverRequestRejectionCounts`, `lastServerRequestRejectionMethod`,
  `lastServerRequestRejectionReason`, and `lastServerRequestRejectionAt`, so
  overload, pending-limit, and scope-policy rejections can be reconciled with
  `gateway_v2_client_request_rejections{method,reason}` and
  `gateway_v2_server_request_rejections{method,reason}` from the same health
  snapshot used for fail-closed and transport-pressure rollout evidence
- `/healthz.v2Connections` now also mirrors v2 upstream request failures as
  `upstreamRequestFailureCounts`, `lastUpstreamRequestFailureMethod`,
  `lastUpstreamRequestFailureReconnectBackoffActive`, and
  `lastUpstreamRequestFailureAt`, so operators can correlate
  `gateway_v2_upstream_request_failures{method,reconnect_backoff_active}`
  with the same health snapshot used to diagnose degraded-route protection
- `/healthz.v2Connections` now also mirrors v2 transport pressure as
  `downstreamBackpressureCounts`, `lastDownstreamBackpressureWorkerId`,
  `lastDownstreamBackpressureAt`, `clientSendTimeoutCount`, and
  `lastClientSendTimeoutAt`, so rollout evidence can line up
  `gateway_v2_downstream_backpressure_events{worker_id}` and
  `gateway_v2_client_send_timeouts` with the terminal connection outcome
  fields in the same health response
- `/healthz.v2Connections` now also mirrors v2 multi-worker thread routing
  diagnostics as `threadListDeduplicationCounts`,
  `lastThreadListDeduplicationSelectedWorkerId`,
  `lastThreadListDeduplicationAt`, `threadRouteRecoveryCounts`,
  `lastThreadRouteRecoveryOutcome`, `lastThreadRouteRecoveryAt`,
  `degradedThreadDiscoveryCounts`, `lastDegradedThreadDiscoveryMethod`,
  `lastDegradedThreadDiscoveryReconnectBackoffActive`, and
  `lastDegradedThreadDiscoveryAt`, so rollout snapshots can explain duplicate
  thread-list selection, lazy route recovery, and intentionally degraded
  visible-thread discovery without relying only on metrics export or logs
- `/healthz.v2Connections` now also mirrors v2 suppressed notifications as
  `suppressedNotificationCounts`, `lastSuppressedNotificationMethod`,
  `lastSuppressedNotificationReason`, and `lastSuppressedNotificationAt`, so
  duplicate notification suppression, pending-refresh suppression, client
  opt-out drops, and hidden-thread notification drops can be reconciled with
  `gateway_v2_suppressed_notifications` from health snapshots as well as
  metrics and logs
- `/healthz.v2Connections` now also mirrors v2 notification fan-in delivery as
  `forwardedNotificationCounts`, `lastForwardedNotificationMethod`,
  `lastForwardedNotificationAt`, `notificationSendFailureCounts`,
  `lastNotificationSendFailureMethod`, `lastNotificationSendFailureOutcome`,
  and `lastNotificationSendFailureAt`, so operators can reconcile
  `gateway_v2_forwarded_notifications` and
  `gateway_v2_notification_send_failures` with the same health snapshot used
  to diagnose suppressed notifications and slow-client delivery failures
- `/healthz.v2Connections` now also mirrors v2 transport and server-request
  delivery failures as `clientResponseSendFailureCounts`,
  `downstreamShutdownFailureCounts`, `closeFrameSendFailureCounts`,
  `serverRequestForwardSendFailureCounts`,
  `serverRequestAnswerDeliveryFailureCounts`, and
  `serverRequestRejectionDeliveryFailureCounts`, plus corresponding `last*`
  fields, so slow-client response writes, failed close frames, downstream
  shutdown failures, and partially completed prompt delivery failures can be
  reconciled with their metrics without relying only on logs
- `/healthz.v2Connections` now also mirrors the broader v2 server-request
  lifecycle stream as `serverRequestLifecycleEventCounts`,
  `lastServerRequestLifecycleEvent`, `lastServerRequestLifecycleMethod`, and
  `lastServerRequestLifecycleAt`, so forwarded, answered, delivered, resolved,
  rejected, duplicate, cleanup, and delivery-failed prompt stages can be
  reconciled with `gateway_v2_server_request_lifecycle_events` from the same
  rollout health snapshot
- the HTTP `/healthz` regression suite now also serializes a non-empty
  `v2Connections` rollout snapshot containing connection outcomes, request
  outcomes, request rejections, fail-closed routes, upstream failures,
  transport pressure, routing diagnostics, notification delivery,
  server-request delivery failures, lifecycle events, and worker-specific
  protocol violations, so the new operator fields are covered at the
  northbound API boundary rather than only in the in-memory health registry

The remaining Phase 6 work is:

- keep using the real-client embedded and single-worker remote harnesses as the
  release gate, extending them when new v2 workflows land so those two
  topologies remain the drop-in baseline instead of drifting behind the gateway
- continue hardening the northbound v2 transport under overload and failure,
  especially slow-client paths, reconnect churn, partially completed
  server-request lifecycles, and the operator-visible logs / health / metrics
  that need to explain those failures in production
- keep expanding the multi-worker Stage B profile from broad validated
  steady-state and reconnect coverage into a release-quality compatibility
  target, closing the remaining parity and rollout-hardening gaps before it is
  documented as equivalent to embedded or single-worker remote mode
- finish project-aware account routing by validating the documented
  account-aware multi-worker guardrails and promotion-evidence checklist
  against real deployments, then closing any remaining gaps before promoting
  the bounded handoff profile to release-quality multi-worker guidance;
  arbitrary live active-context migration remains a separate planned
  capability
