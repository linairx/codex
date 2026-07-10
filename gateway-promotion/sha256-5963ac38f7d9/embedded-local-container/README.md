# Gateway Promotion Evidence Bundle

- Gateway build: sha256-5963ac38f7d9
- Topology id: embedded-local-container
- Worker builds: embedded-in-process
- Tenant/project scope: tenant=local, projects=/tmp/project
- Captured by: Codex local Docker verification
- Capture start: 2026-07-09T00:22:34Z
- Capture end: 2026-07-09T00:22:34Z
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | embedded-in-process | embedded://in-process | local-account | none | |

## Runtime Configuration

- Gateway listen address: 0.0.0.0:8080 in container, published as 127.0.0.1:8080 on host
- Northbound auth: none
- Downstream auth: not applicable for embedded runtime
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
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |
