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
- the real embedded and single-worker remote harnesses now also verify the
  gateway-owned scope-policy rejection path for history/path-based
  `thread/resume` and path-based `thread/fork`, so those guarded flows are
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
- that same exact-duplicate suppression now also covers
  `externalAgentConfig/import/completed`, so one shared client session does
  not surface duplicate import-finished notifications when a fanout import
  completes on more than one worker
- that same exact-duplicate suppression now also covers
  `account/login/completed`, so one shared client session does not surface
  duplicate onboarding-complete notifications when a fanout login flow
  completes on more than one worker
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
  v2 session can still complete an `item/tool/call` dynamic-tool round trip
  with `serverRequest/resolved` ordering plus `item/started` /
  `item/completed` forwarding before `turn/completed`
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
  `app/list/updated`, `mcpServer/startupStatus/updated`, and
  `externalAgentConfig/import/completed`
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
  can resume round-robin `thread/start` and aggregated `thread/list` / sticky
  `thread/read` coverage for the recovered worker without reconnecting
- that same-session recovery path now also verifies sticky `thread/resume` and
  `thread/fork` on the recovered worker, plus a follow-up `thread/read` of the
  returned forked thread to confirm worker ownership is re-registered for the
  new thread id after reconnect
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
- that same-session recovery path now also verifies connection-scoped
  `fs/watch` and `fs/unwatch` fan out across the recovered worker and the
  surviving worker on one shared northbound session, so shared filesystem watch
  setup and teardown do not silently drop the recovered worker after reconnect
- that same-session recovery path now also verifies recovered primary
  worker handling for `configRequirements/read`, managed
  `account/login/start`, `account/login/cancel`, the resulting
  `account/login/completed` notification, `feedback/upload`, and the
  standalone `command/exec` control plane
  (`command/exec`, `command/exec/write`, `command/exec/resize`,
  `command/exec/terminate`, plus `command/exec/outputDelta`), so primary
  setup, onboarding, and standalone command flows stay usable after reconnect
  on one shared client session too
- that same-session recovery path now also verifies later aggregated
  bootstrap/discovery refreshes re-add a recovered worker into one shared
  client session, covering `account/read`, `account/rateLimits/read`,
  unpaginated `model/list`, `externalAgentConfig/detect`, threadless
  `app/list`, `skills/list`, `mcpServerStatus/list`,
  `thread/realtime/listVoices`, cwd-aware `config/read`,
  `experimentalFeature/list`, and `collaborationMode/list`
- dedicated northbound multi-worker v2 websocket regression coverage now also
  verifies that a managed `account/login/start` can reconnect a missing
  primary worker and still forward that recovered worker's resulting
  `account/login/completed` notification over the shared northbound session
- that same-session recovery path now also verifies sticky
  `turn/start`, `turn/steer`, `turn/interrupt`, `thread/realtime/start`,
  `thread/realtime/appendText`, `thread/realtime/appendAudio`, and
  `thread/realtime/stop` on the recovered worker, so active-turn and realtime
  control requests do not silently fall back to the surviving worker after
  reconnect
- that same-session recovery path now also verifies the resulting recovered
  worker turn and realtime notifications still fan in on the shared northbound
  session, covering `thread/status/changed`, `turn/started`, `hookStarted`,
  `item/started`, `item/agentMessage/delta`,
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, `item/fileChange/outputDelta`,
  `hookCompleted`, `item/completed`, `turn/completed`,
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that same-session recovery path now also verifies translated
  thread-scoped approval / elicitation server requests plus the
  connection-scoped `account/chatgptAuthTokens/refresh` round trip still
  route back through a lazily re-added worker on one shared northbound
  session, instead of leaking stale worker-local request ids after reconnect
- dedicated northbound multi-worker v2 websocket regression coverage now also
  verifies that a later client request can reconnect a missing worker and then
  still forward subsequent server requests from that recovered worker over the
  shared northbound session, covering both a thread-scoped
  `item/tool/requestUserInput` round trip and a connection-scoped
  `account/chatgptAuthTokens/refresh` round trip
- that same lazy-reconnect path now also replays the client's prior
  `initialized` notification to any re-added downstream worker session before
  later requests are routed there, so recovered workers re-enter the same
  post-handshake app-server state instead of only receiving `initialize`
- that same lazy-reconnect path now also replays any active `fs/watch`
  registrations to a re-added downstream worker session before later filesystem
  watch traffic routes there, so shared watch subscriptions survive worker
  recovery on one northbound session
- that same lazy-reconnect path now also emits structured gateway logs for
  reconnect attempts, reconnect failures, connection-state replay failures, and
  successful worker re-add with initialized/fs-watch replay context, so
  multi-worker recovery debugging does not depend only on request-level errors
- multi-worker lazy reconnect now also applies a short per-worker retry
  backoff after failed reconnect attempts or failed connection-state replay, so
  one repeatedly unhealthy worker does not trigger an immediate reconnect storm
  on every later client request while the shared northbound session stays up
- connection-scoped fanout mutations now also fail closed while any downstream
  worker remains unavailable during reconnect retry backoff, instead of
  silently applying shared setup or watch state to only the surviving subset
