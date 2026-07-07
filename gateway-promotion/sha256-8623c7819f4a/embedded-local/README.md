# Gateway Promotion Evidence Bundle

- Gateway build: sha256-8623c7819f4a
- Topology id: embedded-local
- Worker builds: embedded-in-process, codex-gateway remote mock workers, codex-gateway remote mock workers
- Tenant/project scope: tenant=default, projects=project-a
- Captured by: codex
- Capture start: 2026-07-07T04:21:37Z
- Capture end: 2026-07-07T05:24:27Z
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | embedded-in-process | embedded://in-process | local-account | none | |
| 1 | codex-gateway remote mock workers | remote-mock://worker-0 | acct-a | none | project route-selection evidence |
| 2 | codex-gateway remote mock workers | remote-mock://worker-1 | acct-b | none | project route-selection evidence |

## Runtime Configuration

- Gateway listen address: http://127.0.0.1:18080 during mounted-auth live smoke
- Northbound auth: none
- Downstream auth: mounted Codex auth in the embedded container
- v2 initialize timeout: 30
- v2 client send timeout: 10
- v2 reconnect retry backoff: 1
- v2 max pending server requests: 64
- v2 max pending client requests: 64

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 1 |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-project-a.txt | healthz/02-steady-state-project-a.json | events/02-steady-state-project-a.sse | metrics/02-steady-state-project-a.json | logs/02-steady-state-project-a.log | worksheet row 2 |
| Mounted-auth live turn and event consumption | transcripts/03-auth-mounted-live-smoke.txt | healthz/03-auth-mounted-live-smoke.json | events/03-auth-mounted-live-smoke.sse | metrics/03-auth-mounted-live-smoke.json | logs/03-auth-mounted-live-smoke.log | worksheet row 3 |
| Project route selection | transcripts/04-project-route-selection.txt | healthz/04-project-route-selection.json | events/04-project-route-selection.sse | metrics/04-project-route-selection.json | logs/04-project-route-selection.log | worksheet row 4 |
| Reconnect and recovery | transcripts/05-reconnect-and-recovery.txt | healthz/05-reconnect-and-recovery.json | events/05-reconnect-and-recovery.sse | metrics/05-reconnect-and-recovery.json | logs/05-reconnect-and-recovery.log | worksheet row 5 |
| Degraded-route fail-closed | transcripts/06-degraded-route-fail-closed.txt | healthz/06-degraded-route-fail-closed.json | events/06-degraded-route-fail-closed.sse | metrics/06-degraded-route-fail-closed.json | logs/06-degraded-route-fail-closed.log | worksheet row 6 |
| Account-capacity transition | transcripts/07-account-capacity-transition.txt | healthz/07-account-capacity-transition.json | events/07-account-capacity-transition.sse | metrics/07-account-capacity-transition.json | logs/07-account-capacity-transition.log | worksheet row 7 |
| Bounded restoration | transcripts/08-bounded-restoration.txt | healthz/08-bounded-restoration.json | events/08-bounded-restoration.sse | metrics/08-bounded-restoration.json | logs/08-bounded-restoration.log | worksheet row 8 |
| Live active-context no-handoff | transcripts/09-live-active-context-no-handoff.txt | healthz/09-live-active-context-no-handoff.json | events/09-live-active-context-no-handoff.sse | metrics/09-live-active-context-no-handoff.json | logs/09-live-active-context-no-handoff.log | worksheet row 9 |
| Slow-client or backlog window | transcripts/10-slow-client-backlog-window.txt | healthz/10-slow-client-backlog-window.json | events/10-slow-client-backlog-window.sse | metrics/10-slow-client-backlog-window.json | logs/10-slow-client-backlog-window.log | worksheet row 10 |
| Cleanup or delivery failure | transcripts/11-cleanup-delivery-failure.txt | healthz/11-cleanup-delivery-failure.json | events/11-cleanup-delivery-failure.sse | metrics/11-cleanup-delivery-failure.json | logs/11-cleanup-delivery-failure.log | worksheet row 11 |
