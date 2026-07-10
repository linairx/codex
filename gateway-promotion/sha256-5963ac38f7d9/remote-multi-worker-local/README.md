# Gateway Promotion Evidence Bundle

- Gateway build: sha256-5963ac38f7d9
- Topology id: remote-multi-worker-local
- Worker builds: sha256-ddb5ca60b296, sha256-ddb5ca60b296
- Tenant/project scope: tenant=tenant-a, projects=project-a, project-b
- Captured by: Codex local Docker multi-worker verification
- Capture start: 2026-07-09T07:39:34Z
- Capture end: 2026-07-09T07:40:52Z
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | sha256-ddb5ca60b296 | ws://127.0.0.1:19001/v2 | acct-a | none | loopback TCP forward inside gateway container to app-server-a:9001 |
| 1 | sha256-ddb5ca60b296 | ws://127.0.0.1:19002/v2 | acct-b | none | loopback TCP forward inside gateway container to app-server-b:9001 |

## Runtime Configuration

- Gateway listen address: http://127.0.0.1:8081
- Northbound auth: none
- Downstream auth: codex-local-worker-token
- v2 initialize timeout: 30
- v2 client send timeout: 10
- v2 reconnect retry backoff: 1
- v2 max pending server requests: 64
- v2 max pending client requests: 64

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 1 |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-bootstrap-thread-turn.txt | healthz/02-steady-state-bootstrap-thread-turn.json | events/02-steady-state-bootstrap-thread-turn.sse | metrics/02-steady-state-bootstrap-thread-turn.json | logs/02-steady-state-bootstrap-thread-turn.log | worksheet row 2 |
| Project route selection | transcripts/03-project-route-selection.txt | healthz/03-project-route-selection.json | events/03-project-route-selection.sse | metrics/03-project-route-selection.json | logs/03-project-route-selection.log | worksheet row 3 |
| Reconnect and recovery | transcripts/04-reconnect-recovery.txt | healthz/04-reconnect-recovery.json | events/04-reconnect-recovery.sse | metrics/04-reconnect-recovery.json | logs/04-reconnect-recovery.log | worksheet row 4 |
| Degraded-route fail-closed | transcripts/05-degraded-route-fail-closed.txt | healthz/05-degraded-route-fail-closed.json | events/05-degraded-route-fail-closed.sse | metrics/05-degraded-route-fail-closed.json | logs/05-degraded-route-fail-closed.log | worksheet row 5 |
| Account-capacity transition | transcripts/06-account-capacity-transition.txt | healthz/06-account-capacity-transition.json | events/06-account-capacity-transition.sse | metrics/06-account-capacity-transition.json | logs/06-account-capacity-transition.log | worksheet row 6 |
| Bounded restoration | transcripts/07-bounded-restoration.txt | healthz/07-bounded-restoration.json | events/07-bounded-restoration.sse | metrics/07-bounded-restoration.json | logs/07-bounded-restoration.log | worksheet row 7 |
| Live active-context no-handoff | transcripts/08-live-active-context-no-handoff.txt | healthz/08-live-active-context-no-handoff.json | events/08-live-active-context-no-handoff.sse | metrics/08-live-active-context-no-handoff.json | logs/08-live-active-context-no-handoff.log | worksheet row 8 |
| Slow-client or backlog window | transcripts/09-slow-client-backlog-window.txt | healthz/09-slow-client-backlog-window.json | events/09-slow-client-backlog-window.sse | metrics/09-slow-client-backlog-window.json | logs/09-slow-client-backlog-window.log | worksheet row 9 |
| Cleanup or delivery failure | transcripts/10-cleanup-delivery-failure.txt | healthz/10-cleanup-delivery-failure.json | events/10-cleanup-delivery-failure.sse | metrics/10-cleanup-delivery-failure.json | logs/10-cleanup-delivery-failure.log | worksheet row 10 |

## Notes

- Project route selection audit logs were captured from the same gateway image
  and multi-worker topology with `RUST_LOG` enabled so the `codex_gateway.audit`
  tracing target was emitted to container stderr.
