# Gateway Operations

This guide describes how to run `codex-gateway` and how to decide whether a
deployment profile is ready to be treated as app-server v2 compatible.

The detailed compatibility plan lives in
[gateway-v2-compat.md](/home/lin/project/codex/docs/gateway-v2-compat.md).
The method-level routing profile lives in
[gateway-v2-method-matrix.md](/home/lin/project/codex/docs/gateway-v2-method-matrix.md).

## Profile Matrix

| Profile | Status | Use when | Validation expectation |
| --- | --- | --- | --- |
| Embedded | Release-quality local baseline | You want the simplest drop-in v2 compatibility check and the app-server can run in process | Normal client harnesses pass for the exact workflow being added |
| Single-worker remote | Release-quality remote baseline | The app-server must run out of process but one worker still owns the whole northbound session | Normal client harnesses pass and the single worker stays aligned with direct app-server behavior |
| Multi-worker remote | Bounded Stage B validation profile | You need account-aware multi-worker routing for a specific deployment shape | Promotion evidence must be collected for the exact topology before release-quality guidance is claimed |

## Status Terms

- `release-quality`: the deployment profile is treated as a drop-in baseline
  for the workflows covered by the current evidence
- `bounded Stage B`: the deployment profile is validated, but only for the
  exact topology and route classes recorded in the evidence bundle
- `promotion evidence`: the captured health, events, metrics, logs, and
  transcripts that justify the decision for one topology
- `exact topology`: the specific gateway build, worker builds, worker URLs,
  account labels, auth mode, timeouts, and pending-request limits in the
  evidence bundle

## Runtime Signals

Use the same runtime signals across profiles, but interpret them by profile:

- `Embedded`: look for normal `/healthz` stability, ordinary `/v1/events`
  delivery, and client transcripts that match the direct app-server workflow
- `Single-worker remote`: look for the same client-visible behavior plus a
  single downstream worker route and matching reconnect or account-capacity
  signals when that worker changes state
- `Multi-worker remote`: look for `remoteAccountLabelsComplete`, `projectWorkerRoutes`,
  `accountCapacityEvent*`, `/v1/events`, metrics, and structured logs that
  agree on the same tenant, project, worker, and account scope

## Failure Signals

| Situation | Primary signals | Expected operator read |
| --- | --- | --- |
| Account exhaustion | `remoteWorkers[].accountCapacity*`, `accountCapacityEvent*`, `gateway_v2_account_capacity_events`, `/v1/events` account-capacity entries | The exhausted account should be visible in health, events, metrics, and logs, and any live work should fail closed unless a bounded restore surface is being exercised |
| Worker reconnect or loss | `remoteWorkers[].reconnecting`, worker reconnect counters, `gateway_v2_worker_reconnects`, reconnect events | Reconnect/backoff should be obvious without inferring from stale routes; if a required worker is down, the affected route classes should fail closed or re-add the worker later |
| Slow client or downstream lag | pending-client and backlog health fields, `gateway_v2_client_send_timeouts`, downstream backpressure counters, close/send failure logs | The connection should show explicit pressure or timeout state instead of silently stalling, and terminal logs should line up with the same time window |
| Protocol violation | `gateway_v2_protocol_violations`, protocol-violation health fields, close-frame / request rejection metrics, audit logs | Malformed or out-of-order traffic should be attributed to the same worker or phase that triggered it, not disguised as an ordinary disconnect |
| Invalid multi-worker shape | `remoteAccountLabelsComplete=false`, unlabeled-worker counters, `projectWorkerRoutes[].accountRoutingEligible=false`, rollout checklist failure | The deployment should stay bounded Stage B until every worker is labeled and the promotion evidence bundle matches the exact topology |

## Triage Order

When a rollout looks wrong, check the signals in this order:

1. `GET /healthz` for the current worker, account, and route state.
2. `/v1/events` for the exact operator-visible transition that happened.
3. Metrics for the same wall-clock window to confirm whether the problem is
   isolated or repeating.
4. Structured logs for the tenant, project, worker, account, thread, and
   request ids that match the same incident.
5. The evidence worksheet or decision file if the question is whether the
   deployment shape should stay scoped or can be widened.

## Response Matrix

| Situation | Recommended response |
| --- | --- |
| Account exhaustion in a release-quality baseline | Verify whether a bounded restore surface exists; if not, keep the deployment scoped and record the exhausted account, worker, and tenant/project scope in the evidence bundle |
| Worker reconnect or loss during multi-worker validation | Wait for the required worker to rejoin or for the recovery path to fail closed; do not widen traffic until the route classes that depend on that worker pass again |
| Slow-client or downstream lag | Hold the current rollout shape until the pending-client or backlog pressure clears or the terminal timeout/backpressure behavior is proven in the same window |
| Protocol violation | Treat the deployment shape as unstable, capture the malformed or out-of-order traffic details, and rerun the exact scenario after fixing the client or worker behavior |
| Invalid multi-worker shape | Keep the profile bounded Stage B, fix the worker labels or topology mismatch, and regenerate the promotion evidence bundle before widening traffic |

## Rerun Matrix

| Situation | Rerun first |
| --- | --- |
| Account exhaustion in a release-quality baseline | Account-capacity transition, bounded restoration, and live active-context no-handoff |
| Worker reconnect or loss during multi-worker validation | Reconnect and recovery, degraded-route fail-closed, and project route selection |
| Slow-client or downstream lag | Slow-client or backlog window, cleanup or delivery failure, and the affected steady-state scenario |
| Protocol violation | The exact affected scenario plus the baseline before traffic capture for the same topology |
| Invalid multi-worker shape | Baseline before traffic, project route selection, and the full scenario order after the topology is corrected |

## Rollout Checklist

1. Pick the deployment profile first: embedded, single-worker remote, or
   multi-worker remote.
2. Capture the profile-appropriate runtime signals before widening traffic.
3. Compare `/healthz`, `/v1/events`, metrics, and logs for the same
   tenant/project/worker/account scope.
4. Confirm the evidence worksheet and decision taxonomy match the validated
   topology and method families.
5. Treat multi-worker remote as release-quality only when the exact topology
   is fully labeled and the evidence bundle passes review.

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

## Docker Deployment

Use the Dockerfile when you want a packaged local deployment that still
matches the same CLI flags as the direct `cargo run` path.

Build the image from the repository root:

```bash
docker build -f Dockerfile.gateway -t codex-gateway:local .
```

Run the embedded baseline with port 8080 published on the host:

```bash
docker run --rm -p 8080:8080 codex-gateway:local
```

Run a remote worker topology by passing the same CLI flags through the
container entrypoint:

```bash
docker run --rm -p 8080:8080 \
  codex-gateway:local \
  --runtime remote \
  --remote-websocket-url ws://host.docker.internal:9001/v2
```

If your Docker daemon does not resolve `host.docker.internal` automatically,
add `--add-host=host.docker.internal:host-gateway` on Linux or replace the
worker URL with the host address that your container network can reach.

Use the Compose file for the embedded baseline when you want a repeatable
local service definition:

```bash
docker compose -f docker-compose.gateway.yml up --build
```

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
- an explicit decision taxonomy value that names the topology and method
  families covered by the evidence

## Artifact Matrix

| Artifact | Primary purpose |
| --- | --- |
| `README.md` | Record the exact gateway build, topology id, worker builds, worker URLs, account labels, auth mode, capture window, and evidence index |
| `worksheet.md` | Record the scenario-by-scenario capture references and reconciliation notes for the deployment shape being reviewed |
| `decision.md` | Record the decision taxonomy value, promotion scope, route-family exclusions, reconciliation summary, blocking mismatches, and invalidation rules |
| `transcripts/` | Store the client-visible scenario transcript for each captured flow |
| `healthz/` | Store the `/healthz` snapshot for each captured flow and capture window |
| `events/` | Store the `/v1/events` stream for each captured flow and capture window |
| `metrics/` | Store the metric query output for the same wall-clock window as each captured flow |
| `logs/` | Store the structured audit and warning logs for the same wall-clock window as each captured flow |

## Scenario Order

Keep the scenario order identical in `README.md`, `worksheet.md`, and the
capture directories:

1. Baseline before traffic
2. Steady-state bootstrap and thread/turn
3. Project route selection
4. Reconnect and recovery
5. Degraded-route fail-closed
6. Account-capacity transition
7. Bounded restoration
8. Live active-context no-handoff
9. Slow-client or backlog window
10. Cleanup or delivery failure

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

The bundle generator requires one `--tenant-id`, at least one `--project-id`,
at least one `--account-id`, `--auth-mode`, every v2 timeout value, and both
pending-request limits so the worksheet and decision file can be created with
an explicit deployment shape from the start. The generated `README.md` also
repeats that tenant/project scope and runtime shape so the top-level bundle
metadata stays self-describing.

Run `just gateway-promotion-bundle-test` after changing the template generator
or its expected layout.

Use `just gateway-promotion-bundle-create -- ...` to create a fresh promotion
bundle skeleton without remembering the script path.

