# Gateway Promotion Evidence Bundle

- Gateway build: codex-gateway-6e29ba757b
- Topology id: embedded-auth-container
- Worker builds: embedded
- Tenant/project scope: tenant=local, projects=/tmp/project
- Captured by: Codex authenticated local-container verification
- Capture start: 2026-07-11T10:38:10Z
- Capture end: 2026-07-11T10:38:10Z
- Decision file: decision.md

## Topology

| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- | --- |
| 0 | embedded | embedded://in-process | chatgpt-authenticated | chatgpt | |

## Runtime Configuration

- Gateway listen address:
- Northbound auth: chatgpt
- Downstream auth:
- v2 initialize timeout: 30
- v2 client send timeout: 10
- v2 reconnect retry backoff: 1
- v2 max pending server requests: 64
- v2 max pending client requests: 64

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 1 |
| Steady-state bootstrap and thread/turn | transcripts/02-thread-turn.txt | healthz/02-thread-turn.json | events/02-thread-turn.sse | metrics/02-thread-turn.json | logs/02-thread-turn.log | worksheet row 2 |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |
