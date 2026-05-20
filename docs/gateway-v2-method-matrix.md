# Gateway v2 Method Coverage Matrix

This matrix defines the Stage A compatibility target for `codex-gateway` when a
Codex client connects over the app-server v2 WebSocket protocol.

Source inventory:

- client request usage was derived from
  `codex-rs/tui/src/app_server_session.rs`
- client server-request handling was derived from
  `codex-rs/tui/src/app/app_server_requests.rs`
- current gateway implementation status was checked against
  `codex-rs/gateway/src/northbound/v2.rs`,
  `codex-rs/gateway/src/v2.rs`, and `codex-rs/gateway/src/embedded.rs`

Stage A scope:

- release-quality baseline topologies: embedded runtime and remote runtime with
  exactly one downstream worker
- transport model: one northbound v2 connection maps to one downstream
  app-server session
- multi-worker remote runtime now has partial Stage B support; this matrix keeps
  Stage A as the drop-in baseline and calls out the current multi-worker
  additions separately where they affect rollout guidance

Stage B rollout gate:

- multi-worker remote remains a bounded validation profile until broad
  steady-state, reconnect, degraded-route, and slow-client coverage is
  exercised for the deployment shape being promoted
- the deployment evidence must identify which methods aggregate, fan out, stay
  primary-worker affine, use worker discovery, remain sticky to thread routes,
  or use bounded account-handoff surfaces, and must show those route classes
  behaved as documented for the validated build
- account-backed multi-worker validation requires every worker to be labeled
  with `--remote-account-id`, plus explicit checks for same-project affinity,
  cross-project account distribution, quota-aware new-thread failover, and
  bounded resumable-thread handoff
- release-quality multi-worker guidance must continue to distinguish explicit
  restore surfaces, which may hand off to a replacement account-backed worker,
  from live active-context requests, which must fail closed when the owning
  account is exhausted
- `/healthz`, `/v1/events`, metrics, and audit logs must agree on worker
  health, reconnect state, account-capacity events, and fail-closed decisions
  before multi-worker remote is documented as equivalent to embedded or
  single-worker remote
- server-request rollout evidence must include the v2 backlog health fields
  that split pending and answered-but-unresolved prompts by owning worker, so
  prompt buildup can be correlated with lifecycle metrics and worker-route logs
  without exposing individual request ids; the matching method-count fields and
  worker-loss cleanup method lists must also be checked so approval,
  user-input, elicitation, and token-refresh buildup are distinguishable in
  health output and disconnect diagnostics

Stage B route-class validation checklist:

| Route class | Validate with | Evidence to capture |
| --- | --- | --- |
| aggregation | `account/read`, `model/list`, `thread/list`, `thread/loaded/list`, `skills/list`, `thread/realtime/listVoices` | merged response shape, duplicate handling, fail-closed behavior while required workers are unavailable, and recovered-worker re-add on the same northbound session |
| fanout mutation | `account/login/start` for external auth, `config/batchWrite`, `config/value/write`, `skills/config/write`, `fs/watch`, `fs/unwatch` | every configured worker receives the mutation, duplicate notifications are suppressed, and the request fails closed instead of updating only surviving workers during reconnect backoff |
| primary-worker affinity | managed login, `account/login/cancel`, `feedback/upload`, `account/sendAddCreditsNudgeEmail`, standalone command control, streaming fuzzy-file-search sessions | side effects are not duplicated across workers, the primary worker is reconnected before routing, and primary-worker backoff produces the documented fail-closed error |
| worker discovery | `plugin/read`, `plugin/install`, `plugin/uninstall`, `mcpServer/oauth/login`, legacy `gitDiffToRemote` | first successful worker selection, incomplete-worker-set fail-closed behavior, and recovered-worker discovery on a later request |
| thread-sticky routing | `thread/read`, thread mutation methods, `turn/start`, `turn/steer`, `turn/interrupt`, thread-scoped MCP calls | route ownership stays pinned to the visible thread owner, lazy route recovery behaves as documented, and hidden or unavailable routes fail closed |
| bounded account handoff | `thread/read`, `thread/resume`, `thread/fork`, rollout-path `thread/resume` / `thread/fork`, legacy `getConversationSummary` variants | replacement-account success preserves the requested thread id or rollout path, updates sticky routing, emits the expected `/v1/events` account event, and records matching account-capacity metrics |
| live active-context fail-closed | `turn/start`, `turn/steer`, `turn/interrupt`, realtime append/stop, thread-scoped MCP calls, approval and elicitation replies | exhausted owner accounts do not silently replay live or side-effecting work on another worker; client-visible errors, `/v1/events`, health fields, metrics, and audit logs identify the same worker/account/scope |
| account-label rollout guardrail | multi-worker remote startup with `--remote-account-id` labels | every worker is labeled before account-aware validation; blank worker WebSocket URLs fail closed before remote startup; normal startup logs report account-label completeness and unlabeled-worker count, incomplete-label warning logs include the affected worker ids and WebSocket URLs, `/healthz.remoteAccountLabelsComplete`, `remoteUnlabeledAccountWorkerCount`, `remoteUnlabeledAccountWorkerIds`, and `remoteUnlabeledAccountWorkers` identify incomplete labels, and `gateway_remote_account_label_events{event="labeled"|"unlabeled",worker_id=...}` metrics are emitted for dashboards |
| connection-outcome health mirror | normal completion, client disconnects, slow-client timeouts, protocol violations, downstream failures | `/healthz.v2Connections.connectionOutcomeCounts`, `lastConnectionOutcome`, `lastConnectionDetail`, `lastConnectionDurationMs`, and `maxConnectionDurationMs` mirror `gateway_v2_connections{outcome}` and `gateway_v2_connection_duration`, so rollout evidence can reconcile terminal connection behavior and duration outliers without relying only on metrics export or audit logs |
| request-outcome health mirror | ordinary success, policy, quota, protocol-violation, internal-error v2 request outcomes, and request latency | `/healthz.v2Connections.requestCounts`, `lastRequestMethod`, `lastRequestOutcome`, `lastRequestDurationMs`, `maxRequestDurationMs`, and `lastRequestAt` mirror `gateway_v2_requests{method,outcome}` while surfacing the latest and largest observed request latency for comparison with `gateway_v2_request_duration`, so rollout evidence can reconcile method-level request results and latency outliers without relying only on metrics export or audit logs |
| request-rejection health mirror | saturated `command/exec` client requests, pending-limit server requests, and hidden-thread server requests | `/healthz.v2Connections.clientRequestRejectionCounts`, `serverRequestRejectionCounts`, and their `last*` fields mirror `gateway_v2_client_request_rejections{method,reason}` and `gateway_v2_server_request_rejections{method,reason}`, so rollout evidence can identify overload or scope-policy rejections without relying only on metrics export or logs |
| thread routing diagnostics health mirror | duplicate `thread/list` snapshots, lazy visible-thread route recovery, degraded discovery during worker loss | `/healthz.v2Connections.threadListDeduplicationCounts`, `threadRouteRecoveryCounts`, `degradedThreadDiscoveryCounts`, and their `last*` fields mirror `gateway_v2_thread_list_deduplications`, `gateway_v2_thread_route_recoveries`, and `gateway_v2_degraded_thread_discovery`, so rollout evidence can identify selected duplicate thread-list workers, lazy route recovery outcomes, and intentionally degraded discovery methods without relying only on metrics export or logs |
| suppressed-notification health mirror | duplicate connection-state notifications, pending-refresh invalidations, and client opt-out drops | `/healthz.v2Connections.suppressedNotificationCounts`, `lastSuppressedNotificationMethod`, `lastSuppressedNotificationReason`, and `lastSuppressedNotificationAt` mirror `gateway_v2_suppressed_notifications{method,reason}`, so rollout evidence can distinguish intentional notification drops from missing fan-in without relying only on metrics export or logs |
| notification delivery health mirror | forwarded downstream notifications and northbound notification send failures | `/healthz.v2Connections.forwardedNotificationCounts`, `lastForwardedNotificationMethod`, `lastForwardedNotificationAt`, `notificationSendFailureCounts`, `lastNotificationSendFailureMethod`, `lastNotificationSendFailureOutcome`, and `lastNotificationSendFailureAt` mirror `gateway_v2_forwarded_notifications{method}` and `gateway_v2_notification_send_failures{method,outcome}`, so rollout evidence can distinguish successful fan-in, intentional suppression, and slow-client delivery failure without relying only on metrics export or logs |
| transport and server-request delivery failure health mirror | slow-client response writes, failed close frames, downstream shutdown failure, server-request forward failures, answer delivery failures, and rejection delivery failures | `/healthz.v2Connections.clientResponseSendFailureCounts`, `downstreamShutdownFailureCounts`, `closeFrameSendFailureCounts`, `serverRequestForwardSendFailureCounts`, `serverRequestAnswerDeliveryFailureCounts`, `serverRequestRejectionDeliveryFailureCounts`, and their `last*` fields mirror the corresponding `gateway_v2_*_failure*` metrics so rollout evidence can explain partially completed response and prompt delivery lifecycles without relying only on metrics export or logs |
| server-request lifecycle health mirror | forwarded, answered, delivered, resolved, rejected, duplicate, cleanup, and delivery-failed prompt lifecycle events | `/healthz.v2Connections.serverRequestLifecycleEventCounts`, `lastServerRequestLifecycleEvent`, `lastServerRequestLifecycleMethod`, and `lastServerRequestLifecycleAt` mirror `gateway_v2_server_request_lifecycle_events{event,method}`, so rollout evidence can follow partially completed prompt stages without relying only on metrics export or logs |
| server-request cleanup event stream | worker-loss cleanup for pending or answered-but-unresolved thread-scoped prompts and stranded connection-scoped prompts | `/v1/events` publishes `gateway/v2ServerRequestCleanup` with the affected worker route, remaining worker count, disconnect message, cleanup counts, prompt method families, resolved thread ids, and gateway / downstream server-request ids, so rollout evidence can correlate cleanup lifecycle metrics and logs with an operator-visible event-stream record |
| protocol-violation health mirror | malformed client frames, repeated initialize, duplicate client request ids, downstream malformed or out-of-order traffic | `/healthz.v2Connections.protocolViolationCounts`, `protocolViolationWorkerCounts`, `lastProtocolViolationPhase`, `lastProtocolViolationReason`, `lastProtocolViolationWorkerId`, and `lastProtocolViolationAt` mirror `gateway_v2_protocol_violations{phase,reason}` while splitting downstream violations by worker id, so rollout evidence can identify whether violations came from pre-initialize ordering, post-initialize client traffic, or a specific downstream app-server worker without relying only on metrics export or logs |
| health response serialization | non-empty v2 rollout diagnostics returned by `GET /healthz` | the HTTP health route regression serializes representative non-empty values for connection outcomes, request outcomes, request rejections, fail-closed routes, upstream failures, transport pressure, routing diagnostics, notification delivery, server-request delivery failures, lifecycle events, and worker-specific protocol violations, so the documented fields are pinned at the northbound JSON boundary as well as in the health registry |
| background client-request observability | `command/exec` and other long-running client requests | `/healthz.v2Connections.activeConnectionPendingClientRequestStartedAt`, `activeConnectionMaxPendingClientRequestCount`, `activeConnectionPeakPendingClientRequestCount`, `activeConnectionPendingClientRequestWorkerCounts`, `activeConnectionPendingClientRequestMethodCounts`, `lastConnectionMaxPendingClientRequestCount`, `lastConnectionPendingClientRequestStartedAt`, `lastConnectionPendingClientRequestWorkerCounts`, and `lastConnectionPendingClientRequestMethodCounts` match the worker route, request method, buildup age, terminal connection outcome, lifecycle peak, and `gateway_v2_connection_pending_client_requests_by_worker` / `gateway_v2_connection_pending_client_requests_by_method` metrics so background request buildup remains explainable after teardown; the real slow-client `command/exec` WebSocket regression now pins the active health snapshot while the downstream request is still pending before the terminal timeout snapshot is recorded |
| server-request backlog observability | approval, user-input, elicitation, and ChatGPT token-refresh server requests | `/healthz.v2Connections.activeConnectionServerRequestBacklogWorkerCounts`, `lastConnectionServerRequestBacklogWorkerCounts`, `activeConnectionServerRequestBacklogMethodCounts`, `lastConnectionServerRequestBacklogMethodCounts`, `activeConnectionPeakServerRequestBacklogCount`, and `lastConnectionMaxServerRequestBacklogCount` match the worker route, prompt method family, pending versus answered-but-unresolved stage, lifecycle peak, lifecycle metrics, cleanup or resolution logs, and `gateway_v2_connection_pending_server_requests_by_worker` / `gateway_v2_connection_answered_but_unresolved_server_requests_by_worker` metrics; the real concurrent multi-worker harness now pins active and last-completed health counts for user-input, permissions approval, MCP elicitation, and ChatGPT token-refresh prompts across both workers while earlier prompt classes remain answered-but-unresolved; worker-loss cleanup logs also identify the resolved thread-scoped and stranded connection-scoped prompt methods |

Status legend:

- `transparent passthrough`: gateway proxies the v2 method without
  method-specific translation
- `policy interception`: gateway intentionally terminates or shapes part of the
  flow at the transport boundary
- `deferred`: not part of the current drop-in target, or blocked on a broader
  topology/policy gap

## Handshake and Session Lifecycle

| Method / flow | Why current clients use it | Gateway status | Notes |
| --- | --- | --- | --- |
| `initialize` | Required by every v2 client session | `policy interception` | Gateway authenticates the WebSocket upgrade, enforces an explicit initialize handshake timeout, connects a downstream app-server session, propagates client identity/capabilities downstream, enforces exact `optOutNotificationMethods` suppression at the northbound boundary, and returns a gateway-owned `InitializeResponse`. |
| `initialized` | Required handshake completion notification | `transparent passthrough` | Gateway accepts the northbound notification and downstream session setup already sends `initialized` as part of the app-server client transport. |
| connection close / downstream disconnect | Required for normal session teardown | `transparent passthrough` | Downstream disconnect terminates the northbound connection. Malformed downstream JSON-RPC is now reported separately as `downstream_protocol_violation` and records a downstream protocol-violation metric, including invalid text JSON-RPC after initialization or during the downstream initialize handshake, unsupported binary data frames after initialization, non-text frames during the downstream initialize handshake, and post-initialize JSON-RPC responses or errors with unknown request ids. |

## Bootstrap and Session Setup Requests

