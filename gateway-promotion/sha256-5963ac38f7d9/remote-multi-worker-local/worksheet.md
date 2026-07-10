# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: sha256-5963ac38f7d9
- Topology id: remote-multi-worker-local
- Worker builds: sha256-ddb5ca60b296, sha256-ddb5ca60b296
- Worker URLs: ws://127.0.0.1:19001/v2, ws://127.0.0.1:19002/v2
- Account labels: acct-a, acct-b
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=tenant-a, projects=project-a, project-b
- Method matrix route classes covered: project-aware account routing, multi-worker HTTP thread create/read/list, turn start, reconnect recovery, degraded-route fail-closed, account-capacity transition, bounded restoration, live active-context no-handoff, slow-client backlog, cleanup delivery failure

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-bootstrap-thread-turn.txt | healthz/02-steady-state-bootstrap-thread-turn.json | events/02-steady-state-bootstrap-thread-turn.sse | metrics/02-steady-state-bootstrap-thread-turn.json | logs/02-steady-state-bootstrap-thread-turn.log | captured health |
| Project route selection | transcripts/03-project-route-selection.txt | healthz/03-project-route-selection.json | events/03-project-route-selection.sse | metrics/03-project-route-selection.json | logs/03-project-route-selection.log | captured health |
| Reconnect and recovery | transcripts/04-reconnect-recovery.txt | healthz/04-reconnect-recovery.json | events/04-reconnect-recovery.sse | metrics/04-reconnect-recovery.json | logs/04-reconnect-recovery.log | targeted test passed |
| Degraded-route fail-closed | transcripts/05-degraded-route-fail-closed.txt | healthz/05-degraded-route-fail-closed.json | events/05-degraded-route-fail-closed.sse | metrics/05-degraded-route-fail-closed.json | logs/05-degraded-route-fail-closed.log | targeted test passed |
| Account-capacity transition | transcripts/06-account-capacity-transition.txt | healthz/06-account-capacity-transition.json | events/06-account-capacity-transition.sse | metrics/06-account-capacity-transition.json | logs/06-account-capacity-transition.log | targeted test passed |
| Bounded restoration | transcripts/07-bounded-restoration.txt | healthz/07-bounded-restoration.json | events/07-bounded-restoration.sse | metrics/07-bounded-restoration.json | logs/07-bounded-restoration.log | targeted test passed |
| Live active-context no-handoff | transcripts/08-live-active-context-no-handoff.txt | healthz/08-live-active-context-no-handoff.json | events/08-live-active-context-no-handoff.sse | metrics/08-live-active-context-no-handoff.json | logs/08-live-active-context-no-handoff.log | targeted test passed |
| Slow-client or backlog window | transcripts/09-slow-client-backlog-window.txt | healthz/09-slow-client-backlog-window.json | events/09-slow-client-backlog-window.sse | metrics/09-slow-client-backlog-window.json | logs/09-slow-client-backlog-window.log | targeted test passed |
| Cleanup or delivery failure | transcripts/10-cleanup-delivery-failure.txt | healthz/10-cleanup-delivery-failure.json | events/10-cleanup-delivery-failure.sse | metrics/10-cleanup-delivery-failure.json | logs/10-cleanup-delivery-failure.log | targeted test passed |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: tenant-a/project-a routes to worker 0 acct-a and tenant-a/project-b routes to worker 1 acct-b in captured health and transcripts
- Account-capacity identities agree across health, events, metrics, and logs: targeted account-capacity transition test passed with exhausted-account retry evidence
- Bounded handoff success/failure outcomes match client-visible behavior: targeted bounded restoration test passed with replacement-worker handoff evidence
- Live active-context methods fail closed instead of moving accounts: targeted active-context no-handoff test passed with handoff-failure event evidence
- Backlog and cleanup windows are bounded and observable: targeted reconnect, degraded-route, slow-client backlog, and cleanup delivery-failure tests passed

## Decision

- Decision: Scoped Stage B
- Promotion scope: local Docker multi-worker project-aware routing smoke for tenant-a project-a/project-b
- Excluded method families or route classes: wider production rollout
- Follow-up required before wider rollout: replay the targeted fault matrix against a live gateway canary
