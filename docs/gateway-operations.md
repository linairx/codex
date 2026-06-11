# Gateway Operations

This guide describes how to run `codex-gateway` and how to decide whether a
deployment profile is ready to be treated as app-server v2 compatible.

The detailed compatibility plan lives in
[gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md).
The method-level routing profile lives in
[gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md).

## Deployment Profiles

Embedded mode is the release-quality local baseline. It runs the gateway and
app-server stack in one process and should be used as the first compatibility
check for new v2 client workflows.

```bash
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime embedded
```

Single-worker remote mode is the release-quality remote baseline. It keeps one
northbound v2 client connection mapped to one downstream app-server worker.

```bash
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime remote \
  --remote-websocket-url ws://127.0.0.1:9001/v2
```

Use `--remote-auth-token` when downstream workers require bearer
authentication, and use `--bearer-token` when northbound clients must
authenticate to the gateway.

Multi-worker remote mode is a bounded Stage B profile until the exact
deployment shape has promotion evidence. Every account-backed worker in an
account-aware validation should have a matching `--remote-account-id`; unlabeled
workers are allowed only when the rollout explicitly validates the incomplete
pool behavior.

```bash
cargo run -p codex-gateway -- \
  --listen 127.0.0.1:8080 \
  --runtime remote \
  --bearer-token "$GATEWAY_TOKEN" \
  --remote-auth-token "$WORKER_TOKEN" \
  --remote-websocket-url ws://127.0.0.1:9001/v2 \
  --remote-account-id acct-a \
  --remote-websocket-url ws://127.0.0.1:9002/v2 \
  --remote-account-id acct-b \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64
```

When `--remote-account-id` is configured, it must be provided once per
`--remote-websocket-url`.

## Health Checks

Use `GET /healthz` before traffic, during steady-state traffic, during worker
reconnect, after account-capacity changes, and after induced failure cases.
For multi-worker release evidence, keep these fields together in the same
snapshot:

- `remoteWorkers[]`, including `accountId`, `healthy`, `reconnecting`,
  `accountCapacity`, `accountCapacityReason`, and reconnect timing fields
- `remoteAccountLabelsComplete`, `remoteUnlabeledAccountWorkerCount`,
  `remoteUnlabeledAccountWorkerIds`, and `remoteUnlabeledAccountWorkers`
- `projectWorkerRoutes[]`, including `accountRoutingEligible`
- `v2Connections`, including request outcomes, connection outcomes,
  fail-closed requests, account-capacity events, reconnect events,
  server-request backlog fields, and pending client-request fields

For account-aware routing, do not infer eligibility from only one field. The
route is eligible only when the pinned worker is healthy, account-labeled, and
has available account capacity.

Example capture:

```bash
curl -fsS http://127.0.0.1:8080/healthz \
  | jq . > healthz-baseline.json
```

For scoped validation, include the same tenant and project headers used by the
client traffic:

```bash
curl -fsS \
  -H 'x-codex-tenant-id: tenant-a' \
  -H 'x-codex-project-id: project-a' \
  http://127.0.0.1:8080/healthz \
  | jq . > healthz-project-a.json
```

## Promotion Evidence

Use one evidence worksheet per deployment shape. Do not mix evidence from
different gateway builds, worker builds, account labels, timeout values, or
pending-request limits.

At minimum, capture:

- build ids, worker URLs, account labels, auth mode, timeout values, and
  pending-request limits
- the method families exercised from the v2 method matrix
- baseline and traffic `/healthz` snapshots
- `/v1/events` records for project route selection, account exhaustion,
  account handoff, active-thread no-handoff, reconnect, and cleanup cases
- exported metrics for the same time windows as the health snapshots
- `codex_gateway.audit` and structured warning logs for the same tenant,
  project, worker, account, thread, and request ids
- client transcripts for bootstrap, thread/turn, approval or user-input,
  reconnect, degraded-route, account-capacity, and cleanup scenarios
- an explicit pass/fail decision that names the topology and method families
  covered by the evidence

The promotion evidence worksheet in
[gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md)
is the source of truth for the full evidence list.

Recommended evidence bundle layout:

```text
gateway-promotion/<gateway-build>/<topology-id>/
  README.md
  worksheet.md
  transcripts/
  healthz/
  events/
  metrics/
  logs/
  decision.md
```

Create that skeleton with:

```bash
scripts/create-gateway-promotion-bundle.sh \
  --output gateway-promotion \
  --gateway-build "$GATEWAY_BUILD" \
  --topology-id "$TOPOLOGY_ID" \
  --worker-build "$WORKER_A_BUILD" \
  --worker-build "$WORKER_B_BUILD" \
  --worker-url ws://127.0.0.1:9001/v2 \
  --worker-url ws://127.0.0.1:9002/v2 \
  --account-id acct-a \
  --account-id acct-b \
  --tenant-id tenant-a \
  --project-id project-a \
  --project-id project-b \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64
```

Run `just gateway-promotion-bundle-test` after changing the template generator
or its expected layout.

Run `just gateway-promotion-bundle-check-test` after changing the checker, and
use `just gateway-promotion-bundle-check <bundle-dir>` to validate a populated
promotion bundle before it is reviewed. The checker verifies the required
files, directories, template headings, and populated rows before a bundle is
accepted for review.

Use stable names that preserve the scenario order, for example
`01-baseline.json`, `02-steady-state-project-a.json`,
`03-worker-reconnect-project-a.json`, and
`04-account-exhaustion-project-a.json`. Each captured file should identify the
gateway build, worker build, tenant id, project id, worker id, account id, and
capture time either in the file body or in the worksheet row that references
it.

Worksheet template:

```markdown
# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build:
- Worker builds:
- Worker URLs:
- Account labels:
- Auth mode:
- v2 timeout values:
- Pending request limits:
- Tenant/project scope:
- Method matrix route classes covered:

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | | | | | | |
| Steady-state bootstrap and thread/turn | | | | | | |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit:
- Account-capacity identities agree across health, events, metrics, and logs:
- Bounded handoff success/failure outcomes match client-visible behavior:
- Live active-context methods fail closed instead of moving accounts:
- Backlog and cleanup windows are bounded and observable:

## Decision

- Pass/fail:
- Promotion scope:
- Excluded method families or route classes:
- Follow-up required before wider rollout:
```

Evidence bundle `README.md` template:

```markdown
# Gateway Promotion Evidence Bundle

- Gateway build:
- Worker builds:
- Captured by:
- Capture start:
- Capture end:
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| | | | | |

## Runtime Configuration

- Gateway listen address:
- Northbound auth:
- Downstream auth:
- v2 initialize timeout:
- v2 client send timeout:
- v2 reconnect retry backoff:
- v2 max pending server requests:
- v2 max pending client requests:

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
| | | | | | | |
```

Evidence bundle `decision.md` template:

```markdown
# Gateway Multi-Worker Promotion Decision

- Decision:
- Promotion scope:
- Topology id:
- Gateway build:
- Worker builds:
- Tenant/project scopes:
- Method families included:
- Method families excluded:

## Reconciliation Summary

- Route-selection evidence:
- Account-capacity evidence:
- Bounded restoration evidence:
- Live active-context no-handoff evidence:
- Reconnect/degraded-route evidence:
- Slow-client/backlog evidence:
- Cleanup/delivery-failure evidence:

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| | | | | |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
```

## Event And Metric Capture

`GET /v1/events` is an SSE stream. Start the stream before inducing the event
you want to validate, and keep the tenant/project headers aligned with the
client flow when the evidence is scope-specific.

```bash
curl -N \
  -H 'x-codex-tenant-id: tenant-a' \
  -H 'x-codex-project-id: project-a' \
  http://127.0.0.1:8080/v1/events \
  > gateway-events-project-a.sse
```

Use this stream to retain operator events such as:

- `gateway/projectWorkerRouteSelected`
- `gateway/accountCapacityExhausted`
- `gateway/accountFailoverSucceeded`
- `gateway/accountThreadHandoffSucceeded`
- `gateway/accountThreadHandoffFailed`
- `gateway/accountPathHandoffSucceeded`
- `gateway/accountPathHandoffFailed`
- `gateway/accountActiveThreadHandoffFailed`
- `gateway/v2ServerRequestCleanup`

Gateway metrics are exported through the configured `codex_otel` pipeline; the
gateway does not expose a separate `/metrics` HTTP route. During promotion,
query the metrics backend for the same time window as the `/healthz`,
`/v1/events`, and audit-log captures.