These are the v2 requests the current TUI issues during bootstrap or early
session setup.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `account/read` | Bootstrap auth/account state | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `account/read` refresh re-adds a recovered worker into merged auth state for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `account/rateLimits/read` | Background rate-limit refresh for status surfaces | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded and single-worker remote compatibility harnesses, and by multi-worker remote aggregation coverage that preserves the primary historical view from the first worker while merging the per-limit map across workers. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `account/rateLimits/read` refresh re-adds a recovered worker into the shared per-limit view for the same session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `model/list` | Bootstrap model picker/default model discovery | `transparent passthrough` | Required for drop-in startup parity and covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `model/list` refresh still succeeds after worker recovery. Multi-worker remote mode now aggregates model inventory across workers with gateway-owned pagination, drains worker-local model pages behind the gateway boundary, fails closed if a worker repeats a page cursor, has real northbound WebSocket bootstrap coverage for follow-up `model-offset:` continuation requests, has reconnect regression coverage showing that a later aggregated `model/list` refresh re-adds a recovered worker into merged model inventory for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `externalAgentConfig/detect` | Import existing external agent config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `externalAgentConfig/detect` refresh still succeeds after worker recovery. Multi-worker remote mode now also has reconnect regression coverage showing that a later aggregated `externalAgentConfig/detect` refresh re-adds a recovered worker into merged import-discovery results for the shared session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `externalAgentConfig/import` | Import external agent config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses, with single-worker remote also observing the follow-up `externalAgentConfig/import/completed` notification, and by the real single-worker reconnect harness after worker recovery. Real multi-worker reconnect coverage now also verifies that a later `externalAgentConfig/import` request reconnects a recovered worker before the shared import fans out, observes the follow-up `externalAgentConfig/import/completed` notification, and verifies duplicate worker completions stay suppressed on the shared `RemoteAppServerClient` session. Dedicated northbound websocket reconnect coverage also pins that recovered-worker fanout on the shared client session. |
| `app/list` | Connector/app discovery and refresh | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later threadless `app/list` refresh still succeeds after worker recovery. Multi-worker remote mode now aggregates the current threadless `app/list` path across workers, including gateway-owned pagination over the merged result set and real northbound WebSocket coverage that drains worker-local app pages before returning `apps-offset:` cursors, while thread-scoped `app/list` stays sticky to the owning worker and is covered by the real multi-worker compatibility harness. Dedicated reconnect regression coverage now also verifies that a later threadless `app/list` refresh re-adds a recovered worker into the shared aggregated result set, that a later thread-scoped `app/list` request still routes to that recovered worker on the same session, and dedicated northbound websocket reconnect coverage now pins the threadless recovered-worker aggregation path on the shared client session. If the owning account-backed worker is marked exhausted, thread-scoped app discovery fails closed instead of reading another account's worker-local app context. |
| `mcpServerStatus/list` | MCP inventory/bootstrap refresh | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `mcpServerStatus/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates connection-scoped MCP server inventory across workers with gateway-owned pagination and real northbound WebSocket coverage that drains worker-local MCP inventory pages before returning `mcp-status-offset:` cursors. Dedicated reconnect regression coverage now also verifies that a later `mcpServerStatus/list` refresh re-adds a recovered worker into the shared aggregated result set, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `mcpServer/oauth/login` | Start OAuth login for a configured MCP server | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded, single-worker remote, and multi-worker remote compatibility harnesses, including the follow-up `mcpServer/oauthLogin/completed` notification on the northbound client session. The real single-worker reconnect harness now also verifies that a later `mcpServer/oauth/login` still succeeds after worker recovery and still forwards the completion notification on the recovered session. Multi-worker remote mode now also uses worker-discovery fallback for this request, with dedicated reconnect regression coverage showing that a later `mcpServer/oauth/login` can reconnect a recovered worker before selecting the downstream session that owns the named MCP server, and dedicated northbound websocket reconnect coverage now also pins the follow-up `mcpServer/oauthLogin/completed` notification on that recovered session. Real multi-worker same-session recovery coverage now also verifies that `mcpServer/oauth/login` routes back to the recovered worker and still forwards the resulting completion notification on one shared client session. |
| `skills/list` | Skills picker / refresh | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `skills/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates `skills/list` across workers, suppresses duplicate `skills/changed` invalidations until the client refreshes, keeps that pending-refresh suppression active when an aggregated refresh fails, reopens fresh invalidation delivery after a later successful refresh on the same shared session, fails closed instead of returning a partial skill inventory while a required worker is unavailable during reconnect backoff, and has dedicated reconnect regression coverage showing that a later `skills/list` request re-adds a recovered worker into the shared session result set. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `skills/config/write` | Enable or disable a skill by path/name | `transparent passthrough` | Covered by dedicated gateway passthrough tests for the low-frequency skill configuration write path, by the real single-worker remote `RemoteAppServerClient` setup harness, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode now fans this setup mutation out across workers and has real shared-session coverage, including same-session recovery coverage after a worker is re-added, so worker-local skill enablement does not drift behind the gateway boundary. The real steady-state and recovered setup-mutation harnesses now also observe the follow-up `skills/changed` invalidation and verify duplicate worker emissions remain suppressed on the shared session. |
| `plugin/list` | Plugin catalog discovery | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. The real single-worker reconnect harness now also verifies that a later threadless `plugin/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also has dedicated aggregation coverage for merged marketplace inventory, featured plugins, and installed-state selection across workers, including a regression that keeps an installed worker copy selected when later workers report the same plugin as only available. Dedicated reconnect regression coverage now also verifies that a later threadless `plugin/list` refresh re-adds a recovered worker into the shared marketplace view, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can re-include the recovered worker's plugin state without reconnecting. |
| `plugin/read` | Plugin detail view | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests using the real northbound v2 client harness. The real single-worker reconnect harness now also verifies that a later `plugin/read` request still succeeds after worker recovery. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the selected plugin request. Dedicated reconnect regression coverage now also verifies that a later `plugin/read` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/read` now fails closed instead of selecting from an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/read` routes back to the recovered worker on one shared client session. |
| `marketplace/add` | Add a plugin marketplace source | `transparent passthrough` | Covered by dedicated gateway passthrough tests for the low-frequency marketplace onboarding path, by the real single-worker remote `RemoteAppServerClient` setup harness, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode now fans this setup mutation out across workers and has real shared-session coverage, including same-session recovery coverage after a worker is re-added, so worker-local marketplace state stays aligned. |
| `config/value/write` | Plugin enablement and other targeted config updates | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, and by the real single-worker reconnect harness after worker recovery; multi-worker remote mode now fans this connection-scoped mutation out across workers so plugin/config state does not drift. Real multi-worker reconnect coverage now also verifies that a later `config/value/write` request reconnects a recovered worker before the shared write fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `config/batchWrite` | Reload user config | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Real multi-worker reconnect coverage now also verifies that a later `config/batchWrite` request reconnects a recovered worker before the shared write fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `account/login/start` | Onboarding sign-in start flow | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded compatibility harness for the external-auth `chatgptAuthTokens` path, by the real single-worker remote compatibility harness for both managed-auth and external-auth onboarding flows, by the real multi-worker remote compatibility harness for the current external-auth onboarding path, and by dedicated multi-worker northbound regression coverage that verifies `apiKey` and `chatgptAuthTokens` login fanout to every downstream worker session while managed ChatGPT login flows remain on the primary worker because they return worker-local `loginId` state. The real multi-worker external-auth harness now also observes deduplicated token-flow `account/login/completed` and `account/updated` delivery before follow-up aggregated `account/read`, and the real multi-worker API-key harness observes deduplicated `account/updated` delivery before follow-up aggregated `account/read`. Dedicated northbound websocket reconnect coverage now also pins both the recovered-worker fanout path for external-auth `apiKey` login and the recovered primary-worker path for managed ChatGPT login on the shared client session. Real multi-worker same-session recovery coverage now also verifies the managed primary-worker login path after the recovered worker is re-added, and the same real bootstrap recovery harness now verifies external-auth `chatgptAuthTokens` plus `apiKey` fanout after worker re-add, including deduplicated token-flow notifications, API-key `account/updated` delivery, and follow-up API-key account state. |
| `account/login/cancel` | Cancel a pending onboarding sign-in flow | `transparent passthrough` | Covered by dedicated gateway passthrough tests, by the real embedded compatibility harness for post-login cancellation semantics, by the real single-worker remote compatibility harness for both managed-auth and external-auth onboarding flows, and by the real multi-worker remote compatibility harness for the current primary-worker external-auth path. Dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker path on the shared client session. Real multi-worker same-session recovery coverage now also verifies that `account/login/cancel` still reaches the recovered primary worker on one shared client session. |
| `account/logout` | Sign-out flow | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Real multi-worker reconnect coverage now also verifies that a later `account/logout` request reconnects a recovered worker before the shared logout fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. The real multi-worker setup-mutation harness now also observes deduplicated `account/updated` delivery after logout fanout in both steady state and same-session recovery. |
| `account/sendAddCreditsNudgeEmail` | Send the add-credits / usage-limit nudge email | `transparent passthrough` | Covered by dedicated gateway passthrough tests for the account quota nudge path, by the real single-worker remote `RemoteAppServerClient` setup harness, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode now keeps this one-shot account side effect primary-worker affine, including real shared-session coverage, same-session recovery coverage after the primary worker is re-added, and reconnect-backoff fail-closed method-family coverage, so the gateway does not fan out duplicate nudge emails or fall through to a secondary worker during primary-worker recovery. |
| `memory/reset` | Reset memory mode state | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Real multi-worker reconnect coverage now also verifies that a later `memory/reset` request reconnects a recovered worker before the shared reset fans out, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fanout on the shared client session. |
| `feedback/upload` | Submit in-product feedback from an active session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded, single-worker remote, and multi-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `feedback/upload` request still succeeds after worker recovery. Dedicated northbound websocket reconnect coverage now also pins the recovered primary-worker route on the shared client session. Real multi-worker same-session recovery coverage now also verifies that `feedback/upload` still routes through the recovered primary worker without reconnecting the northbound client. |

## Supporting Configuration Requests

These requests are used by some clients and test tools outside the main TUI
bootstrap hot path, but they still need stable v2 compatibility behavior.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `config/read` | Read effective config and optional layer metadata | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded, single-worker remote, and multi-worker compatibility coverage for both threadless and cwd-aware reads. The real single-worker reconnect harness now also verifies that a later threadless `config/read` refresh still succeeds after worker recovery. Multi-worker remote mode now also has reconnect regression coverage showing that later threadless and cwd-aware `config/read` refreshes re-add a recovered worker before selecting either the recovered primary worker or the config layers that match the requested cwd. Dedicated northbound websocket reconnect coverage now also pins both recovered-worker selection paths on the shared client session. During reconnect retry backoff, threadless `config/read` now fails closed instead of silently falling through to a surviving secondary worker's config, and cwd-aware `config/read` now fails closed instead of silently falling back to a surviving worker whose config layers do not match the requested path. |
| `configRequirements/read` | Read config requirements and validation metadata | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded, single-worker remote, and primary-worker multi-worker compatibility coverage. The real single-worker reconnect harness now also verifies that a later `configRequirements/read` request still succeeds after worker recovery. Dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker path on the shared client session. Primary-worker route-failure coverage now pins the per-request `internal_error` metric and audit log before returning the gateway-owned JSON-RPC error. Real multi-worker same-session recovery coverage now also verifies that `configRequirements/read` still routes through the recovered primary worker on one shared client session. |
| `experimentalFeature/list` | Discover experimental capability flags | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `experimentalFeature/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also aggregates capability discovery across workers, drains worker-local feature pages before applying gateway-owned `experimental-feature-offset:` pagination, merges duplicate feature flags by OR-ing enabled/default state, and has real northbound WebSocket coverage for follow-up `experimental-feature-offset:` continuation requests. Dedicated reconnect regression coverage now also verifies that a later aggregated `experimentalFeature/list` refresh re-adds a recovered worker into the shared feature inventory. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `experimentalFeature/enablement/set` | Update experimental feature enablement | `transparent passthrough` | Covered by dedicated gateway passthrough tests for the low-frequency feature-toggle mutation path, by the real single-worker remote `RemoteAppServerClient` setup harness, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode now fans this setup mutation out across workers and has real shared-session coverage, including same-session recovery coverage after a worker is re-added, so worker-local feature enablement state stays aligned. |
| `collaborationMode/list` | Discover supported collaboration modes | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies that a later `collaborationMode/list` refresh still succeeds after worker recovery. Multi-worker remote mode now also deduplicates and sorts the aggregated capability set across workers. Dedicated reconnect regression coverage now also verifies that a later aggregated `collaborationMode/list` refresh re-adds a recovered worker into the shared preset inventory. Dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation on the shared client session. |
| `config/mcpServer/reload` | Reload MCP server configuration | `transparent passthrough` | Covered by dedicated gateway passthrough tests for the low-frequency MCP config reload path, by the real single-worker remote `RemoteAppServerClient` setup harness, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode now fans this setup mutation out across workers and has real shared-session coverage, including same-session recovery coverage after a worker is re-added, so worker-local MCP config state stays aligned. |
| `mcpServer/resource/read` | Read an MCP resource in thread context | `transparent passthrough` | Covered by dedicated gateway passthrough tests for thread-scoped MCP resource reads, and now also by the real single-worker remote `RemoteAppServerClient` setup harness after a visible `thread/start`. Multi-worker remote mode now has real shared-session coverage verifying that thread-scoped MCP resource reads stay sticky to the owning worker, including after that worker is lazily re-added to the same session. If the owning account-backed worker is marked exhausted, resource reads fail closed instead of using another account's worker-local MCP context. |
| `mcpServer/tool/call` | Invoke an MCP tool in thread context | `transparent passthrough` | Covered by dedicated gateway passthrough tests for thread-scoped MCP tool calls, including the optional argument payload, and now also by the real single-worker remote `RemoteAppServerClient` setup harness after a visible `thread/start`. Multi-worker remote mode now has real shared-session coverage verifying that thread-scoped MCP tool calls stay sticky to the owning worker, including after that worker is lazily re-added to the same session. If the owning account-backed worker is marked exhausted, tool calls fail closed instead of invoking worker-local MCP tools through another account. |
| `fuzzyFileSearch` | File picker/search results for local roots | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests and by the real embedded plus single-worker remote compatibility harnesses. The embedded harness now also exercises a real temporary filesystem root before starting a streaming fuzzy-search session, and the real single-worker reconnect harness now also verifies a later one-shot search after worker recovery. Multi-worker remote mode now fans one-shot search across all configured workers, fails closed while any required worker is unavailable during reconnect backoff, deduplicates repeated file matches by root/path/type, keeps the highest-scoring duplicate, and returns a score-sorted merged result set. Real multi-worker `RemoteAppServerClient` filesystem and same-session recovery harnesses now also verify fan-in from two workers on one shared northbound session, and dedicated request-routing plus northbound WebSocket reconnect coverage verifies missing workers are reconnected before the aggregate is served. Streaming fuzzy-search sessions remain primary-worker-local because their session ids own worker-local search state. |
| `fuzzyFileSearch/sessionStart` | Start a streaming fuzzy-file-search session | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. The real single-worker reconnect harness now also verifies session start after worker recovery. Multi-worker remote mode routes this primary-worker-local session setup through the shared northbound session, routes it back to a recovered primary worker through real northbound WebSocket reconnect coverage, has real same-session recovery coverage after the primary worker is re-added, and fails closed during primary-worker reconnect backoff. |
| `fuzzyFileSearch/sessionUpdate` | Update the query for a streaming fuzzy-file-search session | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses, with matching notification forwarding coverage for `fuzzyFileSearch/sessionUpdated`. The real single-worker reconnect harness now also verifies session update plus `fuzzyFileSearch/sessionUpdated` delivery after worker recovery. Multi-worker remote mode routes this primary-worker-local session update through the shared northbound session, has real shared-session notification coverage for `fuzzyFileSearch/sessionUpdated`, routes it back to a recovered primary worker through real northbound WebSocket reconnect coverage, has real same-session recovery coverage after the primary worker is re-added, and fails closed during primary-worker reconnect backoff. |
| `fuzzyFileSearch/sessionStop` | Stop a streaming fuzzy-file-search session | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses, with matching notification forwarding coverage for `fuzzyFileSearch/sessionCompleted`. The real single-worker reconnect harness now also verifies session stop plus `fuzzyFileSearch/sessionCompleted` delivery after worker recovery. Multi-worker remote mode routes this primary-worker-local session teardown through the shared northbound session, has real shared-session notification coverage for `fuzzyFileSearch/sessionCompleted`, routes it back to a recovered primary worker through real northbound WebSocket reconnect coverage, has real same-session recovery coverage after the primary worker is re-added, and fails closed during primary-worker reconnect backoff. |
| `fs/readFile` | Read local file contents for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/writeFile` | Write local file contents for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/createDirectory` | Create local directories for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/getMetadata` | Read local file metadata for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/readDirectory` | List local directory entries for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/remove` | Remove local files/directories for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/copy` | Copy local files/directories for app/tool filesystem helpers | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, by real embedded plus single-worker remote compatibility harnesses, and by the real single-worker reconnect harness after worker recovery. Multi-worker remote mode treats this as a primary-worker-local filesystem operation, fails closed during primary-worker reconnect backoff, and now has real northbound WebSocket coverage verifying that the shared session routes this family to the primary worker plus real same-session recovery coverage after the primary worker is re-added. |
| `fs/watch` | Subscribe the current connection to filesystem change notifications | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses, with the single-worker remote harness now also observing the follow-up `fs/changed` notification through an unmodified `RemoteAppServerClient` session and the single-worker reconnect harness verifying recovered-worker `fs/changed` delivery after `fs/watch`. Multi-worker remote mode now fans connection-scoped watches out across workers so one shared northbound session can observe worker-local filesystem changes instead of only the primary worker's watch set. Dedicated reconnect regression coverage now also verifies that a later `fs/watch` request reconnects a recovered worker before the shared watch is applied, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker fanout on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can still fan `fs/watch` back out across the recovered worker and the surviving worker without reconnecting, including `fs/changed` fan-in from both worker-local watch sets. |
| `fs/unwatch` | Remove a prior filesystem watch for the current connection | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility harnesses. Multi-worker remote mode now fans connection-scoped unwatch requests out across workers so one shared northbound session can tear down worker-local watch state consistently. Dedicated reconnect regression coverage now also verifies that a later `fs/unwatch` request reconnects a recovered worker before the shared watch teardown runs, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker fanout on the shared client session. Real multi-worker same-session recovery coverage now also verifies that one shared northbound client can still fan `fs/unwatch` back out across the recovered worker and the surviving worker without reconnecting. |
| `windowsSandbox/setupStart` | Start Windows sandbox setup from the client UI | `transparent passthrough` | Covered by dedicated northbound gateway passthrough tests, and now by real embedded plus single-worker remote compatibility harnesses that also observe the follow-up `windowsSandbox/setupCompleted` notification through unmodified `RemoteAppServerClient` sessions. The real single-worker reconnect harness now also verifies the request and completion notification after worker recovery. Multi-worker remote mode keeps this low-frequency platform setup helper primary-worker affine, including reconnect-before-routing coverage, reconnect-backoff fail-closed coverage, real northbound WebSocket coverage verifying the shared session routes setup plus the completion notification through the primary worker, and real same-session recovery coverage after the primary worker is re-added. |

Additional multi-worker recovery coverage:

