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
  `item/completed`, `hookStarted`, and `hookCompleted`, so one shared client
  session still sees item and hook progress from the owning worker
- multi-worker remote runtime now also has a real northbound v2 client
  regression covering thread mutation routing across two workers, verifying
  that `thread/name/set` stays sticky to the owning worker and the aggregated
  `thread/read` / `thread/list` results reflect each worker's renamed thread
  state on the shared northbound connection
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
- the real multi-worker connection-state notification regressions now also
  cover `account/login/completed`, `warning`, `configWarning`, and
  `deprecationNotice` dedupe in both steady state and after worker reconnect,
  so onboarding-complete and other visible connection-state notifications stay
  single-delivery on one shared northbound session
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
  `mcpServerStatus/list`, `mcpServer/oauth/login`, and `skills/list`,
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
  (`thread/status/changed`, `turn/started`, `hookStarted`, `item/started`,
  `item/agentMessage/delta`, reasoning / command / file-change deltas,
  `hookCompleted`, `item/completed`, and `turn/completed`) plus the current
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
  `account/rateLimits/read`, unpaginated `model/list`,
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
  replies to an unknown server-request id before the gateway closes the socket
- malformed post-handshake client payloads now also have dedicated northbound
  regression coverage for the same teardown warning fields, pinning scope and
  pending-request ids before the gateway closes the socket
- that same malformed-payload teardown path now also pins any answered-but-
  unresolved server-request routes after the client has already replied but
  downstream `serverRequest/resolved` has not arrived yet
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
  ownership probes stay stable as Phase 6 transport hardening continues
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
  failed in the current loop, plus the next scheduled reconnect time, so
  transient worker loss is visible without inferring from logs alone
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
- rollout visibility is operator-facing instead of test-only:
  startup and `/healthz` distinguish embedded, single-worker remote, and the
  current partial multi-worker profile; `/healthz` exposes
  `initializeTimeoutSeconds`, `clientSendTimeoutSeconds`,
  `reconnectRetryBackoffSeconds`, and `maxPendingServerRequests`; remote-worker
  entries expose `lastStateChangeAt`, `lastErrorAt`, `reconnecting`,
  `reconnectAttemptCount`, and `nextReconnectAt`; `/healthz` now also exposes
  `v2Connections.activeConnectionCount`,
  `v2Connections.peakActiveConnectionCount`,
  `v2Connections.totalConnectionCount`,
  `v2Connections.lastConnectionStartedAt`,
  `v2Connections.lastConnectionDurationMs`,
  `v2Connections.lastConnectionPendingServerRequestCount`, and
  `v2Connections.lastConnectionAnsweredButUnresolvedServerRequestCount`, plus
  the latest completed v2 connection outcome/detail/timestamp; health, metrics,
  audit logs, and structured logs now all carry the last v2 connection
  duration needed to diagnose slow-client and short-lived failure paths
- that operator-facing `v2Connections` health snapshot now also has direct
  embedded, single-worker remote, and multi-worker remote regression coverage,
  pinning active, peak, and total connection counts plus the last-started and
  last-outcome fields across live and settled client sessions; multi-worker
  remote `/healthz` coverage now also pins the gateway-owned
  `client_send_timed_out` outcome/detail, duration, plus the last completed
  connection's pending and answered-but-unresolved server-request counts after
  a slow-client teardown via a real northbound WebSocket session
- v2 connection accounting now also records terminal outcome and decrements the
  active connection count even when an early handshake-time close frame or
  JSON-RPC error response cannot be delivered, so `/healthz` does not retain
  stale `activeConnectionCount` under setup-time send failures
- those operator-facing v2 connection and reconnect logs now also have direct
  regression coverage, pinning the fields and outcome mapping that rollout
  debugging relies on
- v2 connection completion and audit logs now also include terminal detail plus
  the pending and answered-but-unresolved server-request counts, so ordinary
  connection outcome logs carry the same stranded-session summary that
  `/healthz` and metrics expose
- v2 server-request saturation and scope-policy rejection now also emit a
  dedicated rejection counter tagged by server-request method and reason
  (`pending_limit` or `hidden_thread`), so overload and policy dashboards can
  distinguish bounded prompt-state pressure or hidden-thread prompt drops from
  ordinary connection outcomes
- multi-worker v2 reconnect activity now also emits a
  `gateway_v2_worker_reconnects` counter tagged by worker id and outcome
  (`attempt`, `success`, `connect_failure`, `replay_failure`, or
  `backoff_suppressed`), so reconnect churn can be tracked from metrics instead
  of only logs and `/healthz`
- those reconnect outcome counters now also have direct reconnect-path
  regression coverage for successful reconnects, connection failures, replay
  failures, and retry-backoff suppression, so the metric contract is pinned at
  the transport boundary rather than only at the observability helper
- multi-worker v2 fail-closed requests now also emit a
  `gateway_v2_fail_closed_requests` counter tagged by JSON-RPC method and
  whether any required worker route is currently held in reconnect backoff, so
  degraded-session protection can be measured separately from generic
  `internal_error` request outcomes
- primary-worker-only multi-worker requests now also have method-family
  reconnect-backoff regression coverage for config requirements, managed
  login, login cancellation, feedback upload, and standalone command control,
  basic filesystem operations, fuzzy file search, and Windows sandbox setup, so
  degraded-session fail-closed behavior is pinned across the whole primary
  route set instead of only a representative command request
