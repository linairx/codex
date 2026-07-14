# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: codex-gateway-6e29ba757b
- Topology id: embedded-compose-18081
- Worker builds: embedded
- Worker URLs: embedded://in-process
- Account labels: none
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local, projects=/tmp/project
- Method matrix route classes covered: healthz, HTTP thread creation, HTTP turn admission

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-thread-turn.txt | healthz/02-thread-turn.json | events/02-thread-turn.sse | metrics/02-thread-turn.json | logs/02-thread-turn.log | partial: gateway admitted the turn; model backend returned HTTP 401 |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: not applicable to embedded topology
- Account-capacity identities agree across health, events, metrics, and logs: not applicable to embedded topology
- Bounded handoff success/failure outcomes match client-visible behavior: not captured
- Live active-context methods fail closed instead of moving accounts: not captured
- Backlog and cleanup windows are bounded and observable: not captured; model backend authentication failed before completion

## Decision

- Decision: Reject / no promotion
- Promotion scope: embedded Compose deployment smoke and baseline HTTP thread/turn admission only
- Excluded method families or route classes: completed model turns; all remote and multi-worker scenarios
- Follow-up required before wider rollout: configure valid model credentials, then repeat the complete embedded traffic window before collecting remote topology evidence