- Real same-session recovery tests now also verify that later aggregated
  refreshes for `account/read`, `getAuthStatus`, `account/rateLimits/read`,
  `model/list`, `externalAgentConfig/detect`, threadless `app/list`, threadless
  `plugin/list`, `skills/list`, `mcpServerStatus/list`,
  `thread/realtime/listVoices`, cwd-aware `config/read`,
  `experimentalFeature/list`, and `collaborationMode/list` re-add a recovered
  worker into the shared northbound session instead of leaving bootstrap and
  discovery state pinned to the surviving subset. The same recovery harness now
  also verifies duplicate recovered connection-state notifications stay
  suppressed before later login traffic on that shared session.
- The same lazy-reconnect path now also replays active `fs/watch`
  registrations onto any re-added downstream worker session before later
  filesystem watch traffic routes there, so shared watch subscriptions survive
  worker recovery on one northbound connection.

## Command Execution Requests

These requests drive the standalone `command/exec` control plane outside the
normal thread/turn flow.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `command/exec` | Start a standalone command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests, including a streaming/PTTY-shaped request body plus the final `command/exec` response. The real embedded compatibility harness now validates the in-process baseline flow for `command/exec` plus `command/exec/outputDelta`, and now also starts a live PTY-backed process for follow-up control requests while the final `command/exec` response remains pending on the same northbound WebSocket connection. Pending long-running `command/exec` responses are bounded by the gateway per-connection pending-request limit, so overload returns a gateway-owned rate-limit error instead of accumulating unbounded background upstream requests; dedicated northbound v2 regression coverage verifies that a rejected second `command/exec` does not close the WebSocket or prevent the original pending response from completing, pins the corresponding `rate_limited` / `ok` v2 request metrics, and pins the structured saturation warning log fields. Slow-client teardown coverage now also verifies pending `command/exec` request ids, methods, worker ids, and worker WebSocket URLs appear in diagnostics while the background request is still active, and now also pins the active pending-client `/healthz.v2Connections` count, max count, started-at timestamp, worker split, and method split before terminal timeout cleanup. Completed-but-undelivered final responses now record the connection-owned failure outcome in per-method request metrics after active-route cleanup, and aborted background requests now have direct audit-log coverage for their terminal request outcome. The real single-worker remote harness validates the broader standalone control plane through a gateway-backed remote worker session, and the real single-worker reconnect harness now also verifies that `command/exec` plus `command/exec/outputDelta` still work after ordinary worker recovery. The real multi-worker primary-worker harness validates the baseline flow through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, primary-worker route fail-closed coverage now pins the per-request `internal_error` metric and audit log before the northbound connection ends, and the real multi-worker same-session recovery harness now also verifies that `command/exec` plus `command/exec/outputDelta` still flow through the re-added primary worker without reconnecting the northbound client. |
| `command/exec/write` | Write stdin bytes to an active command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real embedded compatibility harness now validates `command/exec/write` against a live PTY-backed process through the in-process gateway transport. The real single-worker remote harness validates `command/exec/write` through a gateway-backed remote worker session, and the real single-worker reconnect harness now also verifies it after ordinary worker recovery. The real multi-worker primary-worker harness validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/write` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/write` still reaches the re-added primary worker on one shared client session. |
| `command/exec/resize` | Resize an active PTY-backed command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real embedded compatibility harness now validates `command/exec/resize` against a live PTY-backed process through the in-process gateway transport. The real single-worker remote harness validates `command/exec/resize` through a gateway-backed remote worker session, and the real single-worker reconnect harness now also verifies it after ordinary worker recovery. The real multi-worker primary-worker harness validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/resize` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/resize` still reaches the re-added primary worker on one shared client session. |
| `command/exec/terminate` | Terminate an active command-execution session | `transparent passthrough` | Covered by dedicated gateway passthrough tests. The real embedded compatibility harness now validates `command/exec/terminate` against a live PTY-backed process through the in-process gateway transport. The real single-worker remote harness validates `command/exec/terminate` through a gateway-backed remote worker session, and the real single-worker reconnect harness now also verifies it after ordinary worker recovery. The real multi-worker primary-worker harness validates the same path through one shared gateway-backed multi-worker session. In multi-worker remote mode this follows the primary-worker connection path; dedicated reconnect regression coverage verifies that a later `command/exec/terminate` request reconnects a missing primary worker before that routing occurs, dedicated northbound websocket reconnect coverage now also pins that recovered primary-worker route on the shared client session, and the real multi-worker same-session recovery harness now also verifies that `command/exec/terminate` still reaches the re-added primary worker on one shared client session. |

## Plugin Management Requests

These are the plugin-management requests the current TUI issues after bootstrap.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `plugin/install` | Install a selected plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. The real single-worker reconnect harness now also verifies that a later `plugin/install` request still succeeds after worker recovery and still updates follow-up `plugin/list` state. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the install request. Dedicated reconnect regression coverage now also verifies that a later `plugin/install` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/install` now fails closed instead of mutating plugin state on an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/install` can still mutate recovered-worker plugin state without reconnecting the northbound client. |
| `plugin/uninstall` | Remove an installed plugin | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `plugin/list` state changes. The real single-worker reconnect harness now also verifies that a later `plugin/uninstall` request still succeeds after worker recovery and still updates follow-up `plugin/list` state. Dedicated multi-worker northbound regression coverage now also verifies fallback routing to the first worker that can satisfy the uninstall request. Dedicated reconnect regression coverage now also verifies that a later `plugin/uninstall` request reconnects a recovered worker before that fallback selection runs, and dedicated northbound websocket reconnect coverage now pins that recovered-worker fallback on the shared client session. During reconnect retry backoff, `plugin/uninstall` now fails closed instead of mutating plugin state on an incomplete worker set. Real multi-worker same-session recovery coverage now also verifies that `plugin/uninstall` can still clear recovered-worker plugin state without reconnecting the northbound client. |

## Thread and Turn Requests

These methods are used by the current TUI for normal thread lifecycle and turn
control.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/start` | Start a new session | `transparent passthrough` | Covered by embedded and single-worker remote gateway tests. Real multi-worker remote compatibility tests now also cover steady-state `thread/start` routing and `thread/started` notification fan-in across one shared northbound session, and same-session recovery coverage now also verifies that a later `thread/start` can reuse a re-added worker without reconnecting the client. |
| `thread/resume` | Resume an existing session | `policy interception` | `threadId`-based resume is covered by the real embedded thread re-entry harness, by single-worker remote compatibility tests, and by real multi-worker routing and same-session recovery regressions. History-based resume is now treated as connection-scoped so it does not trip visibility checks on its placeholder `threadId`, and path-based resume is now allowed when the rollout path is already visible in the current tenant/project scope; real multi-worker compatibility coverage plus dedicated northbound JSON-RPC regressions cover the visible-path success path, and the real multi-worker same-session recovery harness now also verifies that visible-path resume stays sticky on a recovered worker after reconnect, while embedded and single-worker remote harnesses continue to pin the hidden-path rejection path. Multi-worker path-based resume now also skips a cached path route whose account is marked exhausted, attempts the same rollout path on another available worker, accepts only a replacement response for that rollout path, updates the route on success, and fails closed if no replacement worker can restore the context. Explicit thread-id resume now does the same bounded account-handoff attempt when the visible thread's cached worker account is exhausted, accepts only a response for the requested thread id, publishes account thread-handoff operator events, leaves the existing sticky route in place when no replacement restores the context, and has real `RemoteAppServerClient` coverage for both the successful replacement-account restoration path and the no-replacement fail-closed path, including a follow-up `thread/read` that stays on the replacement worker after success. |
| `thread/fork` | Fork an existing session | `policy interception` | `threadId`-based fork is covered by single-worker remote compatibility tests plus real multi-worker routing and same-session recovery regressions, including follow-up `thread/read` route backfill for the returned forked thread. Path-based fork is now allowed when the rollout path is already visible in the current tenant/project scope; real multi-worker compatibility coverage plus dedicated northbound JSON-RPC regressions cover the visible-path success path, and the real multi-worker same-session recovery harness now also verifies that visible-path fork stays sticky on a recovered worker after reconnect, while embedded and single-worker remote harnesses continue to pin the hidden-path rejection path. Multi-worker path-based fork now also shares the exhausted-account replacement behavior used by path-based resume, so a recoverable rollout path can move to another available account-backed worker without silently using the exhausted account. Explicit thread-id fork now also makes the same bounded replacement-account attempt when the visible source thread's cached worker account is exhausted, publishes account thread-handoff operator events, and has real `RemoteAppServerClient` coverage for both the successful replacement-account restoration path and the no-replacement fail-closed path. |
| `thread/list` | Session picker/history | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/list` on one shared northbound session, while multi-worker routing continues to deduplicate repeated thread ids across workers, select the newest visible thread metadata, and backfill sticky worker ownership from that winner. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/list` refresh re-adds a recovered worker into the shared visible-thread set and restores sticky ownership for its threads, and dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation plus sticky-route backfill on the shared client session. |
| `thread/loaded/list` | Discover currently loaded subagent threads | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests, including scope filtering. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/loaded/list` on one shared northbound session. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/loaded/list` refresh re-adds a recovered worker into the shared loaded-thread set and restores sticky ownership for its threads, dedicated northbound websocket reconnect coverage now also pins that recovered-worker aggregation plus sticky-route backfill on the shared client session, and the real multi-worker same-session recovery harness now also exercises that recovered `thread/loaded/list` path through `RemoteAppServerClient`. |
| `thread/read` | Read thread metadata/history | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/read` routing plus scope-filtered reads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/read` on the same session still routes to a recovered worker, including a follow-up read of a recovered-worker review thread materialized after reconnect. When the cached owner account is exhausted, visible `thread/read` now makes a bounded replacement-account attempt, accepts only the requested thread id, updates sticky routing on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the read; real multi-worker `RemoteAppServerClient` harnesses now cover both successful direct restoration and no-replacement fail-closed behavior. |
| `thread/name/set` | Rename thread | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/name/set`, including `thread/name/updated` notification fan-in plus follow-up `thread/read` that reflects the worker-local rename on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/name/set` on the same session still routes to a recovered worker and is reflected by a follow-up `thread/read`, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. When the cached owner account is exhausted, visible `thread/name/set` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the rename. |
| `thread/memoryMode/set` | Update memory mode | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/memoryMode/set` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/memoryMode/set` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. When the cached owner account is exhausted, visible `thread/memoryMode/set` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the memory-mode update. |
| `thread/unsubscribe` | Drop local subscription | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/unsubscribe` through the recovered worker. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/unsubscribe` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/unsubscribe` on the same session still routes to a recovered worker. If the owning account-backed worker is marked exhausted, unsubscribe fails closed instead of routing the thread-scoped subscription change to another account. |
| `thread/compact/start` | Compact thread history | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/compact/start` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/compact/start` on the same session still routes to a recovered worker. If the owning account-backed worker is marked exhausted, this active side-effecting request fails closed instead of replaying compaction on another account. |
| `thread/shellCommand` | Start shell command from thread context | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/shellCommand` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/shellCommand` on the same session still routes to a recovered worker. If the owning account-backed worker is marked exhausted, this active side-effecting request fails closed instead of starting shell work on another account. |
| `thread/backgroundTerminals/clean` | Clean background terminals | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/backgroundTerminals/clean` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/backgroundTerminals/clean` on the same session still routes to a recovered worker. If the owning account-backed worker is marked exhausted, background-terminal cleanup fails closed instead of issuing active cleanup on another account. |
| `thread/rollback` | Roll back thread turns | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/rollback` across worker-owned threads on one shared northbound session, and real multi-worker reconnect coverage verifies that a later sticky `thread/rollback` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/rollback` now makes a bounded replacement-account attempt, accepts only a response for the requested thread id, updates sticky routing on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the rollback; real multi-worker `RemoteAppServerClient` harnesses now cover both successful direct restoration and no-replacement fail-closed behavior. |
| `review/start` | Start review flow | `transparent passthrough` | Covered by embedded and single-worker remote compatibility tests, including follow-up `thread/read` of the returned detached review thread. Real single-worker remote reconnect coverage now also verifies that a later client session can still `review/start` plus follow-up `thread/read` of the detached review thread through the recovered worker. Real multi-worker remote compatibility tests now also cover steady-state sticky `review/start` on one shared northbound session, including follow-up `thread/read` of worker-owned review threads. Real multi-worker reconnect coverage now also verifies that a later sticky `review/start` on the same session still routes to a recovered worker and re-registers the returned review thread for follow-up `thread/read`, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, review start fails closed instead of creating detached review state on another account without an explicit restore surface. |
| `turn/start` | Start a turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests, including turn lifecycle notifications. The real embedded and single-worker remote harnesses now also cover plan-mode proposed-plan item lifecycle notifications through unmodified `RemoteAppServerClient` sessions. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/start` plus shared-session turn lifecycle fan-in across worker-owned threads, and now also cover plan-mode proposed-plan `item/started` / `item/completed` fan-in on one shared client session. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/start` on the same session still routes to a recovered worker, and that the recovered worker's turn lifecycle notifications still fan in on the shared northbound session. Dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, turn start fails closed and emits the active-thread handoff-failure path instead of replaying live turn work on another account. |
| `turn/interrupt` | Interrupt active turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/interrupt` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/interrupt` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, interrupt fails closed instead of attempting turn control on another account. |
| `turn/steer` | Steer active turn | `transparent passthrough` | Covered by real embedded and single-worker remote compatibility tests. Real multi-worker remote compatibility tests now also cover steady-state sticky `turn/steer` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `turn/steer` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, steer fails closed instead of attempting active turn mutation on another account. |

## Additional Thread-Control Requests

These lower-frequency requests are not central to the Stage A bootstrap flow,
but they already have dedicated passthrough or real-client gateway coverage.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/archive` | Archive a thread from history views | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/archive` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/archive` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/archive` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the archive. |
| `thread/unarchive` | Restore an archived thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/unarchive` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/unarchive` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/unarchive` now makes a bounded replacement-account attempt, accepts only the requested thread id, updates sticky routing on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the unarchive. |
| `thread/metadata/update` | Update thread metadata such as git/context info | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/metadata/update` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/metadata/update` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/metadata/update` now makes a bounded replacement-account attempt, accepts only the requested thread id, updates sticky routing on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the update. |
| `thread/turns/list` | Page thread turns for history/review surfaces | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real single-worker remote reconnect coverage now also verifies that a later client session can still `thread/turns/list` through the recovered worker, and real multi-worker reconnect coverage verifies that a later sticky `thread/turns/list` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/turns/list` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the history page. |
| `thread/increment_elicitation` | Increase elicitation counters on a thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/increment_elicitation` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/increment_elicitation` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the counter update. |
| `thread/decrement_elicitation` | Decrease elicitation counters on a thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/decrement_elicitation` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/decrement_elicitation` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the counter update. |
| `thread/inject_items` | Inject items into an existing thread | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded, single-worker remote, and multi-worker remote thread-control harnesses. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/inject_items` on the same session still routes to a recovered worker. When the cached owner account is exhausted, visible `thread/inject_items` now makes a bounded replacement-account attempt, updates sticky routing from the requested thread id on success, publishes account thread-handoff operator events, and fails closed without mutating the existing route when no replacement restores the injection. |

## Realtime Requests

These methods are used by the TUI realtime conversation path.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `thread/realtime/listVoices` | Discover available realtime voices before a session starts | `transparent passthrough` | Covered by dedicated northbound passthrough tests plus real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state aggregated `thread/realtime/listVoices` on one shared northbound session, deduplicating and preserving the merged default-voice selection across workers. Dedicated reconnect regression coverage now also verifies that a later aggregated `thread/realtime/listVoices` refresh re-adds a recovered worker into the shared voice inventory for the same session, and dedicated northbound websocket reconnect coverage now pins that recovered-worker aggregation on the shared client session. |
| `thread/realtime/start` | Start realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/start` across worker-owned threads on one shared northbound session, with shared-session fan-in for the resulting realtime notifications. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/start` on the same session still routes to a recovered worker, and that the recovered worker's realtime notifications still fan in on the shared northbound session. Dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, realtime start fails closed instead of starting a live session on another account. |
| `thread/realtime/appendAudio` | Stream microphone audio | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/appendAudio` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/appendAudio` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, audio append fails closed instead of continuing a realtime stream on another account. |
| `thread/realtime/appendText` | Stream text into realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/appendText` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/appendText` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, text append fails closed instead of continuing a realtime stream on another account. |
| `thread/realtime/stop` | Stop realtime session | `transparent passthrough` | Covered by dedicated gateway passthrough tests plus the real embedded and single-worker remote compatibility harnesses. Real multi-worker remote compatibility tests now also cover steady-state sticky `thread/realtime/stop` across worker-owned threads on one shared northbound session. Real multi-worker reconnect coverage now also verifies that a later sticky `thread/realtime/stop` on the same session still routes to a recovered worker, and dedicated northbound websocket reconnect coverage now also pins that recovered sticky route on the shared client session. If the owning account-backed worker is marked exhausted, realtime stop fails closed instead of issuing control for the active session on another account. |

## Legacy Client Compatibility Requests

These v1-shaped client requests remain part of the shared app-server protocol
surface even though they are outside the primary Stage A TUI bootstrap path.