- suppressed multi-worker connection notifications now also emit a
  `gateway_v2_suppressed_notifications` counter tagged by notification method
  and suppression reason (`duplicate`, `pending_refresh`, or
  `hidden_thread`), so rollout dashboards can distinguish healthy
  gateway-owned dedupe and scope filtering from missing worker events or
  client-side notification loss
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `windows/worldWritableWarning` and
  `windowsSandbox/setupCompleted`, so one shared northbound session does not
  surface duplicate platform setup notices when more than one worker emits the
  same payload
- duplicate downstream `serverRequest/resolved` replays that are dropped after
  gateway request-id translation now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_resolved_replay`, so worker replay anomalies are measurable
  from metrics instead of only visible in structured logs
- downstream server requests that reuse a still-pending gateway request id now
  also emit `gateway_v2_server_request_lifecycle_events` with
  `event=duplicate_pending_request` before the gateway fails the session
  closed, so request-id collision anomalies are visible in metrics alongside
  the close outcome and structured warning log
- worker-loss cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` counters for synthetic
  thread-scoped `serverRequest/resolved` notifications and stranded
  connection-scoped server requests, so disconnect-driven prompt cleanup is
  measurable in addition to the existing structured cleanup logs
- those worker-loss cleanup counters are recorded as soon as the gateway drains
  the affected routes, before any synthetic `serverRequest/resolved`
  notifications are sent, so slow-client send timeouts do not hide the cleanup
  event from metrics
- client-side connection cleanup now also emits
  `gateway_v2_server_request_lifecycle_events` counters for rejected
  thread-scoped pending prompts, rejected connection-scoped pending prompts,
  and answered-but-unresolved prompts left behind by client disconnect,
  protocol-violation, or slow-send teardown paths
- client replies for unknown server-request ids now also emit
  `gateway_v2_server_request_lifecycle_events` with
  `event=unexpected_client_server_request_response`, so protocol-violation
  prompt replies are visible in metrics before the gateway closes the
  northbound v2 connection
- malformed or out-of-order v2 client traffic now also emits
  `gateway_v2_protocol_violations` with `phase` and `reason` tags, covering
  pre-initialize ordering errors, invalid JSON-RPC payloads, invalid UTF-8
  binary frames, and repeated `initialize` requests after the handshake; direct
  northbound websocket regressions pin those metric tags at the transport
  boundary
- downstream app-server event stream lag/backpressure now also emits
  `gateway_v2_downstream_backpressure_events` with the affected worker id
  before the gateway sends the close frame, so slow-client close-send failures
  do not hide downstream lag from metrics
- slow-client northbound send timeouts now also emit a dedicated
  `gateway_v2_client_send_timeouts` counter in addition to the terminal
  connection outcome and server-request cleanup metrics, so dashboards can
  track client backpressure directly without filtering broad connection
  outcomes
- multi-worker thread routing diagnostics now also emit
  `gateway_v2_thread_list_deduplications` tagged by selected worker and
  `gateway_v2_thread_route_recoveries` tagged by success or miss, so
  operators can quantify cross-worker duplicate snapshots and failed lazy
  route probes during Stage B rollout
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
- dedicated northbound notification coverage now also includes connection-
  scoped filesystem change delivery for `fs/changed`, so the watch setup /
  teardown surface has a pinned notification path in addition to the existing
  `fs/watch` and `fs/unwatch` request coverage
- dedicated northbound passthrough coverage now also includes the
  `fuzzyFileSearch` request family plus `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` notifications, so the file-picker search
  surface is explicit in the Stage A compatibility gate
- dedicated northbound passthrough coverage now also includes the basic
  filesystem operation family (`fs/readFile`, `fs/writeFile`,
  `fs/createDirectory`, `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and
  `fs/copy`), so file helper requests are explicit in the Stage A
  compatibility gate instead of only covering watch setup / teardown
- dedicated northbound passthrough coverage now also includes the low-frequency
  Windows sandbox setup surface: `windowsSandbox/setupStart`,
  `windows/worldWritableWarning`, and `windowsSandbox/setupCompleted`, so
  platform-specific setup prompts are pinned at the v2 compatibility boundary
- multi-worker `windowsSandbox/setupStart` routing now also stays
  primary-worker affine, including reconnect-before-routing coverage plus
  fail-closed behavior while the primary worker remains in reconnect backoff
- the real embedded and single-worker remote compatibility harnesses now also
  exercise the fuzzy-file-search surface through unmodified
  `RemoteAppServerClient` sessions: embedded mode validates one-shot search
  against a real temporary filesystem root, and single-worker remote mode
  validates the one-shot plus streaming session start/update/stop requests
- those same real embedded and single-worker remote compatibility harnesses
  now also exercise the basic filesystem operation family through unmodified
  `RemoteAppServerClient` sessions, with embedded mode validating real local
  file creation / read / metadata / directory / copy / remove behavior and
  single-worker remote mode validating the gateway-backed passthrough surface
- multi-worker `fuzzyFileSearch` routing now also stays primary-worker
  affine, including real northbound WebSocket coverage for lazy primary-worker
  reconnect plus fail-closed behavior while the primary worker remains in
  reconnect backoff, so local file-search state does not silently fall through
  to a secondary worker
- multi-worker basic filesystem operations now also stay primary-worker
  affine, so local file helper state does not silently fall through to a
  secondary worker while the primary worker is unavailable
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
- multi-worker remote mode now has a broad real shared-session Stage B harness
  for aggregated bootstrap/setup discovery, sticky thread and turn routing,
  translated server-request ids, plugin-management fallback, primary-worker
  onboarding / feedback / command-exec flows, steady-state and reconnect
  realtime routing / fan-in, lazy worker re-add, and same-session recovery
  after worker loss
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