- cwd-aware `config/read` now also fails closed while any downstream worker
  remains unavailable during reconnect retry backoff, instead of silently
  falling back to a surviving worker whose config layers do not match the
  requested path
- worker-discovery plugin requests now also fail closed while any downstream
  worker remains unavailable during reconnect retry backoff, instead of
  silently picking a surviving worker from an incomplete plugin inventory
- primary-worker-only requests now also fail closed while that primary worker
  remains unavailable during reconnect retry backoff, instead of silently
  drifting onto a surviving secondary worker in the shared session
- those same multi-worker fail-closed request paths now also emit structured
  gateway warning logs with request method, scope, live worker ids, missing
  worker ids, missing worker websocket URLs, and reconnect-backoff worker ids,
  so degraded routing decisions are visible without relying only on request
  errors or `/healthz` polling
- dedicated northbound multi-worker v2 websocket regression coverage now also
  verifies shared-session fan-in for the full current realtime notification
  set across workers, including `thread/realtime/started`,
  `thread/realtime/itemAdded`, `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- dedicated northbound multi-worker v2 regression coverage now verifies exact
  duplicate connection-state notifications such as `account/updated` are
  emitted only once on the shared northbound session
- dedicated northbound multi-worker v2 regression coverage now also verifies
  exact-duplicate suppression for `externalAgentConfig/import/completed`, so
  fanout setup imports do not surface multiple completion notifications on one
  shared client session
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
  the current per-worker health state, last observed worker error, and the
  latest worker health / error timestamps so operators can distinguish stale
  historical errors from a fresh outage
- `/healthz` remote-worker entries now also expose whether the gateway is
  actively reconnecting that worker plus the next scheduled reconnect time, so
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
  (`initializeTimeoutSeconds`, `clientSendTimeoutSeconds`,
  `reconnectRetryBackoffSeconds`, and `maxPendingServerRequests`) so rollout
  validation does not depend only on startup logs
- `/healthz` remote-worker entries now also expose `lastStateChangeAt` and
  `lastErrorAt`, so rollout checks can tell whether a worker is currently
  flapping or only carrying a historical last-error string
- `/healthz` remote-worker entries also expose `reconnecting` and
  `nextReconnectAt`, so operators can see when the remote runtime is still in
  its background reconnect loop instead of only seeing a generic unhealthy
  worker
- remote worker disconnects, reconnect attempts, reconnect failures, and
  successful reconnects now also emit structured gateway logs with worker id,
  websocket URL, and retry timing so rollout debugging does not depend only on
  `/healthz` polling
- northbound v2 connections now also emit structured gateway logs with
  connection outcome, scope, duration, and terminal error detail when present,
  so operators can diagnose handshake, slow-client, and downstream-session
  failures without depending only on metrics or audit-enabled logs
- real `RemoteAppServerClient` harnesses now cover embedded and single-worker
  remote drop-in client flows across bootstrap, setup/discovery, thread
  lifecycle, turn control, approvals and other server-request round trips,
  onboarding/feedback, standalone command execution, realtime workflows, and
  reconnect / thread re-entry behavior
- embedded compatibility coverage now also preserves app-server's current
  unmaterialized-thread semantics for `thread/resume` and `thread/fork`
- dedicated northbound v2 passthrough and regression coverage now spans the
  broader Stage A method inventory, lower-frequency thread/control/setup
  methods, user-visible notifications, scope enforcement, malformed payload
  handling, initialize ordering, Ping/Pong behavior, and the bounded pending
  server-request policy
- v2 parity coverage now includes longer-running turn and review lifecycle
  notifications, dynamic-tool `item/tool/call`, standalone command execution,
  and the current realtime request/notification set that the TUI consumes
- multi-worker remote mode now has a real shared-session Stage B harness for
  aggregated bootstrap/setup discovery, sticky thread and turn routing,
  translated server-request ids, plugin management fallback, primary-worker
  onboarding / feedback / command-exec flows, realtime fan-in, reconnect
  recovery, lazy worker re-add, and same-session recovery after worker loss
- multi-worker northbound transport now also hardens reconnect behavior with
  route backfill, connection-state replay, reconnect retry backoff, fail-closed
  handling for degraded fanout/config/plugin/primary-worker requests, duplicate
  notification suppression, and synthesized or deduplicated
  `serverRequest/resolved` behavior where needed
- that same multi-worker reconnect hardening now also preserves multi-worker
  routing semantics when one shared v2 session is temporarily down to a single
  live downstream worker, so primary-worker-only requests still fail closed,
  connection-scoped fanout and dedupe paths do not silently collapse into
  single-worker behavior, and translated server-request ids remain
  gateway-owned until missing workers reconnect
- v2 transport now applies the same gateway-owned scope, admission, audit, and
  metrics policies as HTTP traffic
- method-by-method coverage status and rollout caveats are kept current in
  [docs/gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md)
  and [docs/gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md)

The remaining Phase 6 work is:

- keep broadening real-client validation so embedded and single-worker remote
  remain the release-quality drop-in baseline as the gateway evolves
- continue hardening around load, reconnect, slow-client behavior, and
  operator-visible rollout signals for the northbound v2 transport
- broaden and harden the multi-worker Stage B coordination path until it can be
  documented as full compatibility instead of a bounded profile with caveats