| Method | Client use | Gateway status | Notes |
| --- | --- | --- | --- |
| `getConversationSummary` | Read a rollout summary by thread id or rollout path | `policy interception` | `conversationId` requests use the same thread visibility and sticky worker routing as other thread-scoped requests. When that cached worker account is exhausted, conversation-id summary requests now make a bounded replacement-account attempt, accept only a summary for the requested `conversationId`, register the returned `conversationId` plus `path` on success, publish account thread-handoff operator events, and fail closed without mutating the existing route when no replacement restores the summary. `rolloutPath` requests now use visible rollout paths as the scope key, route through the multi-worker path-discovery fallback used by path-based `thread/resume` / `thread/fork`, skip exhausted cached path routes when another available worker can restore the summary, accept only a replacement summary for that rollout path, and register the returned `conversationId` plus `path` for later sticky routing. Dedicated gateway regression coverage pins the visible-path multi-worker route and both conversation-id account-handoff branches, and the real multi-worker `RemoteAppServerClient` legacy harness now validates both rollout-path summary routing and follow-up `conversationId` sticky routing after a worker-owned thread registers that path. A real multi-worker legacy harness also now drives `account/rateLimits/read` to mark the cached rollout-path worker exhausted, then verifies `getConversationSummary.rolloutPath` and `getConversationSummary.conversationId` restore through a replacement account-backed worker; both forms now also have no-replacement real-client coverage that fails closed with the matching account handoff failure event when no replacement account has capacity. Dedicated regressions now also reject wrong-path replacement summaries without mutating the cached route. |
| `gitDiffToRemote` | Read git diff for a local checkout | `transparent passthrough` | In embedded and single-worker remote mode this remains ordinary passthrough. In multi-worker remote mode it now uses worker-discovery fallback instead of defaulting to the primary worker, so a cwd-local diff can be served by the worker that can compute it. The route requires all configured workers to be present and fails closed during reconnect backoff rather than returning a surviving worker's partial local view. Dedicated northbound WebSocket coverage now pins both steady-state first-successful-worker routing and lazy recovered-worker routing on one shared client session, and the real multi-worker `RemoteAppServerClient` legacy harness now validates the steady-state worker-discovery route. |
| `getAuthStatus` | Deprecated legacy auth status read, superseded by `account/read` | `transparent passthrough` | In embedded and single-worker remote mode this remains ordinary passthrough. In multi-worker remote mode it now mirrors the `account/read` aggregation shape: the primary worker's `authMethod` / `authToken` view is preserved while `requiresOpenaiAuth` is OR-ed across all configured workers, and degraded sessions fail closed while required workers remain unavailable. Dedicated aggregation, passthrough, reconnect, and reconnect-backoff regressions pin the legacy compatibility behavior, and the real multi-worker bootstrap harness now validates the aggregated legacy auth read alongside `account/read`. |

## Server Notifications

The current gateway forwards downstream server notifications back to the client
on the same WebSocket connection without method-specific translation.

Required notification behavior for Stage A:

- `thread/started`
- `thread/status/changed`
- `turn/started`
- `turn/completed`
- item delta/update notifications used during active turns

Current status:

- notification forwarding is `transparent passthrough`
- `thread/started`, `thread/status/changed`, `turn/started`, and
  `turn/completed` are covered by gateway compatibility tests
- `item/agentMessage/delta` is covered by a dedicated gateway compatibility
  test for active-turn streaming notifications
- `item/started` and `item/completed` are now covered by dedicated gateway
  compatibility tests for longer-running turn item lifecycle notifications
- `hook/started` and `hook/completed` are now covered by dedicated gateway
  compatibility tests for hook lifecycle notifications
- the real single-worker remote compatibility harness now also covers
  `item/started`, `item/completed`, `hook/started`, and `hook/completed`
  during a full `turn/start` workflow
- `item/autoApprovalReview/started` and `item/autoApprovalReview/completed`
  are now covered by dedicated gateway
  compatibility tests for approval auto-review lifecycle notifications, and
  by the real single-worker remote turn workflow harness through an
  unmodified `RemoteAppServerClient` session
- `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, and
  `item/fileChange/outputDelta` are now covered
  by real single-worker remote and multi-worker remote turn-workflow
  compatibility tests
- `item/plan/delta`, `item/reasoning/summaryPartAdded`,
  `item/commandExecution/terminalInteraction`, `turn/diff/updated`,
  `turn/plan/updated`, `thread/tokenUsage/updated`,
  `item/mcpToolCall/progress`, `thread/compacted`, `model/rerouted`, and
  `rawResponseItem/completed` are now also covered by the real single-worker
  remote turn-workflow harness, so the release-quality remote baseline covers
  the same lower-frequency forwarding paths as the broad multi-worker Stage B
  harness
- the real embedded turn-workflow harness now also covers
  `item/reasoning/summaryPartAdded` from Responses summary-part events and
  `thread/tokenUsage/updated` from a token-bearing Responses completion, so
  the in-process release-quality baseline exercises those low-frequency
  notification forwarding paths through an unmodified `RemoteAppServerClient`
  session
- the real embedded plan-mode harness now also covers `item/plan/delta`
  between the proposed-plan item start and completion notifications, so
  streamed plan text is pinned in the in-process release-quality baseline
- dedicated gateway notification coverage now also includes
  `error`, `thread/archived`, `thread/unarchived`, `thread/closed`,
  `rawResponseItem/completed`, `plan/delta`, `reasoning/summaryTextDelta`,
  `reasoning/summaryPartAdded`, `reasoning/textDelta`,
  `terminalInteraction`, `commandExecution/outputDelta`,
  `fileChange/outputDelta`, `turn/diffUpdated`, `turn/planUpdated`,
  `thread/name/updated`, `thread/tokenUsage/updated`,
  `mcpToolCall/progress`, `contextCompacted`, and `model/rerouted`
- real multi-worker thread-control coverage now also observes
  `thread/closed`, `thread/archived`, and `thread/unarchived` from both
  worker-owned visible threads on one shared `RemoteAppServerClient` session,
  so those lifecycle notifications are covered by Stage B client traffic as
  well as targeted gateway notification fixtures
- real multi-worker thread-mutation coverage now also observes
  `thread/name/updated` from both worker-owned visible threads after sticky
  `thread/name/set` routing, so steady-state rename notification fan-in is
  covered by Stage B client traffic as well as targeted gateway notification
  fixtures and recovered-worker coverage
- real multi-worker same-session recovery coverage now also observes those
  thread lifecycle notifications from a lazily re-added worker on the shared
  client session, so recovered-worker thread state fan-in is covered by the
  broad Stage B harness too
- that same same-session recovery coverage now also observes
  `thread/name/updated` after sticky `thread/name/set` routing back to the
  lazily re-added worker, so recovered-worker rename fan-in is covered by real
  client traffic
- dedicated gateway connection-notification coverage now also includes
  `fuzzyFileSearch/sessionUpdated` and `fuzzyFileSearch/sessionCompleted`, so
  streaming file-search sessions are pinned alongside their request surface;
  real multi-worker steady-state and same-session recovery coverage now also
  observe both notifications through the shared northbound client session
- `thread/realtime/started` is covered by a dedicated gateway compatibility
  test for experimental realtime notification forwarding
- additional experimental realtime notifications now also have dedicated
  gateway compatibility coverage for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- the real embedded compatibility harness now also covers realtime
  notification forwarding for `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed` during a full
  sideband-backed realtime workflow
- dedicated gateway connection-notification coverage now also includes
  `command/exec/outputDelta`, so streamed standalone command output is pinned at
  the northbound transport boundary too
- multi-worker northbound v2 regression coverage now also verifies fan-in for
  experimental realtime notifications on one shared client session, now
  covering the full current set:
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`,
  `thread/realtime/outputAudio/delta`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- the real multi-worker remote compatibility harness now also covers
  `item/started`, `item/completed`, `hook/started`, and `hook/completed`
  fan-in during worker-owned `turn/start` workflows
- lower-frequency user-visible notifications now also have dedicated gateway
  compatibility coverage for `warning`, `configWarning`,
  `deprecationNotice`, and `mcpServer/startupStatus/updated`
- the real embedded compatibility harness now also covers `configWarning`
  during startup, and the real single-worker remote compatibility harness now
  also covers `warning`, `configWarning`, `deprecationNotice`,
  `account/login/completed`, `mcpServer/oauthLogin/completed`, and
  `mcpServer/startupStatus/updated`, so those visible notification paths are
  exercised through unmodified `RemoteAppServerClient` sessions in addition to
  dedicated northbound coverage
- the real embedded and single-worker remote compatibility harnesses now also
  cover `thread/name/updated` during the `thread/name/set` rename flow, so the
  visible rename notification path is exercised through unmodified
  `RemoteAppServerClient` sessions in addition to dedicated northbound
  passthrough coverage
- dedicated gateway connection-notification coverage now also includes
  `account/login/completed` for onboarding auth completion flows
- dedicated gateway connection-notification coverage now also includes
  `mcpServer/oauthLogin/completed` for MCP OAuth completion status
- exact-duplicate suppression for multi-worker connection-state notifications
  now also covers `account/login/completed`, so one shared northbound session
  does not surface duplicate onboarding-complete notifications when a fanout
  login flow completes on more than one worker
- that same exact-duplicate suppression now also covers
  `mcpServer/oauthLogin/completed`, so one shared northbound session does not
  surface duplicate MCP OAuth completion notifications when more than one
  worker emits the same payload
- the real multi-worker connection-state harnesses now also verify that
  `mcpServer/oauthLogin/completed` is emitted exactly once both in steady
  state and after worker reconnect, so that dedupe path is exercised through
  an unmodified `RemoteAppServerClient` session in addition to dedicated
  northbound regressions
- that same exact-duplicate suppression now also covers connection-scoped
  `warning`, `configWarning`, and `deprecationNotice`, so one shared
  northbound session does not surface duplicate visible notices when more than
  one worker emits the same payload
- dedicated northbound v2 regression coverage now also pins the steady-state
  exact-duplicate suppression path for `account/rateLimits/updated` and
  `app/list/updated` alongside `account/updated`, so the core connection-state
  dedupe set is covered directly at the gateway boundary
- dedicated northbound v2 notification forwarding coverage now also includes
  the full core connection-state set of `account/updated`,
  `account/rateLimits/updated`, and `app/list/updated`, so single-worker
  passthrough and multi-worker dedupe coverage exercise the same notification
  family
- dedicated gateway connection-notification coverage now also includes
  `externalAgentConfig/import/completed`, so fanout imports do not surface
  duplicate completion notifications on one shared multi-worker session
- dedicated gateway connection-notification coverage now also includes
  exact-duplicate suppression for `windows/worldWritableWarning` and
  `windowsSandbox/setupCompleted`, so platform setup notices stay
  single-delivery on one shared multi-worker session
- dedicated gateway connection-notification coverage now also includes
  `fs/changed`, so filesystem watch change delivery is pinned at the same
  northbound boundary as `fs/watch` and `fs/unwatch`
- the real single-worker remote compatibility harness now also observes
  `fs/changed` after `fs/watch`, so filesystem watch notification delivery is
  covered by release-quality remote client traffic as well as targeted
  northbound fixtures
- the real single-worker remote reconnect harness now also observes recovered-
  worker `fs/changed` delivery after `fs/watch`, so the release-quality remote
  baseline covers filesystem watch notifications across ordinary worker
  recovery
- that same single-worker remote reconnect harness now also observes
  recovered-worker `windowsSandbox/setupCompleted` delivery after
  `windowsSandbox/setupStart`, so the release-quality remote baseline covers
  Windows sandbox setup notifications across ordinary worker recovery
- that same single-worker remote reconnect harness now also observes
  recovered-worker `fuzzyFileSearch/sessionUpdated` and
  `fuzzyFileSearch/sessionCompleted` delivery after replaying a streaming
  fuzzy file search session, so file-picker notifications are covered across
  ordinary worker recovery
- dedicated gateway connection-notification coverage now also verifies
  multi-worker `fs/changed` fan-in, so one shared northbound session receives
  worker-local filesystem change notifications from more than one downstream
  app-server session
- dedicated reconnect coverage now also verifies `fs/changed` fan-in from a
  lazily re-added worker after shared `fs/watch`, so filesystem watch
  notifications remain covered across worker loss and same-session recovery
- dedicated reconnect coverage now also verifies
  `mcpServer/startupStatus/updated` delivery from a lazily re-added worker
  after `mcpServerStatus/list`, so recovered-worker MCP startup state remains
  visible on one shared northbound session; the real multi-worker
  same-session bootstrap recovery harness now also observes that notification
  through an unmodified `RemoteAppServerClient` session
- the real multi-worker same-session bootstrap recovery harness now also
  observes recovered-worker `account/updated`, `account/rateLimits/updated`,
  `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`, and
  `windows/worldWritableWarning` delivery after the corresponding
  connection-scoped discovery refreshes, so core connection-state and visible
  notice fan-in are covered by real Stage B client traffic as well as targeted
  northbound regressions
- real connection-state notification harnesses now also observe
  `windowsSandbox/setupCompleted` through unmodified `RemoteAppServerClient`
  sessions across steady-state, reconnect, duplicate-suppression, and
  same-session recovered-worker delivery paths
- dedicated reconnect-backoff coverage now also verifies that
  `mcpServer/oauth/login` fails closed while a worker-discovery route is still
  unavailable, so OAuth login routing does not proceed from incomplete MCP
  inventory during retry backoff, and asserts the fail-closed metric is tagged
  with active reconnect backoff
- exact-duplicate connection-state notification suppression now remembers a
  bounded history of previously forwarded payloads and source workers per
  method, so interleaved distinct payloads cannot immediately make an older
  cross-worker duplicate eligible for delivery again on one shared multi-worker
  session while same-worker repeats remain deliverable and dedupe memory
  remains capped; same-worker repeats refresh their history position without
  being suppressed, and dedicated coverage now pins that interleaved duplicate
  path for `account/updated`, `account/rateLimits/updated`,
  `account/login/completed`, `app/list/updated`, `warning`, `configWarning`,
  `deprecationNotice`, `mcpServer/oauthLogin/completed`,
  `mcpServer/startupStatus/updated`, `windows/worldWritableWarning`, and
  `windowsSandbox/setupCompleted` notifications, same-worker repeat delivery
  across the same broader connection-state notification set,
  refreshed-history behavior, empty-payload import-completion repeats, and the
  per-method history bound, including a negative metrics assertion that
  same-worker repeats do not emit `gateway_v2_suppressed_notifications`
- duplicate `skills/changed` suppression now remains pending when an aggregated
  `skills/list` refresh fails, so a failed refresh attempt cannot reopen
  duplicate invalidation delivery before the client receives a successful
  refresh response
- dedicated websocket coverage now also verifies that a later successful
  `skills/list` refresh on the same shared session reopens fresh
  `skills/changed` delivery after an earlier failed refresh left duplicate
  invalidations suppressed
- multi-worker `model/list` aggregation now also supports gateway-owned
  pagination over merged worker inventories, including downstream worker-local
  page draining and stable `model-offset:` cursors at the northbound boundary
- dedicated gateway notification coverage now also includes the thread-scoped
  `error` turn-failure notification and internal
  `rawResponseItem/completed` completion notification, so failure signals and
  raw response item replay are pinned by the same visible-thread forwarding
  path as lifecycle and item-progress events
- the real multi-worker turn-routing harness now also observes
  `item/plan/delta`, `item/reasoning/summaryPartAdded`,
  `item/commandExecution/terminalInteraction`, `turn/diff/updated`,
  `turn/plan/updated`, `thread/tokenUsage/updated`,
  `item/mcpToolCall/progress`, `thread/compacted`, `model/rerouted`, and
  `rawResponseItem/completed` from both worker-owned visible threads on one
  shared `RemoteAppServerClient` session, so those lower-frequency turn
  forwarding paths are covered by real multi-worker Stage B client traffic in
  addition to targeted northbound passthrough fixtures
- the real multi-worker same-session recovery harness now also verifies that
  this lower-frequency turn notification set still fans in from a lazily
  re-added worker after reconnect, so recovered-worker notification delivery
  is covered beyond the basic lifecycle and streaming-delta path
- the real multi-worker turn-routing and same-session recovery harnesses now
  also observe `item/autoApprovalReview/started` and
  `item/autoApprovalReview/completed` through unmodified
  `RemoteAppServerClient` sessions, so guardian approval auto-review
  notification forwarding is covered by broad Stage B client traffic as well
  as targeted northbound fixtures
- the real single-worker remote turn workflow harness now also observes those
  guardian approval auto-review notifications, so the release-quality remote
  baseline covers the same forwarding path through real client traffic
- those same real multi-worker harnesses now also observe turn-scoped `error`
  notifications from worker-owned visible threads, so this opportunistic
  warning/error surface is covered by broad Stage B client traffic in addition
  to targeted northbound fixtures
- the real single-worker remote turn workflow harness now also observes
  turn-scoped `error` notifications, so the release-quality remote baseline
  covers this opportunistic warning/error surface through real client traffic
