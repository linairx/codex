# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: sha256-8623c7819f4a
- Topology id: remote-multi-worker-local
- Worker builds: sha256-ddb5ca60b296, sha256-ddb5ca60b296
- Worker URLs: ws://127.0.0.1:19001/v2, ws://127.0.0.1:19002/v2
- Backend workers: codex-gateway-mw-worker-a:9001, codex-gateway-mw-worker-b:9001
- Account labels: acct-a, acct-b
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=tenant-a, projects=project-a, project-b
- Method matrix route classes covered: multi-worker health baseline, account-label completeness, worker-pool leases, project-aware account routing, HTTP thread list/create/read, HTTP turn start acceptance

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-bootstrap-thread-turn.txt | healthz/02-steady-state-bootstrap-thread-turn.json | events/02-steady-state-bootstrap-thread-turn.sse | metrics/02-steady-state-bootstrap-thread-turn.json | logs/02-steady-state-bootstrap-thread-turn.log | scoped Stage B evidence |
| Project route selection | transcripts/03-project-route-selection.txt | healthz/03-project-route-selection.json | events/03-project-route-selection.sse | metrics/03-project-route-selection.json | logs/03-project-route-selection.log | captured |
| Reconnect and recovery | transcripts/04-reconnect-recovery.txt | healthz/04-reconnect-recovery.json | events/04-reconnect-recovery.sse | metrics/04-reconnect-recovery.json | logs/04-reconnect-recovery.log | scoped Stage B follow-up |
| Degraded-route fail-closed | transcripts/05-degraded-route-fail-closed.txt | healthz/05-degraded-route-fail-closed.json | events/05-degraded-route-fail-closed.sse | metrics/05-degraded-route-fail-closed.json | logs/05-degraded-route-fail-closed.log | scoped Stage B follow-up |
| Account-capacity transition | transcripts/06-account-capacity-transition.txt | healthz/06-account-capacity-transition.json | events/06-account-capacity-transition.sse | metrics/06-account-capacity-transition.json | logs/06-account-capacity-transition.log | scoped Stage B evidence |
| Bounded restoration | transcripts/07-bounded-restoration.txt | healthz/07-bounded-restoration.json | events/07-bounded-restoration.sse | metrics/07-bounded-restoration.json | logs/07-bounded-restoration.log | scoped Stage B follow-up |
| Live active-context no-handoff | transcripts/08-live-active-context-no-handoff.txt | healthz/08-live-active-context-no-handoff.json | events/08-live-active-context-no-handoff.sse | metrics/08-live-active-context-no-handoff.json | logs/08-live-active-context-no-handoff.log | scoped Stage B follow-up |
| Slow-client or backlog window | transcripts/09-slow-client-backlog-window.txt | healthz/09-slow-client-backlog-window.json | events/09-slow-client-backlog-window.sse | metrics/09-slow-client-backlog-window.json | logs/09-slow-client-backlog-window.log | scoped Stage B follow-up |
| Cleanup or delivery failure | transcripts/10-cleanup-delivery-failure.txt | healthz/10-cleanup-delivery-failure.json | events/10-cleanup-delivery-failure.sse | metrics/10-cleanup-delivery-failure.json | logs/10-cleanup-delivery-failure.log | scoped Stage B follow-up |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: health records project-a on worker 0 / acct-a and project-b on worker 1 / acct-b; events, metrics, and audit logs include project-a route selection.
- Account-capacity identities agree across health, events, metrics, and logs: startup health and audit logs show two labeled healthy worker slots leased to acct-a and acct-b; dynamic capacity transition remains scoped follow-up.
- Bounded handoff success/failure outcomes match client-visible behavior: bounded restoration was not exercised in this local Stage B capture and remains follow-up.
- Live active-context methods fail closed instead of moving accounts: active-context no-handoff behavior was not exercised in this local Stage B capture and remains follow-up.
- Backlog and cleanup windows are bounded and observable: reconnect, slow-client backlog, and cleanup delivery-failure windows were not exercised in this local Stage B capture and remain follow-up.
- Health shows `runtimeMode=remote`, `v2Compatibility=remoteMultiWorker`, two healthy workers, `remoteAccountLabelsComplete=true`, `workerPool.leasedAccountCount=2`, and `workerPool.healthyWorkerSlotCount=2`.
- `/healthz.v2Connections.projectWorkerRouteSelectionCount=2` and `projectWorkerRouteSelectionWorkerCounts` contains one selection for each worker.
- Events include `gateway/projectWorkerRouteSelected` for project-a; the SSE stream was scoped to project-a, so project-b route selection is reconciled through health and audit logs.
- HTTP thread/turn smoke accepted turns on both project-scoped threads. Model execution was not part of this capture because the local workers did not have OpenAI bearer credentials and emitted expected upstream 401 retry events.
- Metrics files record local health/log reconciliation because this local gateway has no `/metrics` endpoint.

## Decision

- Decision: Scoped Stage B
- Promotion scope: remote multi-worker local health plus project-aware routing smoke
- Included method families or route classes: multi-worker health baseline, account-label completeness, worker-pool leases, project-aware account routing, HTTP thread list/create/read, HTTP turn start acceptance
- Excluded method families or route classes: reconnect/degraded routing, bounded account handoff, live active-context fail-closed, slow-client backlog, cleanup delivery failure, full model turn completion with authenticated upstream OpenAI responses
- Follow-up required before wider rollout: run the full multi-worker promotion bundle with real telemetry export and authenticated upstream model traffic, preferably using `wss://` or a deployment-native secure worker transport instead of local loopback TCP forwards.