Run `just gateway-promotion-bundle-check-test` after changing the checker, and
use `just gateway-promotion-bundle-check <bundle-dir>` to validate a populated
promotion bundle before it is reviewed. The checker verifies the required
files, directories, template headings, populated rows, matching capture
scenarios, populated decision metadata, populated capture metadata, and
referenced capture paths before a bundle is accepted for review. The checker
also verifies that the decision reconciliation summary is populated, that
each referenced artifact carries the required gateway, worker, tenant,
project, account, and capture-time labels, and that the referenced paths stay
under the bundle root after canonicalization. The README evidence index and
worksheet captures must refer to the same artifact paths for each scenario.
Each capture directory must contain at least one artifact file.

The README, worksheet, and decision files must agree on topology id,
tenant/project scope, gateway build, and worker builds, and the README
top-level worker build list must match the README topology rows. The worksheet
and decision files must also agree on promotion scope, excluded method
families, method-family coverage, and reconciliation summaries. The README and
worksheet deployment fields for worker URLs, account labels, auth mode,
timeout values, and pending-request limits must also agree.

The README `Worksheet row` column must contain a real row reference, not just
free-form text. In practice that reference should name the matching worksheet
row number, for example `worksheet row 1`, and the row number must line up
with the scenario order in `worksheet.md`. The README capture start and
capture end fields must use `YYYY-MM-DDTHH:MM:SSZ` timestamps, and the checker
rejects both malformed strings and impossible dates. Capture end must not
precede capture start.

The transcript, health, events, metrics, and logs files for one scenario must
also agree on the gateway build, worker build, tenant id, project id, worker
id, account id, and capture time they record. The scenario metadata must match
the bundle's declared gateway build, tenant/project scope, worker build list,
and account-label topology. The worker build and account id must match the
same README topology row rather than only appearing somewhere in the bundle,
and each file's capture time must fall within the README capture window and
obey the same ISO validation rule. The capture metadata labels must have real
values; empty or placeholder metadata is rejected before review. For an
unlabeled worker, write `account id: none` or `account id: <none>` in every
capture artifact; the checker treats those as the blank account label in the
matching topology row.

The README `Decision file` field must point to `decision.md` exactly. The
README and worksheet scenario names must be unique within each file, and they
must retain every required promotion scenario family from the route-class plan
below. Deleting a scenario from both files is still rejected because the
rollout gate needs an explicit pass or scoped exclusion for each family.

The worksheet `Scope`, `Reconciliation`, and `Decision` fields must be
populated. The checker rejects empty or placeholder gateway build, worker
build, worker URL, account label, auth mode, timeout, pending-limit,
tenant/project, method-matrix, route-selection, account-capacity, bounded
handoff, live active-context, backlog/cleanup, decision, promotion scope,
exclusion, and follow-up entries. The worksheet decision value must use the
same taxonomy as `decision.md`.

Every blocking-mismatches row must populate every column; later rows are
validated the same way as the first row. The README topology worker ids and
WebSocket URLs must be unique, and worker ids must use alnum, dot, underscore,
or hyphen characters only. The checker also rejects obvious template
placeholders such as `TBD`, `TODO`, `FIXME`, and `placeholder` in populated
fields, topology table cells, and evidence tables. The paired smoke tests pin
the template fields, the discoverable `--help` / `just` wrapper output, and
the checker failure modes for invalid decision values, inverted capture
windows, malformed or invalid capture times, duplicate scenarios, missing,
empty, placeholder, or out-of-scope capture metadata, placeholder topology
rows, mismatched worker/account topology pairs, path escapes, and mismatched
worksheet references, so edits to the bundle workflow stay aligned with the
documented contract.

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
- Topology id:
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

- Decision:
- Promotion scope:
- Excluded method families or route classes:
- Follow-up required before wider rollout:
```

Evidence bundle `README.md` template:

```markdown
# Gateway Promotion Evidence Bundle

- Gateway build:
- Topology id:
- Worker builds:
- Tenant/project scope:
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

The `Decision:` field must use one of the three taxonomy values above exactly.
For `Release-quality multi-worker`, the excluded method-family fields in
`worksheet.md` and `decision.md` must be `none`; any excluded route class keeps
the decision scoped to Stage B. The README topology must also account-label
every worker for a `Release-quality multi-worker` decision; an unlabeled worker
keeps the bundle in `Scoped Stage B` even when the incomplete-pool behavior was
intentionally validated. The worksheet `Follow-up required before wider
rollout:` field must be `none` for a `Release-quality multi-worker` decision;
any required follow-up keeps the promotion scoped or rejected.

Promotion is invalidated by any later change to worker count, worker URLs,
account labels, auth mode, timeout values, pending-request limits, enabled v2
method families, or gateway / worker builds. Treat those changes as a new
deployment shape and collect a fresh evidence bundle.