- the real multi-worker remote harness now also exercises plan-mode
  proposed-plan `item/started` and `item/completed` notifications on a worker-owned
  thread, so proposed-plan lifecycle fan-in is covered by broad Stage B client
  traffic as well as the embedded and single-worker release-quality baselines
- the real single-worker remote turn workflow harness now also observes the
  lower-frequency turn notification set already covered by the multi-worker
  Stage B harness, so those forwarding paths are pinned through real
  release-quality remote client traffic as well as targeted northbound
  fixtures
- the real embedded turn workflow harness now also observes
  `item/reasoning/summaryPartAdded` from Responses summary-part events and
  `thread/tokenUsage/updated` from a token-bearing model completion, so those
  low-frequency notification forwarding paths are pinned in the in-process
  release-quality baseline as well as the remote baseline
- the real embedded plan-mode harness now also observes `item/plan/delta`
  before the proposed-plan item completes, so streamed plan text is part of
  the in-process release-quality baseline rather than only targeted or remote
  coverage
- the real embedded compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- the real single-worker remote compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- the real single-worker remote compatibility harness now also covers
  `account/login/completed` notification delivery during the onboarding auth
  flow
- the real multi-worker remote compatibility harness now also covers
  `account/login/completed` during the external-auth onboarding flow
- real multi-worker same-session recovery coverage now also verifies managed
  `account/login/completed` delivery after the recovered primary worker is
  re-added to one shared client session
- dedicated northbound multi-worker websocket regression coverage now also
  verifies that a managed `account/login/start` can reconnect a missing
  primary worker and still forward that recovered worker's resulting
  `account/login/completed` notification over the shared northbound session
- broader notification hardening is still deferred to parity validation and
  rollout work, but the current TUI-facing realtime notification set now has
  dedicated gateway coverage
- forwarded downstream notifications now emit
  `gateway_v2_forwarded_notifications{method}` after successful northbound
  delivery, with visible-forwarding, multi-worker fan-in count coverage, plus
  opt-out and hidden-thread negative coverage so delivered fan-in has a metric
  counterpart to duplicate, opt-out, pending-refresh, and hidden-thread
  suppression counters
- failed downstream notification delivery now emits
  `gateway_v2_notification_send_failures{method,outcome}`, so failed fan-in
  can be attributed by notification method as well as terminal connection
  outcome
- failed notification delivery now also has structured warning logs with
  tenant/project scope, worker route, method, outcome, and error detail
- downstream server requests that pass gateway policy but fail during
  northbound delivery now emit
  `event=downstream_server_request_forward_delivery_failed` on the
  server-request lifecycle counter, plus structured warning logs with
  tenant/project scope, worker route, method, outcome, and error detail
- those downstream server-request forward delivery failures now also emit
  `gateway_v2_server_request_forward_send_failures{method,outcome}`, so
  prompt / elicitation delivery failures have a direct rollout counter in
  addition to the broader lifecycle event stream
- client answers to forwarded server requests that fail during downstream
  delivery now also emit
  `gateway_v2_server_request_answer_delivery_failures{response_kind}`, so
  answered-but-not-delivered prompt / elicitation replies have a direct rollout
  counter in addition to the broader lifecycle event stream
- gateway-owned server-request rejections that fail during downstream delivery
  now also emit
  `gateway_v2_server_request_rejection_delivery_failures{method}`, so prompt /
  elicitation rejections that never reach the owning worker have a direct
  rollout counter in addition to the broader lifecycle event stream
- client-side cleanup rejections for still-pending server requests now also
  emit
  `gateway_v2_server_request_rejection_delivery_failures{method}` with the
  original server-request method when the downstream handoff fails or the
  worker route is already unavailable, so teardown-time cleanup failures and
  skips are visible through the same direct rejection-delivery counter and can
  still be split by prompt family
- gateway client request responses that fail during northbound delivery now
  emit structured warning logs with tenant/project scope, JSON-RPC request id,
  method, outcome, and error detail, matching the existing per-request outcome
  metric for that response-path failure
- gateway client request response delivery failures now also emit
  `gateway_v2_client_response_send_failures{method,outcome}` for initialize
  handshake responses, ordinary request responses, and background
  `command/exec` completions, giving response-path send failures a direct
  counter instead of relying only on terminal connection or per-request outcome
  metrics
- ordinary success and error client-response send failures now also have
  dedicated northbound WebSocket regression coverage, so the direct failure
  counter is exercised at the compatibility boundary as well as by
  observability helper tests
- gateway-owned WebSocket close frame delivery failures now emit
  `gateway_v2_close_frame_send_failures{code,outcome}`, so terminal
  policy/protocol close failures are measurable separately from response,
  notification, and server-request delivery failures, and now also emit
  structured warning logs with tenant/project scope, the close code,
  protocol-safe reason, outcome, and send error detail
- gateway-owned close frame delivery failures now also have dedicated
  northbound WebSocket regression coverage for the direct counter and
  structured warning log fields, so the terminal close path is pinned at the
  compatibility boundary instead of only by helper-level observability tests
- synthesized worker-cleanup `serverRequest/resolved` delivery failures now
  also emit
  `gateway_v2_notification_send_failures{method="serverRequest/resolved",outcome}`
  plus
  `gateway_v2_server_request_lifecycle_events{event="worker_cleanup_resolution_send_failed",method}`
  with the original server-request method; successful synthesized-resolution
  delivery lifecycle events use that same original prompt method tag, while
  the direct notification send-failure metric remains tagged as
  `method="serverRequest/resolved"` because the emitted northbound notification
  is still `serverRequest/resolved`

## Server Requests

These are the server-initiated requests the current TUI explicitly handles.

| Method | Client handling status | Gateway status | Notes |
| --- | --- | --- | --- |
| `item/commandExecution/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/fileChange/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/permissions/requestApproval` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this approval path without leaking stale worker-local request IDs. |
| `item/tool/requestUserInput` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests and by the real multi-worker remote `RemoteAppServerClient` harness, which verifies per-worker request-id translation on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this request/response path without leaking stale worker-local request IDs. |
| `mcpServer/elicitation/request` | Supported by current TUI | `transparent passthrough` | Covered by single-worker remote round-trip tests, including the real `RemoteAppServerClient` northbound harness in both steady state and post-reconnect recovery, plus the real multi-worker remote harness for translated per-worker request IDs. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker resumes this elicitation path without leaking stale worker-local request IDs. |
| `item/tool/call` | TUI marks unsupported today | `transparent passthrough` | This is a client capability gap, not a gateway transport gap; dedicated northbound passthrough coverage now verifies the request/response round trip, the real embedded and single-worker remote compatibility harnesses now also exercise that same transport path over the northbound client connection, with the embedded harness verifying `serverRequest/resolved` plus `item/started` / `item/completed` forwarding for the dynamic-tool call lifecycle in steady state, the single-worker harness verifying that same lifecycle ordering in steady state, the single-worker reconnect harness verifying the same ordering after worker recovery, and the real multi-worker remote harness verifying translated per-worker request IDs on one shared northbound session. |
| `account/chatgptAuthTokens/refresh` | TUI accepts | `transparent passthrough` | Covered by dedicated northbound server-request round-trip tests plus single-worker and multi-worker remote round-trip tests, with the real single-worker `RemoteAppServerClient` harness now covering both steady-state and post-reconnect recovery, and the multi-worker harness covering translated per-worker request IDs on one shared northbound session. Real multi-worker same-session recovery coverage now also verifies that a lazily re-added worker can resume this connection-scoped refresh round trip without leaking stale worker-local request IDs, and real multi-worker disconnect coverage now also verifies that the shared session fails closed both while a refresh prompt is still pending and while the client response is still awaiting downstream `serverRequest/resolved`. Embedded mode currently relies on targeted northbound coverage for this path because the in-process downstream app-server client rejects ChatGPT token-refresh prompts before they can surface through the real embedded compatibility harness. |
| legacy `apply_patch` / `exec_command` approvals | TUI marks unsupported today | `transparent passthrough` | These are not part of the primary Stage A drop-in target. Dedicated northbound websocket regression coverage now verifies round trips for legacy `applyPatchApproval` and `execCommandApproval`, and scope-enforcement unit coverage now also pins that their `conversationId` is treated as thread ownership so legacy prompts do not bypass gateway visibility checks. |

Additional transport behavior:

- real multi-worker same-session recovery coverage now also verifies that after
  a dropped worker is lazily re-added on one shared client session, translated
  thread-scoped approval / elicitation requests plus the connection-scoped
  `account/chatgptAuthTokens/refresh` flow still round trip through that
  worker correctly
- when one shared northbound client session has already completed the v2
  handshake, any later lazily re-added downstream worker now also receives the
  forwarded `initialized` notification before subsequent requests route there,
  so reconnect recovery restores full post-handshake app-server session state
- dedicated northbound multi-worker websocket regression coverage now also
  verifies that a later client request can reconnect a missing worker and then
  still forward subsequent server requests from that recovered worker over the
  same shared northbound session
- thread-scoped server-request answers are also guarded by the live
  active-context fail-closed policy: if the pending request's owning worker
  account is marked exhausted before the client answers, the gateway now
  rejects the answer locally, emits
  `gateway_v2_account_capacity_events{event="active_thread_handoff_failure"}`,
  publishes `gateway/accountActiveThreadHandoffFailed`, and does not deliver
  the approval, user-input, or elicitation answer to the exhausted
  account-backed worker. Dedicated northbound WebSocket regression coverage now
  pins that behavior on the real shared client session, including the absence
  of downstream delivery and the matching server-request lifecycle metrics.
- `/healthz.v2Connections` now also groups active and last-completed
  server-request backlog by owning worker id, splitting pending prompts from
  answered-but-unresolved prompts so Stage B rollout evidence can identify
  which downstream session is accumulating unresolved server-request state
  without exposing individual request ids.

- if a northbound v2 connection ends while a forwarded server request is still
  pending, including client disconnects and malformed-payload protocol closes,
  plus other terminal northbound I/O failures such as client-send timeouts or
  broken pipes,
  the gateway now rejects that pending request back to the downstream
  app-server session with a gateway-owned internal error instead of leaving it
  unresolved
- that teardown path now also emits structured warning logs with the
  connection outcome, terminal detail, pending gateway request ids, per-scope
  counts, and affected worker ids, so operators can distinguish disconnect-
  driven cleanup from downstream request failures
- if a northbound client sends a `JSONRPCResponse` or `JSONRPCError` for a
  server request that is no longer pending, the gateway now treats that as a
  protocol violation, closes the northbound WebSocket, and rejects any other
  still-pending downstream server requests instead of silently ignoring the
  out-of-order reply
- dedicated northbound regression coverage now also pins the scope and
  pending-request-id warning fields for both response and error variants of
  that protocol-violation path before the socket is closed
- dedicated northbound regression coverage now also pins those same teardown
  warning fields for malformed post-handshake client payloads, so invalid
  JSON-RPC frames do not rely only on close-frame assertions
- that same malformed-payload teardown coverage now also pins any answered-
  but-unresolved server-request routes after the client has already replied but
  downstream `serverRequest/resolved` has not arrived yet
- client server-request replies now also emit lifecycle events for downstream
  delivery success or failure after the gateway receives the answer, so
  partially completed prompt lifecycles can distinguish gateway receipt from
  successful app-server handoff; direct regression coverage now pins the
  delivery-failure branch when the owning worker route is no longer available
- real northbound WebSocket coverage now also verifies that a rejected
  server-request reply can continue through downstream
  `serverRequest/resolved`, so error replies are pinned across forwarded,
  answered, delivered, and resolved lifecycle stages
- delivery failures for answered server requests now also emit structured
  warning logs with tenant/project scope, response kind, worker id, worker
  websocket URL, gateway request id, downstream request id, thread id, and
  error detail
- gateway-owned downstream server-request rejections now also record whether
  the rejection was delivered back to the owning worker, and delivery failures
  emit structured route logs so rejected prompts do not disappear between the
  gateway boundary and app-server
- dedicated northbound regression coverage now also pins the answered-but-
  unresolved variant of that protocol-violation path for both response and
  error frames, so already-answered downstream routes still appear in teardown
  logs before the connection closes
- if a downstream app-server session reuses a still-pending server-request id,
  the gateway now closes the northbound WebSocket with a gateway-owned error
  close and rejects the older pending request instead of silently overwriting
  the original route
- that collision path now also emits a structured warning log with the scope,
  worker id, colliding request id/method, and the gateway/downstream request
  ids, affected thread ids, and worker ids already pending on the connection
- real northbound WebSocket coverage now also pins lifecycle metrics for
  duplicate downstream `serverRequest/resolved` replays after gateway request-id
  translation, including both the first resolved event and the dropped replay
  event on one translated server-request route
- if one downstream worker disconnects during a multi-worker session while a
  thread-scoped server request from that worker is still pending or waiting on
  downstream `serverRequest/resolved`, the gateway now synthesizes
  `serverRequest/resolved` northbound so the client can dismiss that prompt
  instead of leaving it stranded on the shared session
- if one downstream worker disconnects during a multi-worker session while a
  connection-scoped server request from that worker is still pending or
  awaiting downstream `serverRequest/resolved`, the gateway now fails closed
  and ends the northbound WebSocket instead of silently dropping the
  unresolved prompt
- dedicated northbound regression coverage now also exercises the thread-scoped
  answered-but-unresolved variant of that worker-disconnect path, verifying
  that `serverRequest/resolved` is still synthesized after the client has
  already replied but before the worker can confirm resolution

## Topology and Policy Gaps

These are the known gaps that remain after the current Stage A coverage work.

