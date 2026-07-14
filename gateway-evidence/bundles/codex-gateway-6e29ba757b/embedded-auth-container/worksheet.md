# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: codex-gateway-6e29ba757b
- Topology id: embedded-auth-container
- Worker builds: embedded
- Worker URLs: embedded://in-process
- Account labels: chatgpt-authenticated
- Auth mode: chatgpt
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local, projects=/tmp/project
- Method matrix route classes covered:

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-thread-turn.txt | healthz/02-thread-turn.json | events/02-thread-turn.sse | metrics/02-thread-turn.json | logs/02-thread-turn.log | captured health |
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