The most important metric families for multi-worker rollout are:

- `gateway_project_worker_route_selections`
- `gateway_remote_account_label_events`
- `gateway_account_capacity_events`
- `gateway_v2_account_capacity_events`
- `gateway_v2_worker_reconnects`
- `gateway_v2_fail_closed_requests`
- `gateway_v2_upstream_request_failures`
- `gateway_v2_requests`
- `gateway_v2_connections`
- `gateway_v2_connection_pending_client_requests_by_worker`
- `gateway_v2_connection_pending_client_requests_by_method`
- `gateway_v2_connection_max_pending_client_requests`
- `gateway_v2_connection_server_request_backlog_by_worker`
- `gateway_v2_connection_server_request_backlog_by_method`
- `gateway_v2_connection_max_server_request_backlog`
- `gateway_v2_server_request_lifecycle_events`
- `gateway_v2_protocol_violations`
- `gateway_v2_suppressed_notifications`
- `gateway_v2_forwarded_notifications`
- `gateway_v2_notification_send_failures`

## Capture Runbook

Use this sequence for each multi-worker deployment shape being considered for
promotion:

1. Create the evidence bundle and write the exact gateway and worker startup
   commands into `README.md`.
2. Start `/v1/events` capture for every tenant/project scope that will be
   validated before any client traffic runs.
3. Capture baseline `/healthz` for the unscoped gateway view and for each
   scoped tenant/project view.
4. Run the client scenario and save the transcript under `transcripts/`.
5. Capture `/healthz` immediately after each scenario while the relevant
   connection, pending request, reconnect, or account-capacity state is still
   visible.
6. Export metrics for the same wall-clock window as the scenario and save the
   raw query output under `metrics/`.
7. Save gateway audit and structured logs for the same wall-clock window under
   `logs/`.
8. Fill the worksheet row for that scenario before moving to the next scenario,
   so mismatched tenant, project, worker, account, thread, or request ids are
   caught while the run can still be reproduced.
9. Write `decision.md` only after every required route class has either passed
   or has been explicitly excluded from the promotion scope.

Useful health slices:

```bash
jq '{remoteWorkers, remoteAccountLabelsComplete, projectWorkerRoutes}' \
  healthz/02-steady-state-project-a.json

jq '.v2Connections | {
  connectionOutcomeCounts,
  requestCounts,
  failClosedRequestCounts,
  accountCapacityEventCounts,
  workerReconnectEventCounts,
  activeConnectionPendingClientRequestCount,
  activeConnectionPendingServerRequestCount,
  activeConnectionAnsweredButUnresolvedServerRequestCount,
  activeConnectionServerRequestBacklogWorkerCounts,
  activeConnectionServerRequestBacklogMethodCounts
}' healthz/02-steady-state-project-a.json
```

For project route selection, reconcile these fields before accepting the row:

- `/healthz.projectWorkerRoutes[].tenantId`
- `/healthz.projectWorkerRoutes[].projectId`
- `/healthz.projectWorkerRoutes[].workerId`
- `/healthz.projectWorkerRoutes[].accountId`
- `/healthz.v2Connections.lastProjectWorkerRouteSelected*`
- `/v1/events` `gateway/projectWorkerRouteSelected`
- `gateway_project_worker_route_selections` metric labels
- `codex_gateway.audit` `gateway project worker route selected` log fields

For account-capacity transitions and handoffs, reconcile these fields before
accepting the row:

- `/healthz.remoteWorkers[].accountCapacity*`
- `/healthz.v2Connections.accountCapacityEvent*`
- `/v1/events` account-capacity or handoff event payload
- `gateway_account_capacity_events` or `gateway_v2_account_capacity_events`
  metric labels
- structured logs for the same tenant, project, worker, account, thread, and
  request ids

## Scenario Coverage Map

Before running promotion evidence, map the deployment's expected client flows
to the route classes in
[gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md).
At minimum, a multi-worker promotion bundle should include one passing or
explicitly excluded scenario for each row below.

| Scenario | Route classes covered | Minimum evidence |
| --- | --- | --- |
| Bootstrap discovery | aggregation | `account/read`, `account/rateLimits/read`, `model/list`, `thread/list`, and another inventory method return merged results, fail closed while a required worker is unavailable, and re-add a recovered worker on a later request |
| Shared setup mutation | fanout mutation | a setup mutation reaches every configured worker, duplicate notifications are suppressed, and the request fails closed while any required worker is in reconnect backoff |
| Primary side effect | primary-worker affinity | a primary-worker-only request is not duplicated across workers, reconnects the primary worker before routing when possible, and fails closed while the primary worker is unavailable |
| Worker discovery | worker discovery | plugin, MCP OAuth, or legacy git-diff routing selects the first eligible worker with the resource and fails closed when the worker set is incomplete |
| Thread ownership | thread-sticky routing | `thread/read`, turn control, and thread-scoped MCP or app discovery stay on the visible thread owner and fail closed for hidden, missing, or unavailable routes |
| Project route selection | project-aware account routing | project-scoped `thread/start` records the expected project-to-worker/account route, different projects distribute across eligible accounts, and all route-selection evidence agrees |
| Bounded restore | bounded account handoff | an explicit restore surface succeeds on a replacement account or fails closed with no replacement; follow-up `thread/read` proves sticky routing after success |
| Live no-handoff | live active-context fail-closed | turn, realtime, review, thread-scoped MCP, or server-request reply fails closed when the owner account is exhausted and does not replay live work on another account |
| Reconnect and degraded routing | reconnect, degraded-route protection | worker loss produces observable reconnect state, required route classes fail closed during backoff, and recovered workers are re-added on later requests |
| Slow client and backlog | timeout, backpressure, backlog observability | active and terminal health snapshots show pending client requests or server-request backlog, plus matching worker/method metrics and completion logs |
| Cleanup and delivery failure | server-request lifecycle, cleanup event stream | worker-loss cleanup, prompt rejection, answer delivery failure, send timeout, backpressure, or close-frame failure is visible in `/v1/events`, health, metrics, and logs |

If a deployment intentionally does not support one of these scenario families,
record the exclusion in `decision.md` and keep the promotion scope narrower
than full multi-worker drop-in compatibility.

## Release Decisions

Embedded and single-worker remote deployments can be treated as the
release-quality compatibility baseline when their normal client harnesses pass.

Multi-worker remote deployments should stay documented as Stage B unless the
promotion evidence shows all of the following for the exact target topology:

- steady-state traffic behaves according to the method matrix route classes
- reconnect and degraded-route cases fail closed or recover with gateway-owned
  behavior
- project-scoped `thread/start` route selection agrees across `/healthz`,
  `/v1/events`, metrics, and audit logs
- account-capacity transitions agree across worker health, account-capacity
  metrics, operator events, and structured logs
- explicit restore surfaces such as `thread/read`, `thread/resume`,
  `thread/fork`, rollout-path restoration, conversation summaries, or supported
  thread controls either restore context on a replacement account or fail
  closed
- live active-context methods such as turns, realtime, review, thread-scoped
  MCP, and server-request replies do not silently move to another account
- slow-client, pending-request, backlog, cleanup, and delivery-failure cases
  are bounded and observable

If any required evidence is missing or contradictory, keep the deployment
profile scoped to the validated subset instead of calling it release-quality.

Use this decision taxonomy in `decision.md`:

| Decision | Use when | Required wording |
| --- | --- | --- |
| Release-quality multi-worker | Every required scenario passes for the exact topology, account labels, auth mode, timeout values, pending-request limits, and method families being promoted | Name the exact topology and method families; state that evidence matched across health, events, metrics, audit logs, structured logs, and client transcripts |
| Scoped Stage B | Some route classes pass but others are untested, intentionally excluded, or validated only for a narrower client flow | Name the included route classes, excluded route classes, and the operator guardrail that prevents unsupported flows from being described as drop-in compatible |
| Reject / no promotion | Evidence is missing, contradictory, or shows a policy violation such as live active-context work moving accounts without an explicit restore surface | Name the blocking mismatch, affected tenant/project/worker/account identifiers, and the exact scenario that must be rerun after a fix |

Promotion is invalidated by any later change to worker count, worker URLs,
account labels, auth mode, timeout values, pending-request limits, enabled v2
method families, or gateway / worker builds. Treat those changes as a new
deployment shape and collect a fresh evidence bundle.