| Area | Status | Notes |
| --- | --- | --- |
| remote runtime with more than one worker | `partial` | Northbound v2 WebSocket upgrades are now admitted in multi-worker remote mode, with one downstream session per worker plus aggregated `thread/list` / `thread/loaded/list`, deduplicated multi-worker `thread/list` results that keep the newest visible snapshot per thread id, aggregated bootstrap/setup discovery for `account/read`, deprecated `getAuthStatus`, `model/list`, `externalAgentConfig/detect`, `app/list`, `skills/list`, `mcpServerStatus/list`, and threadless `plugin/list`, translated server-request IDs across workers for `item/tool/requestUserInput`, `item/commandExecution/requestApproval`, `item/fileChange/requestApproval`, `item/permissions/requestApproval`, `mcpServer/elicitation/request`, and `account/chatgptAuthTokens/refresh`, including a dedicated real-client regression where two workers keep the same downstream request id pending at once for `item/tool/requestUserInput`, `item/permissions/requestApproval`, `mcpServer/elicitation/request`, and `account/chatgptAuthTokens/refresh` and the gateway still assigns distinct northbound ids, sticky thread routing, path-discovery routing for path-based `thread/resume` / `thread/fork` plus legacy `getConversationSummary.rolloutPath`, worker-discovery routing for `plugin/read` / `plugin/install` / `plugin/uninstall` / `mcpServer/oauth/login` / `gitDiffToRemote`, fanout for connection-scoped setup mutations such as `account/login/start` for external-auth `apiKey` and `chatgptAuthTokens`, `externalAgentConfig/import`, `marketplace/add`, `skills/config/write`, `experimentalFeature/enablement/set`, `config/mcpServer/reload`, `config/batchWrite`, `memory/reset`, `account/logout`, `fs/watch`, and `fs/unwatch`, primary-worker routing for one-shot account nudges such as `account/sendAddCreditsNudgeEmail`, deduplicated `skills/changed` invalidation notifications until the client refreshes with `skills/list`, exact-duplicate suppression for connection-state notifications such as `account/updated`, `account/rateLimits/updated`, `account/login/completed`, `app/list/updated`, `warning`, `configWarning`, `deprecationNotice`, `mcpServer/startupStatus/updated`, `mcpServer/oauthLogin/completed`, and `externalAgentConfig/import/completed`, synthesized `serverRequest/resolved` for thread-scoped prompts stranded by a worker disconnect, route backfill for already visible threads discovered through aggregated `thread/list` / `thread/loaded/list` plus lazy route recovery for later thread-scoped requests when that worker mapping is missing, in-session survival when one downstream worker disconnects and the remaining workers can still serve the shared northbound connection, lazy re-add of a recovered worker on later client requests so the same northbound session can resume routing work back onto that worker, and a short per-worker reconnect retry backoff so repeated worker failures do not trigger an immediate reconnect attempt on every later client request. Thread-scoped requests pinned to an exhausted account now fail closed with an explicit `gateway/accountActiveThreadHandoffFailed` operator event plus account-capacity metric instead of silently moving active context without a protocol-visible resume. Connection-scoped fanout mutations now also fail closed while any worker remains unavailable during retry backoff, instead of silently drifting shared setup or watch state onto only the surviving subset. Connection-scoped aggregated discovery requests such as `account/read`, deprecated `getAuthStatus`, `account/rateLimits/read`, `model/list`, threadless `app/list`, `mcpServerStatus/list`, `externalAgentConfig/detect`, `skills/list`, `experimentalFeature/list`, `collaborationMode/list`, threadless `plugin/list`, `thread/realtime/listVoices`, and one-shot `fuzzyFileSearch` now also fail closed while required workers remain unavailable, instead of returning partial inventory from the surviving worker subset. Threadless `config/read` now also fails closed while the primary worker remains unavailable during retry backoff, instead of silently falling through to a surviving secondary worker's config. Cwd-aware `config/read` now also fails closed while any worker remains unavailable during retry backoff, instead of silently falling back to a surviving worker whose config layers do not match the requested path. Worker-discovery plugin, MCP OAuth, and legacy git-diff requests now also fail closed while any worker remains unavailable during retry backoff, instead of selecting from an incomplete worker set. Primary-worker-only requests now also fail closed while that worker remains unavailable during retry backoff, instead of silently falling through to a surviving secondary worker; method-family coverage now pins that behavior for config requirements, managed login, login cancellation, add-credits nudge email, feedback upload, standalone command control, basic filesystem operations, and streaming fuzzy file-search sessions. That same degraded-session path now keeps multi-worker semantics active while only one worker is temporarily live, so connection-scoped fanout, dedupe, and gateway-owned server-request id translation do not silently collapse into single-worker handling before missing workers reconnect. Later refreshes of aggregated `account/read`, deprecated `getAuthStatus`, `account/rateLimits/read`, `model/list`, threadless `config/read`, aggregated `thread/list`, aggregated `thread/loaded/list`, `plugin/read`, `plugin/install`, `plugin/uninstall`, `mcpServer/oauth/login`, `gitDiffToRemote`, legacy rollout-path `getConversationSummary`, `externalAgentConfig/detect`, `externalAgentConfig/import`, `experimentalFeature/list`, `collaborationMode/list`, `skills/list`, `app/list`, `mcpServerStatus/list`, threadless `plugin/list`, one-shot `fuzzyFileSearch`, `marketplace/add`, `skills/config/write`, `experimentalFeature/enablement/set`, `config/mcpServer/reload`, `config/batchWrite`, `config/value/write`, `memory/reset`, `account/logout`, `fs/watch`, and `fs/unwatch` can all now re-add a recovered worker. Broad steady-state and reconnect validation now exists for thread, turn, realtime, approval, setup, recovery, and legacy compatibility flows, but broader parity and rollout hardening are still pending before this matches embedded or single-worker remote support. |
| multi-worker account-capacity health signal | `complete` | `/healthz.v2Connections.accountCapacityEventCounts` exposes cumulative v2 account-capacity event totals by event name, `accountCapacityEventWorkerCounts` breaks those totals down per worker, and `lastAccountCapacityEvent`, `lastAccountCapacityEventWorkerId`, `lastAccountCapacityEventTenantId`, `lastAccountCapacityEventProjectId`, `lastAccountCapacityEventReason`, and `lastAccountCapacityEventAt` record the newest event, affected worker, gateway tenant/project scope, reason, and timestamp, so operators can poll for quota exhaustion, repeated worker-specific handoffs, bounded account-handoff success, and fail-closed no-handoff decisions without relying only on metrics export or the live `/v1/events` stream. |
| multi-worker account-label rollout signal | `complete` | Remote multi-worker startup records account-label state in three operator-visible places: structured startup logs report account-label completeness and unlabeled-worker count, incomplete-label warning logs identify the unlabeled worker ids and WebSocket URLs, `/healthz.remoteAccountLabelsComplete` plus `remoteUnlabeledAccountWorkerCount`, `remoteUnlabeledAccountWorkerIds`, and `remoteUnlabeledAccountWorkers` expose the same state in health snapshots, and `gateway_remote_account_label_events{event="labeled"|"unlabeled",worker_id=...}` metrics let dashboards alert on unlabeled account-backed routes while distinguishing a fully labeled pool from missing metric data. The real remote multi-worker health regression now pins that `/healthz` reports both unlabeled worker routes when a two-worker validation profile starts without account labels, and direct runtime health coverage pins the fully labeled multi-worker branch as complete with count zero and empty unlabeled-route lists. |
| worker reconnect health mirror | `complete` | `gateway_v2_worker_reconnects` is now mirrored into `/healthz.v2Connections.workerReconnectEventCounts`, `workerReconnectEventWorkerCounts`, `lastWorkerReconnectEvent`, `lastWorkerReconnectEventWorkerId`, and `lastWorkerReconnectEventAt`, so degraded-topology rollout evidence can correlate reconnect attempt, success, failure, replay-failure, and backoff-suppression metrics with the worker ids visible in health snapshots. |
| connection-outcome health mirror | `complete` | `gateway_v2_connections{outcome}` is now mirrored into `/healthz.v2Connections.connectionOutcomeCounts`, while `lastConnectionDurationMs` and `maxConnectionDurationMs` mirror recent and peak `gateway_v2_connection_duration` evidence, so operators can reconcile terminal connection outcomes and duration outliers from health snapshots as well as metrics and audit logs. Real embedded, single-worker remote, and multi-worker remote healthz regressions now pin `client_closed` teardown snapshots, and the real single-worker slow-client timeout harness pins the `client_send_timed_out` outcome alongside terminal backlog diagnostics. |
| request-outcome health mirror | `complete` | `gateway_v2_requests{method,outcome}` is now mirrored into `/healthz.v2Connections.requestCounts`, `lastRequestMethod`, `lastRequestOutcome`, `lastRequestDurationMs`, `maxRequestDurationMs`, and `lastRequestAt`, so operators can reconcile ordinary success, policy, quota, protocol-violation, internal-error request outcomes, and recent / peak request latency from health snapshots as well as metrics and audit logs. |
| request-rejection health mirror | `complete` | `gateway_v2_client_request_rejections` and `gateway_v2_server_request_rejections` are now mirrored into `/healthz.v2Connections.clientRequestRejectionCounts`, `lastClientRequestRejectionMethod`, `lastClientRequestRejectionReason`, `lastClientRequestRejectionAt`, `serverRequestRejectionCounts`, `lastServerRequestRejectionMethod`, `lastServerRequestRejectionReason`, and `lastServerRequestRejectionAt`, so overload and scope-policy rejection evidence is visible in health snapshots as well as metrics and logs. |
| fail-closed route health mirror | `complete` | `gateway_v2_fail_closed_requests{method,reconnect_backoff_active}` is now mirrored into `/healthz.v2Connections.failClosedRequestCounts`, `lastFailClosedRequestMethod`, `lastFailClosedRequestReconnectBackoffActive`, and `lastFailClosedRequestAt`, so rollout evidence can identify which v2 methods were deliberately protected from partial multi-worker routing and whether reconnect backoff was active when they failed closed. |
| upstream request failure health mirror | `complete` | `gateway_v2_upstream_request_failures{method,reconnect_backoff_active}` is now mirrored into `/healthz.v2Connections.upstreamRequestFailureCounts`, `lastUpstreamRequestFailureMethod`, `lastUpstreamRequestFailureReconnectBackoffActive`, and `lastUpstreamRequestFailureAt`, so degraded-route rollout evidence can identify which v2 methods hit downstream routing or send failures and whether reconnect backoff was active at the time. |
| timeout and backpressure health mirror | `complete` | `gateway_v2_downstream_backpressure_events{worker_id}` is now mirrored into `/healthz.v2Connections.downstreamBackpressureCounts`, `lastDownstreamBackpressureWorkerId`, and `lastDownstreamBackpressureAt`, and `gateway_v2_client_send_timeouts` is mirrored into `clientSendTimeoutCount` plus `lastClientSendTimeoutAt`, so slow-client and lagging-worker rollout evidence is available in health snapshots as well as metrics. |
| thread routing diagnostics health mirror | `complete` | `gateway_v2_thread_list_deduplications`, `gateway_v2_thread_route_recoveries`, and `gateway_v2_degraded_thread_discovery` are now mirrored into `/healthz.v2Connections.threadListDeduplicationCounts`, `threadRouteRecoveryCounts`, `degradedThreadDiscoveryCounts`, and their `last*` fields, so multi-worker rollout evidence can identify duplicate thread-list selection, lazy route recovery outcomes, and intentionally degraded visible-thread discovery from health snapshots as well as metrics and logs. |
| suppressed-notification health mirror | `complete` | `gateway_v2_suppressed_notifications{method,reason}` is now mirrored into `/healthz.v2Connections.suppressedNotificationCounts`, `lastSuppressedNotificationMethod`, `lastSuppressedNotificationReason`, and `lastSuppressedNotificationAt`, so operators can reconcile duplicate notification suppression, pending-refresh suppression, client opt-out drops, and hidden-thread notification drops from health snapshots as well as metrics and logs. |
| notification delivery health mirror | `complete` | `gateway_v2_forwarded_notifications{method}` and `gateway_v2_notification_send_failures{method,outcome}` are now mirrored into `/healthz.v2Connections.forwardedNotificationCounts`, `lastForwardedNotificationMethod`, `lastForwardedNotificationAt`, `notificationSendFailureCounts`, `lastNotificationSendFailureMethod`, `lastNotificationSendFailureOutcome`, and `lastNotificationSendFailureAt`, so operators can correlate successful notification fan-in and slow-client delivery failures with suppression diagnostics in the same health snapshot. |
| transport and server-request delivery failure health mirror | `complete` | `gateway_v2_client_response_send_failures`, `gateway_v2_downstream_shutdown_failures`, `gateway_v2_close_frame_send_failures`, `gateway_v2_server_request_forward_send_failures`, `gateway_v2_server_request_answer_delivery_failures`, and `gateway_v2_server_request_rejection_delivery_failures` are now mirrored into `/healthz.v2Connections` count arrays plus corresponding `last*` fields, so operators can correlate slow-client response writes, failed close frames, shutdown cleanup failures, prompt forward failures, answer delivery failures, and rejection delivery failures from health snapshots as well as metrics and logs. |
| server-request lifecycle health mirror | `complete` | `gateway_v2_server_request_lifecycle_events{event,method}` is now mirrored into `/healthz.v2Connections.serverRequestLifecycleEventCounts`, `lastServerRequestLifecycleEvent`, `lastServerRequestLifecycleMethod`, and `lastServerRequestLifecycleAt`, so operators can correlate prompt forwarding, answering, downstream delivery, resolution, duplicate replay, rejection, cleanup, and delivery-failed stages from health snapshots as well as metrics and logs. |
| server-request cleanup event stream | `complete` | V2 worker-loss cleanup now publishes `gateway/v2ServerRequestCleanup` on `/v1/events` whenever cleanup resolves thread-scoped prompts or finds stranded connection-scoped prompts, carrying the affected worker route, remaining worker count, disconnect message, cleanup counts, prompt method families, resolved thread ids, and gateway / downstream server-request ids so promotion evidence can reconcile cleanup lifecycle metrics, health, logs, and operator event-stream captures. Real northbound WebSocket regressions pin that event payload for both a direct cleanup helper and the pending / answered-but-unresolved worker-loss paths that strand connection-scoped prompts. The cleanup log and operator event are emitted before synthesized `serverRequest/resolved` notifications are sent to the northbound client, so a later slow-client send failure does not hide the cleanup from `/v1/events`. |
| protocol-violation health mirror | `complete` | `gateway_v2_protocol_violations` is now mirrored into `/healthz.v2Connections.protocolViolationCounts`, `protocolViolationWorkerCounts`, `lastProtocolViolationPhase`, `lastProtocolViolationReason`, `lastProtocolViolationWorkerId`, and `lastProtocolViolationAt`, so operators can distinguish malformed pre-initialize traffic, post-initialize client protocol violations, and worker-specific downstream app-server protocol regressions from health snapshots even when metrics export is delayed or unavailable. |
| v2 scope enforcement | `complete` | Embedded, single-worker remote, and multi-worker remote v2 connections now derive tenant/project scope from the WebSocket upgrade headers, enforce thread visibility on thread-scoped requests, filter `thread/list` plus `thread/loaded/list`, reject downstream server requests for hidden threads, backfill worker ownership for already visible threads discovered through aggregated list responses, register resumed/forked thread IDs back into scope ownership, and now also register visible rollout paths so path-based `thread/resume` / `thread/fork` plus legacy `getConversationSummary.rolloutPath` stay scope-checked instead of bypassing ownership. Hidden-thread downstream server requests now also emit structured warning logs with scope, worker id, worker websocket URL, request id, method, and hidden thread id when the gateway rejects them. Unknown path-based `thread/resume` and `thread/fork` rejections now also have request-metric coverage for `outcome=invalid_params`, and direct scope-policy coverage pins that their placeholder `threadId` values are ignored for route affinity while `path` remains the scope-check key. History-based `thread/resume` is now treated as a connection-scoped request, so the gateway no longer rejects that app-server flow just because the placeholder `threadId` is not yet visible. Multi-worker remote runtime now also has real northbound `RemoteAppServerClient` regressions covering same-scope re-entry plus cross-scope filtering for aggregated `thread/list` / `thread/loaded/list` / `thread/read`, plus visible-path `thread/resume` / `thread/fork` routing through one shared client session. |
| v2 admission/rate limiting | `complete` | The gateway now applies the same per-scope request rate limit and turn-start quota policy to v2 JSON-RPC requests that it already applies to HTTP routes. Dedicated WebSocket coverage pins both the `turn/start` quota path and the broader request-rate-limit path, including request metrics and audit records with `outcome=rate_limited`. |
| v2 per-message audit/metrics | `complete` | The gateway now emits per-request v2 metrics and audit logs for `initialize` and subsequent client JSON-RPC requests, tagged by method and outcome. Coverage now ties request metrics to startup timeout, invalid initialize params, downstream initialize protocol violations, downstream connection failures, pre-initialize ordering violations, repeated post-initialize `initialize`, malformed client payloads, downstream malformed/out-of-order traffic, unknown server-request replies, and ordinary success / policy outcomes; initialize timeouts, downstream initialize protocol violations, downstream connection failures, malformed initialize params, pre-initialize ordering violations, and repeated post-initialize `initialize` requests now also have direct audit-log coverage with tenant/project scope and their gateway-owned outcomes. Pre- and post-initialize malformed text plus invalid UTF-8 binary payloads now also have direct connection audit-log coverage with tenant/project scope, terminal detail, and `outcome=invalid_client_payload`. |
| v2 per-connection observability | `complete` | The gateway emits per-connection metrics, audit logs, health snapshots, and operator-facing completion logs for northbound WebSocket session outcomes. These records include outcome, duration, scope, terminal detail where applicable, final pending client-request counts plus worker/method summaries, and the final pending, answered-but-unresolved, and combined server-request backlog counts used to diagnose background command saturation and stranded prompt lifecycles; connection metrics now include `gateway_v2_connection_pending_client_requests` alongside pending and answered-but-unresolved server-request histograms so post-teardown dashboards can see all three terminal backlog dimensions with the same outcome tag, and also export `gateway_v2_connection_pending_client_requests_by_worker`, `gateway_v2_connection_pending_client_requests_by_method`, `gateway_v2_connection_pending_server_requests_by_worker`, `gateway_v2_connection_answered_but_unresolved_server_requests_by_worker`, `gateway_v2_connection_pending_server_requests_by_method`, and `gateway_v2_connection_answered_but_unresolved_server_requests_by_method` with outcome, worker, or method tags so terminal background command and prompt backlog can be split by worker route, command, approval, user-input, elicitation, or token-refresh family. Connection teardown also emits a structured warning when pending background client requests are aborted, including tenant/project scope, terminal outcome/detail, request ids, and methods; if downstream app-server shutdown also fails after a gateway v2 connection error, teardown emits a structured warning with tenant/project scope, terminal outcome/detail, pending client-request count, pending server-request count, answered-but-unresolved server-request count, backlog worker and method summaries, and shutdown error detail, and increments `gateway_v2_downstream_shutdown_failures{outcome}`; completed background client requests are settled before final northbound response delivery and drained again before teardown diagnostics, so those diagnostics only report requests that are still pending downstream. `/healthz` also exposes the latest completed connection duration as `v2Connections.lastConnectionDurationMs`, rolls up active-session pending client-request count as `v2Connections.activeConnectionPendingClientRequestCount`, exposes the largest current and lifecycle-peak pending client-request buildup as `v2Connections.activeConnectionMaxPendingClientRequestCount` plus `v2Connections.activeConnectionPeakPendingClientRequestCount`, rolls up active-session pending plus answered-but-unresolved prompt counts as `v2Connections.activeConnectionPendingServerRequestCount` and `v2Connections.activeConnectionAnsweredButUnresolvedServerRequestCount`, records current and last-completed server-request backlog start timestamps as `v2Connections.activeConnectionServerRequestBacklogStartedAt` and `v2Connections.lastConnectionServerRequestBacklogStartedAt`, records last-completed pending client requests as `v2Connections.lastConnectionPendingClientRequestCount`, records the completed connection lifecycle pending-client peak as `v2Connections.lastConnectionMaxPendingClientRequestCount`, records pending client-request buildup age as `v2Connections.activeConnectionPendingClientRequestStartedAt` and `v2Connections.lastConnectionPendingClientRequestStartedAt`, exposes the independent saturation bounds as `v2Transport.maxPendingClientRequests` and `v2Transport.maxPendingServerRequests`, and exposes remote-worker reconnect backoff as `remoteWorkers[].reconnectBackoffRemainingSeconds` next to `nextReconnectAt`. Single-worker and multi-worker remote health regressions now both pin the gateway-owned `client_send_timed_out` snapshot after a real slow-client WebSocket teardown. Saturated pending client-request rejections emit `gateway_v2_client_request_rejections` with `method` / `reason` tags and now have direct audit-log coverage for `gateway_v2_requests{method="command/exec",outcome="rate_limited"}`; duplicate in-flight background client-request ids now fail closed with `gateway_v2_protocol_violations{phase="post_initialize",reason="duplicate_request_id"}` before the active pending-client route can be overwritten, even when the duplicate id is reused by a different follow-up method, that offending follow-up method is also recorded as `gateway_v2_requests{method,outcome="protocol_violation"}` with a matching request audit log, and saturated or hidden-thread downstream server-request rejections emit `gateway_v2_server_request_rejections` with server-request method plus `pending_limit` / `hidden_thread` reason tags; saturated rejection logs include tenant/project scope, configured pending limit, pending gateway/downstream request ids, and affected thread / worker ids, duplicate pending-client logs include the duplicate id plus original worker id and WebSocket URL, while hidden-thread rejection logs include tenant/project scope, worker URL, rejected request id, method, and hidden thread id. Multi-worker reconnect attempts, successes, failures, replay failures, and backoff suppression emit `gateway_v2_worker_reconnects` with worker/outcome tags, with direct reconnect-path regression coverage for each outcome including malformed recovered-worker handshakes. Multi-worker requests that fail closed because required worker routes are unavailable emit `gateway_v2_fail_closed_requests` with method/backoff tags, while ordinary upstream request failures observed during unavailable worker routes emit `gateway_v2_upstream_request_failures` once at the `handle_client_request` routing boundary with the same method/backoff tags. Suppressed multi-worker connection notifications emit `gateway_v2_suppressed_notifications` with method/reason tags for exact-duplicate connection-state notifications, repeated `skills/changed` invalidations, opted-out notifications, and hidden-thread notification drops. Normal client replies to forwarded server requests emit `gateway_v2_server_request_lifecycle_events` with `event=client_server_request_answered` and `method=response` or `method=error`, and delivery of those replies back to the owning worker emits `event=client_server_request_delivered` or `event=client_server_request_delivery_failed`; pending server-request routes snapshot the worker websocket URL at forwarding time so answered-but-undeliverable and cleanup diagnostics retain the original downstream route, and pending server-request log summaries emit `pending_worker_websocket_urls` alongside `pending_worker_ids` for unexpected replies, saturation, downstream backpressure, slow-client timeout, downstream protocol violations, and duplicate downstream request-id diagnostics. Gateway-owned downstream server-request rejections emit `event=downstream_server_request_rejected_pending_limit` or `event=downstream_server_request_rejected_hidden_thread`, followed by `event=downstream_server_request_rejection_delivered` or `event=downstream_server_request_rejection_delivery_failed` once the gateway attempts to notify the worker. Normal downstream `serverRequest/resolved` notifications emit `event=downstream_server_request_resolved`, duplicate downstream `serverRequest/resolved` replays emit lifecycle events with event/method tags after gateway request-id translation has already drained the route, downstream server requests that reuse a still-pending gateway request id emit the same lifecycle metric with `event=duplicate_pending_request` before the gateway fails the session closed, client replies for unknown server-request ids emit the same lifecycle metric with `event=unexpected_client_server_request_response`, and warning logs for those unexpected replies now pin whether the frame was a JSON-RPC `Response` or `Error` plus pending downstream request and worker ids. Worker-loss cleanup emits lifecycle counters for synthesized thread-scoped `serverRequest/resolved` notifications plus stranded connection-scoped prompts, now records successful synthetic-resolution delivery, and records synthesized-resolution send failures with affected thread ids plus worker websocket URLs in cleanup logs, client-side connection cleanup emits lifecycle counters and warning logs for rejected pending prompts plus answered-but-unresolved prompts left behind by disconnect, protocol-violation, or slow-send teardown paths, including pending downstream request ids, pending worker ids, worker websocket URLs, and affected pending plus answered-but-unresolved thread ids, while malformed or out-of-order client traffic emits `gateway_v2_protocol_violations` with phase/reason tags for initialize ordering, invalid JSON-RPC, invalid UTF-8, repeated initialize, and downstream protocol-violation cases, and pre-initialize malformed text payloads now have direct connection audit-log coverage with tenant/project scope, terminal detail, and `outcome=invalid_client_payload`. Downstream event stream lag emits `gateway_v2_downstream_backpressure_events` with worker tags before the gateway attempts the northbound close, slow-client northbound send timeouts emit `gateway_v2_client_send_timeouts` alongside the terminal `client_send_timed_out` connection outcome and warning logs with pending gateway/downstream request ids plus affected pending and answered-but-unresolved thread ids plus pending worker ids, and multi-worker thread routing diagnostics emit `gateway_v2_thread_list_deduplications`, `gateway_v2_thread_route_recoveries`, and `gateway_v2_degraded_thread_discovery` for duplicate snapshot selection, lazy ownership probe outcomes, and intentionally degraded visible-thread discovery while worker routes are unavailable; the corresponding routing, server-request lifecycle, backpressure, worker-cleanup, dedupe, opted-out suppression, and hidden-thread suppression logs include worker websocket URLs for direct downstream-session identification, including available, unavailable, reconnect-backoff URL sets, paired worker id / URL / remaining-second route diagnostics on degraded-route logs, original forwarded worker routes on exact-duplicate connection notification suppression logs, and tenant/project plus payload context on opted-out and hidden-thread suppression logs. |

## Required Test Subset

The current gateway compatibility tests cover this required subset:

- `initialize`
- `account/read`
- `getAuthStatus`
- `getConversationSummary`
- `gitDiffToRemote`
- `account/rateLimits/read`
- `model/list`
- `externalAgentConfig/detect`
- `externalAgentConfig/import`
- `app/list`
- `mcpServerStatus/list`
- `mcpServer/oauth/login`
- `skills/list`
- `skills/config/write`
- `plugin/list`
- `plugin/read`
- `marketplace/add`
- `config/value/write`
- `plugin/install`
- `plugin/uninstall`
- `config/batchWrite`
- `memory/reset`
- `account/login/start`
- `account/login/cancel`
- `account/logout`
- `config/read`
- `configRequirements/read`
- `experimentalFeature/list`
- `experimentalFeature/enablement/set`
- `collaborationMode/list`
- `config/mcpServer/reload`
- `mcpServer/resource/read`
- `mcpServer/tool/call`
- `fuzzyFileSearch`
- `fuzzyFileSearch/sessionStart`
- `fuzzyFileSearch/sessionUpdate`
- `fuzzyFileSearch/sessionStop`
- `feedback/upload`
- `fs/readFile`
- `fs/writeFile`
- `fs/createDirectory`
- `fs/getMetadata`
- `fs/readDirectory`
- `fs/remove`
- `fs/copy`
- `fs/watch`
- `fs/unwatch`
- `windowsSandbox/setupStart`
- `account/sendAddCreditsNudgeEmail`
- `command/exec`
- `command/exec/outputDelta` notification forwarding
- `command/exec/write`
- `command/exec/resize`
- `command/exec/terminate`
- `thread/start`
- `thread/resume`
- `thread/fork`
- `thread/list`
- `thread/loaded/list`
- `thread/read`
- `thread/name/set`
- `thread/memoryMode/set`
- `thread/unsubscribe`
- `thread/archive`
- `thread/unarchive`
- `thread/metadata/update`
- `thread/turns/list`
- `thread/increment_elicitation`
- `thread/decrement_elicitation`
- `thread/inject_items`
- `thread/compact/start`
- `thread/shellCommand`
- `thread/backgroundTerminals/clean`
- `thread/rollback`
- `review/start`
- `thread/realtime/listVoices`
- `thread/realtime/start`
- `thread/realtime/appendAudio`
- `thread/realtime/appendText`
- `thread/realtime/stop`
- `turn/start`
- `turn/interrupt`
- `turn/steer`
- `thread/started` notification forwarding
- `thread/status/changed` notification forwarding
- `thread/archived` notification forwarding
- `thread/unarchived` notification forwarding
- `thread/closed` notification forwarding
- `turn/started` notification forwarding
- `turn/completed` notification forwarding
- `hook/started` notification forwarding
- `hook/completed` notification forwarding
- `item/autoApprovalReview/started` notification forwarding
- `item/autoApprovalReview/completed` notification forwarding
- `item/started` notification forwarding
- `item/completed` notification forwarding
- `item/agentMessage/delta` notification forwarding
- `item/reasoning/summaryTextDelta` notification forwarding
- `item/reasoning/textDelta` notification forwarding
- `item/commandExecution/outputDelta` notification forwarding
- `item/fileChange/outputDelta` notification forwarding
- `thread/realtime/started` notification forwarding
- `thread/realtime/itemAdded` notification forwarding
- `thread/realtime/outputAudio/delta` notification forwarding
- `thread/realtime/transcript/delta` notification forwarding
- `thread/realtime/transcript/done` notification forwarding
- `thread/realtime/sdp` notification forwarding
- `thread/realtime/error` notification forwarding
- `thread/realtime/closed` notification forwarding
- `account/updated` notification forwarding
- `account/rateLimits/updated` notification forwarding
- `app/list/updated` notification forwarding
- `account/login/completed` notification forwarding
- `mcpServer/oauthLogin/completed` notification forwarding
- `mcpServer/startupStatus/updated` notification forwarding
- `externalAgentConfig/import/completed` notification forwarding
- `warning` notification forwarding
- `configWarning` notification forwarding
- `deprecationNotice` notification forwarding
- `skills/changed` notification forwarding
- `fs/changed` notification forwarding
- `windows/worldWritableWarning` notification forwarding
- `windowsSandbox/setupCompleted` notification forwarding
- `fuzzyFileSearch/sessionUpdated` notification forwarding
- `fuzzyFileSearch/sessionCompleted` notification forwarding
- `item/commandExecution/requestApproval` server-request round trip
- `item/fileChange/requestApproval` server-request round trip
- `item/permissions/requestApproval` server-request round trip
- `item/tool/requestUserInput` server-request round trip
- `mcpServer/elicitation/request` server-request round trip
- `account/chatgptAuthTokens/refresh` server-request round trip

This is still narrower than exhaustive TUI parity because some reconnect paths
and operational edge cases still rely more on targeted gateway regressions than
on one broad end-to-end client harness. Lower-frequency notification fan-in is
now covered by the release-quality single-worker remote baseline and the broad
multi-worker Stage B harness where the relevant app-server flows can emit those
notifications through an unmodified `RemoteAppServerClient` session.

The legacy `getAuthStatus`, `getConversationSummary`, and `gitDiffToRemote`
entries above are included because they remain part of the shared app-server
client-request surface and now have explicit gateway compatibility behavior.
They are not part of the primary current TUI bootstrap path.

Separate from the current TUI-focused matrix, dedicated northbound passthrough
coverage now also exercises several lower-frequency app-server methods outside
that hot path: `thread/archive`, `thread/unarchive`,
`thread/metadata/update`, `thread/turns/list`,
`thread/realtime/listVoices`, `thread/increment_elicitation`,
`thread/decrement_elicitation`, and `thread/inject_items`.

Dedicated northbound passthrough coverage now also exercises several
supporting/configuration methods that some clients and test tools use outside
the main TUI flow: `config/read`, `configRequirements/read`,
`experimentalFeature/list`, and `collaborationMode/list`.

Dedicated northbound passthrough coverage now also exercises current TUI
bootstrap/setup discovery requests outside the main account/model hot path:
`externalAgentConfig/detect`, `app/list`, `skills/list`,
`mcpServerStatus/list`, `mcpServer/oauth/login`, and `plugin/list`.

Dedicated northbound passthrough coverage now also exercises current TUI
setup-mutation and plugin-management requests:
`externalAgentConfig/import`, `plugin/read`, `plugin/install`,
`plugin/uninstall`, `config/batchWrite`, `memory/reset`, and
`account/logout`.

Dedicated northbound passthrough coverage now also exercises the core
thread/turn transport requests current TUI sessions rely on:
`thread/start`, `thread/resume`, `thread/fork`, `thread/list`,
`thread/loaded/list`, `thread/read`, `thread/name/set`,
`thread/memoryMode/set`, and `turn/start`.

Real northbound `RemoteAppServerClient` coverage now also exercises those same
supporting/configuration methods in embedded and single-worker remote mode, and
multi-worker remote mode now also routes threadless `config/read` by matching
`cwd` while keeping `configRequirements/read` on the primary worker, plus
aggregated capability discovery for `experimentalFeature/list` and
`collaborationMode/list`, so they are no longer validated only through
targeted passthrough fixtures.
That same real embedded coverage now also includes `account/rateLimits/read`,
so the client's background rate-limit refresh path is exercised through the
in-process gateway transport instead of only through targeted passthrough
fixtures.

Real northbound multi-worker coverage now also exercises the primary-worker
onboarding and feedback flows `account/login/start`,
`account/login/cancel`, `account/login/completed`, and `feedback/upload`.

Multi-worker hardening coverage notes:

- dedicated northbound multi-worker tests now verify exact-duplicate
  connection-state notifications such as `account/updated` are emitted only
  once on the shared northbound session
- dedicated northbound multi-worker tests now also verify that a later
  `mcpServer/oauth/login` reconnects a recovered worker before falling
  through to worker-discovery routing, instead of staying pinned to the
  primary worker
- dedicated northbound multi-worker tests now also verify that lazy
  visible-thread route recovery fails closed while a required worker is still
  unavailable during reconnect backoff, instead of probing only the surviving
  worker subset for an already-visible `thread/read`
- dedicated northbound multi-worker tests now also verify that degraded
  `thread/list` and `thread/loaded/list` responses emit structured warning
  logs with scope, unavailable worker websocket URLs, plus available,
  unavailable, and reconnect-backoff worker ids when the gateway intentionally
  serves the surviving workers' partial thread view
- the real multi-worker remote connection-state harness now also includes
  `account/login/completed`, `warning`, `configWarning`, and
  `deprecationNotice`, so shared-session dedupe is covered for onboarding
  completion and other visible connection-state notifications in addition to
  the existing account, rate-limit, connector, and MCP startup state
  notifications
- the real single-worker remote connection-state harness now also covers
  `warning`, `configWarning`, `deprecationNotice`, and
  `windows/worldWritableWarning` in both steady state and after worker
  reconnect, so the release-quality remote baseline exercises those visible
  notification paths through an unmodified `RemoteAppServerClient` session
- that same real multi-worker connection-state harness now also includes
  `windows/worldWritableWarning` in both steady state and after worker
  reconnect, so Windows sandbox visibility warnings are covered by the same
  shared-session dedupe regression
- the real multi-worker remote setup-mutation harness now also includes
  `externalAgentConfig/import/completed` in both steady state and
  reconnect-after-recovery flows, so fanout import completion notifications
  are covered by shared-session dedupe regressions as well
- the broader same-session setup-mutation recovery harness now also observes
  `externalAgentConfig/import/completed` after recovered-worker
  `externalAgentConfig/import` fanout and verifies duplicate worker
  completions remain suppressed on the shared `RemoteAppServerClient` session
- dedicated northbound multi-worker tests now also verify duplicate
  `skills/changed` invalidations are suppressed until the client refreshes with
  `skills/list`, and that one fresh invalidation is emitted after that refresh

Embedded-mode coverage notes:

- embedded gateway tests now include a real `RemoteAppServerClient` harness
  against the gateway's northbound v2 WebSocket transport, covering bootstrap
  requests plus `thread/start`, `thread/list`, `thread/loaded/list`,
  `thread/read`, `turn/start`, and
  `thread/started` notification delivery
- that embedded harness also covers the core turn-lifecycle notifications
  `thread/status/changed`, `turn/started`, `item/started`,
  `item/agentMessage/delta`, `item/completed`, and `turn/completed`
- that same embedded harness now also covers streamed reasoning notifications
  for `item/reasoning/summaryTextDelta` and `item/reasoning/textDelta`
- that same embedded harness now also covers the experimental
  `item/tool/call` round trip, including forwarded `serverRequest/resolved`
  plus `item/started` / `item/completed` for the dynamic-tool call lifecycle
  through `turn/completed`
- that embedded harness now also covers `externalAgentConfig/detect`,
  `externalAgentConfig/import`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that embedded harness now also covers a real
  `item/tool/requestUserInput` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that embedded harness now also covers real
  `item/commandExecution/requestApproval` and
  `item/fileChange/requestApproval` round trips, including the forwarded
  `serverRequest/resolved` notification ordering before `turn/completed`
- that embedded harness now also covers a real
  `item/permissions/requestApproval` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that embedded harness now also covers a real
  `mcpServer/elicitation/request` server-request round trip, including the
  forwarded `serverRequest/resolved` notification ordering before
  `turn/completed`
- that same embedded harness now also covers `feedback/upload`, validating
  operator-facing feedback submission through the real northbound client
  transport instead of only through targeted passthrough fixtures
- that same embedded harness now also covers one-shot `fuzzyFileSearch` against
  a real temporary filesystem root through the real northbound client transport
- that same embedded harness now also covers the basic filesystem operation
  family (`fs/readFile`, `fs/writeFile`, `fs/createDirectory`,
  `fs/getMetadata`, `fs/readDirectory`, `fs/remove`, and `fs/copy`) against a
  real temporary filesystem root through the real northbound client transport
- that same embedded harness now also covers standalone command execution for
  `command/exec` plus `command/exec/outputDelta`, and now exercises
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate` against a live PTY-backed process through the real
  northbound client transport
- that embedded harness now also covers lower-frequency thread-control flows
  for `thread/unsubscribe`, `thread/archive`, `thread/unarchive`,
  `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- that same embedded harness now also covers turn-control parity for
  `turn/steer` and `turn/interrupt`, including interrupted-turn completion
  through the real northbound client transport
- embedded gateway tests now cover `thread/list`, `thread/name/set`,
  `thread/memoryMode/set`, and `thread/loaded/list`
- embedded gateway tests also verify the current app-server behavior where
  `thread/resume` and `thread/fork` return `no rollout found for thread id ...`
  until the source thread has materialized rollout storage

Single-worker remote coverage notes:

- single-worker remote gateway tests now include a real
  `RemoteAppServerClient` harness against the gateway's northbound v2
  WebSocket transport, covering bootstrap requests plus `thread/start`,
  `thread/list`, `thread/loaded/list`, `thread/read`, `thread/name/set`,
  `thread/memoryMode/set`, `turn/start`, and `thread/started` notification
  delivery
- that single-worker remote harness now also covers
  `externalAgentConfig/detect`, `externalAgentConfig/import`,
  `account/rateLimits/read`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, `mcpServer/oauth/login`, `config/batchWrite`,
  `memory/reset`, and `account/logout`
- that same single-worker remote harness now also covers low-frequency
  setup/config/MCP/account paths through the real northbound client transport:
  `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `mcpServer/resource/read`, `mcpServer/tool/call`, and
  `account/sendAddCreditsNudgeEmail`
- that same single-worker remote harness now also covers onboarding auth and
  feedback flows for `account/login/start`, `account/login/cancel`,
  `account/login/completed`, and `feedback/upload`
- that same single-worker remote harness now also covers the fuzzy-file-search
  request family through the real northbound client transport, including
  `fuzzyFileSearch`, `fuzzyFileSearch/sessionStart`,
  `fuzzyFileSearch/sessionUpdate`, `fuzzyFileSearch/sessionStop`,
  `fuzzyFileSearch/sessionUpdated`, and
  `fuzzyFileSearch/sessionCompleted`
- that same single-worker remote harness now also covers the basic filesystem
  operation family through the real northbound client transport, including
  `fs/readFile`, `fs/writeFile`, `fs/createDirectory`, `fs/getMetadata`,
  `fs/readDirectory`, `fs/remove`, and `fs/copy`
- that single-worker remote harness now also covers a real
  `item/tool/requestUserInput` server-request round trip over the gateway's
  northbound v2 transport
- that single-worker remote harness now also covers real
  `item/commandExecution/requestApproval`,
  `item/fileChange/requestApproval`, and
  `account/chatgptAuthTokens/refresh` round trips over the same northbound v2
  transport
- that same single-worker remote harness now also covers the
  connection-scoped notifications `account/updated`,
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, `mcpServer/startupStatus/updated`, and
  `skills/changed`
- that same single-worker remote harness now also verifies thread re-entry
  from a later northbound v2 client session, covering `thread/read` after the
  original client disconnects
- that same real embedded harness now also verifies thread re-entry from a
  later northbound v2 client session, covering `thread/resume` plus
  follow-up `thread/read` after the original client disconnects
- that same single-worker remote harness now also covers the dynamic-tool
  `item/tool/call` server-request round trip, including forwarded
  `serverRequest/resolved` ordering plus `item/started` / `item/completed`
  before `turn/completed`
- that same single-worker remote harness now also covers standalone command
  execution for `command/exec`, `command/exec/outputDelta`,
  `command/exec/write`, `command/exec/resize`, and
  `command/exec/terminate`
- that real embedded compatibility harness now also covers realtime workflow
  parity for `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`, and
  `thread/realtime/listVoices`, validating that the in-process gateway reaches
  the realtime sideband transport for start and append requests
- that single-worker remote harness also covers the core turn-lifecycle
  notifications `thread/status/changed`, `turn/started`,
  `item/started`, `item/agentMessage/delta`, `item/completed`, and
  `turn/completed`
- that same single-worker remote harness now also covers streamed reasoning
  notifications `item/reasoning/summaryTextDelta` and
  `item/reasoning/textDelta`, plus longer-running lifecycle notifications
  `hook/started` and `hook/completed`
- that single-worker remote harness now also covers realtime workflow parity
  for `thread/realtime/start`, `thread/realtime/appendText`,
  `thread/realtime/appendAudio`, `thread/realtime/stop`, and
  `thread/realtime/listVoices`, plus delivery for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/outputAudio/delta`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, `thread/realtime/sdp`,
  `thread/realtime/error`, and `thread/realtime/closed`
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still complete that realtime workflow after the downstream worker
  disconnects and the gateway restores the remote session
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still satisfy the broader bootstrap/setup discovery surface:
  `model/list`, `externalAgentConfig/detect`, threadless `app/list`,
  `skills/list`, threadless `plugin/list`, threadless `config/read`,
  `configRequirements/read`, `experimentalFeature/list`,
  `collaborationMode/list`, `mcpServerStatus/list`, and
  `mcpServer/oauth/login`
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still apply low-frequency setup mutations:
  `externalAgentConfig/import`, `marketplace/add`, `skills/config/write`,
  `experimentalFeature/enablement/set`, `config/mcpServer/reload`,
  `config/batchWrite`, `config/value/write`, `memory/reset`, and
  `account/logout`
- that single-worker remote harness now also covers turn-control parity for
  `turn/steer` and `turn/interrupt`, plus the resulting
  `item/agentMessage/delta`, `turn/completed`, and
  `thread/status/changed` notifications
- that single-worker remote harness now also covers lower-frequency
  thread-control and review flows for `thread/unsubscribe`,
  `thread/archive`, `thread/unarchive`, `thread/metadata/update`,
  `thread/turns/list`, `thread/increment_elicitation`,
  `thread/decrement_elicitation`, `thread/inject_items`,
  `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- that same single-worker reconnect harness now also verifies the recovered v2
  session can still complete that lower-frequency thread-control path for
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, and `thread/rollback`
- single-worker remote gateway tests also verify the current app-server
  behavior where `thread/resume` and `thread/fork` return
  `no rollout found for thread id ...` until the source thread has
  materialized rollout storage

Multi-worker remote coverage notes:

- multi-worker remote gateway tests now include a real
  `RemoteAppServerClient` harness against the gateway's northbound v2
  WebSocket transport, covering worker-affine `thread/start`, aggregated
  `thread/list` / `thread/loaded/list` / `thread/read`, and tenant/project
  scope filtering across worker-owned threads
- dedicated northbound multi-worker coverage now also verifies that
  `initialize` client identity, experimental capability, and
  `optOutNotificationMethods` are propagated to every downstream worker
  session, and that lazy reconnect reuses those same initialized capability
  parameters when a missing worker is re-added, so shared-session capability
  negotiation does not diverge from the embedded and single-worker release
  baselines
- dedicated northbound coverage now also verifies that the gateway itself
  suppresses exact opted-out notification methods before forwarding downstream
  notifications, preserving the v2 client-visible initialize capability
  contract even when a worker emits an opted-out notification; that suppression
  also emits a structured gateway log with scope, worker, method, and payload
  context for operator diagnostics
- dedicated multi-worker fan-in coverage now also verifies that an opted-out
  worker notification is dropped without blocking a different non-opted-out
  notification from another worker on the same northbound session
- dedicated northbound coverage now also verifies that malformed downstream
  JSON-RPC fails closed with an explicit `downstream_protocol_violation`
  connection outcome and downstream protocol-violation metric instead of being
  reported as ordinary session termination; malformed downstream frames also
  emit a structured gateway log with scope, worker, reason, downstream message,
  active worker count, and server-request route context for operator
  diagnostics
- downstream non-text JSON-RPC data frames now also fail closed as
  `invalid_binary` protocol violations instead of being silently ignored by the
  remote app-server transport, with dedicated app-server-client and gateway
  northbound regression coverage
- downstream non-text frames during the app-server initialize handshake now
  also record the same downstream `invalid_binary` protocol-violation metric
  and terminal connection outcome, with dedicated northbound regression
  coverage plus direct remote app-server client transport coverage for failing
  connection setup on those frames
- downstream invalid JSON-RPC during the app-server initialize handshake now
  also records the downstream `invalid_jsonrpc` protocol-violation metric and
  terminal connection outcome, with dedicated northbound regression coverage
  plus direct remote app-server client transport coverage for failing
  connection setup on invalid initialize text frames
- downstream initialize responses or errors with a valid JSON-RPC envelope but
  the wrong request id now also fail immediately as malformed initialize
  responses instead of waiting for the initialize timeout, with dedicated
  northbound and remote app-server client transport coverage for the
  downstream `invalid_jsonrpc` protocol-violation classification
- downstream server requests received during the app-server initialize
  handshake now also have direct remote transport coverage for the unsupported
  request path, verifying the setup-time request is rejected with a JSON-RPC
  method-not-found error while the initialize handshake can still complete
- valid downstream server requests received during the app-server initialize
  handshake now also have direct gateway northbound coverage, verifying that
  the pending request is forwarded after northbound initialize completes and
  that the northbound client response routes back to the owning worker
- unsupported downstream server requests received during the app-server
  initialize handshake now also have direct gateway northbound coverage,
  verifying method-not-found rejection at the worker transport while the
  northbound session still completes initialize and serves a follow-up request
- downstream JSON-RPC responses or errors with unknown request ids after
  initialization now also fail closed as downstream `invalid_jsonrpc` protocol
  violations instead of being silently ignored, with direct remote
  app-server-client transport coverage and gateway northbound regression
  coverage
- setup-time downstream protocol violations now also emit a structured gateway
  warning log with tenant/project scope, violation reason, and downstream
  transport detail before the initialize request returns an error, so malformed
  worker handshakes are visible without relying only on metrics or terminal
  connection health
- lazy multi-worker reconnect now also classifies malformed downstream
  initialize traffic as a downstream protocol violation, records the
  protocol-violation metric, and emits a structured warning with tenant/project
  scope plus worker id / URL, so recovered-worker handshake failures remain
  distinct from ordinary reconnect failures
- that multi-worker harness now also covers one shared bootstrap/setup session
  through aggregated `account/read`, `account/rateLimits/read`, `model/list`,
  `externalAgentConfig/detect`, `app/list`, `skills/list`,
  `mcpServerStatus/list`, and worker-discovery `mcpServer/oauth/login`,
  instead of validating those setup paths only through separate targeted
  aggregation regressions
- that same multi-worker harness now also covers cwd-aware threadless
  `config/read`, primary-worker `configRequirements/read`, plus aggregated
  capability discovery for `experimentalFeature/list` and
  `collaborationMode/list`
- that same multi-worker harness now also covers the primary-worker
  onboarding, feedback, and account-nudge flows for `account/login/start`,
  `account/login/cancel`, `account/login/completed`, `feedback/upload`, and
  `account/sendAddCreditsNudgeEmail`
- that same multi-worker harness now also covers translated per-worker
  `item/tool/call` round trips on one shared northbound session, including
  gateway-owned request-id translation for overlapping downstream request ids
- that same multi-worker harness now also covers exact-duplicate suppression
  for the connection-scoped notifications `account/updated`,
  `account/rateLimits/updated`, `account/login/completed`,
  `app/list/updated`, and `mcpServer/startupStatus/updated`, including after
  worker reconnect
- that multi-worker harness now also covers one shared setup-mutation session
  through `externalAgentConfig/import`, `config/batchWrite`,
  `config/value/write`, `memory/reset`, and `account/logout`, verifying that
  those connection-scoped mutations still fan out to both worker sessions
- that same multi-worker harness now also covers shared-session dedupe for
  `externalAgentConfig/import/completed` in both steady state and after worker
  reconnect
- the same-session setup-mutation recovery harness now also observes that
  import-completed dedupe path immediately after recovered-worker
  `externalAgentConfig/import` fanout, so the broader recovered setup flow
  validates the completion notification as well as the request fanout
- that multi-worker harness now also covers lower-frequency thread-control and
  review routing for `thread/unsubscribe`, `thread/archive`,
  `thread/unarchive`, `thread/metadata/update`, `thread/turns/list`,
  `thread/increment_elicitation`, `thread/decrement_elicitation`,
  `thread/inject_items`, `thread/compact/start`, `thread/shellCommand`,
  `thread/backgroundTerminals/clean`, `thread/rollback`, and detached
  `review/start` followed by `thread/read` on the returned review thread
- dedicated northbound websocket reconnect coverage now also pins that same
  lower-frequency sticky thread-control set on one shared client session, so
  later requests for those worker-owned threads re-add the recovered worker
  before routing instead of falling through to the surviving worker
- that multi-worker harness now also covers sticky realtime request routing for
  `thread/realtime/start`, `thread/realtime/appendText`, and
  `thread/realtime/stop`, plus shared-session fan-in for
  `thread/realtime/started`, `thread/realtime/itemAdded`,
  `thread/realtime/transcript/delta`,
  `thread/realtime/transcript/done`, and `thread/realtime/closed`
- that same multi-worker harness now also covers `thread/realtime/appendAudio`
  plus shared-session fan-in for `thread/realtime/outputAudio/delta`,
  `thread/realtime/sdp`, and `thread/realtime/error`
- that same multi-worker harness now also covers connection-scoped realtime
  voice discovery for `thread/realtime/listVoices`, aggregating distinct voice
  inventory across workers while keeping the primary worker defaults stable
- that same multi-worker harness now also covers the primary-worker
  standalone command-execution control plane for `command/exec`,
  `command/exec/outputDelta`, `command/exec/write`,
  `command/exec/resize`, and `command/exec/terminate`
- that same multi-worker reconnect harness now also verifies that realtime
  routing and fan-in still work after one worker disconnects and recovers,
  including aggregated `thread/realtime/listVoices` plus the recovered worker
  and a second healthy worker on one shared v2 client session
- that multi-worker harness now also covers sticky `turn/steer` and
  `turn/interrupt` routing on worker-owned threads, plus shared-session fan-in
  for the resulting `thread/status/changed`, `turn/started`,
  `item/started`, `item/agentMessage/delta`,
  `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`,
  `item/commandExecution/outputDelta`, `item/fileChange/outputDelta`,
  `hook/started`, `hook/completed`, `item/completed`, and
  `turn/completed` notifications
- dedicated northbound multi-worker notification coverage now also verifies
  `rawResponseItem/completed` fan-in from multiple worker-owned visible
  threads on one shared client session
- that multi-worker harness now also verifies that same-scope clients can
  re-enter threads created by another northbound session, while other
  tenant/project headers receive filtered lists and `thread not found` for
  hidden reads
- northbound multi-worker transport now also guards consumed
  `serverRequest/resolved` routes, dropping duplicate downstream replays
  instead of forwarding stale worker-local request ids to the client
- that replay-drop path now also emits a structured warning log with the
  scope, worker id, replayed downstream request id, and any still-buffered
  translated routes
- duplicate `skills/changed` suppression and exact-duplicate connection-state
  notification suppression now also emit structured warning logs with scope,
  worker id, method, and params when those notifications are dropped; real
  northbound WebSocket coverage now also pins exact-duplicate `warning`
  suppression logs with tenant/project scope, dropped and original worker
  websocket URLs, method, and payload context
- exact-duplicate `externalAgentConfig/import/completed` suppression now also
  has dedicated metric coverage, so completion fanout dedupe stays visible
  through `gateway_v2_suppressed_notifications`
- aggregate `thread/list` dedupe coverage now also asserts the
  `gateway_v2_thread_list_deduplications` selected-worker metric on the real
  multi-worker route-selection path, so cross-worker duplicate snapshots are
  visible in operator metrics as well as logs
- lazy visible-thread route recovery coverage now also asserts
  `gateway_v2_thread_route_recoveries` success and miss metrics and verifies
  ordinary probe outcomes do not emit fail-closed or upstream-failure metrics
- degraded multi-worker `thread/list` and `thread/loaded/list` discovery now
  also has coverage asserting one degraded-route warning and one
  `gateway_v2_degraded_thread_discovery` metric point per request, so
  operator-facing reconnect-backoff signals are not duplicated by a single
  partial-worker refresh
- dedicated northbound multi-worker regression coverage now also verifies
  those duplicate downstream `serverRequest/resolved` replays are dropped while
  the shared session remains usable for follow-up requests
- that same multi-worker harness now also covers sticky `thread/resume` and
  `thread/fork` routing on worker-owned threads, plus follow-up `thread/read`
  on the returned forked thread
