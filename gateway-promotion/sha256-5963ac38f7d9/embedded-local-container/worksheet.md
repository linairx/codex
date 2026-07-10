# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: sha256-5963ac38f7d9
- Topology id: embedded-local-container
- Worker builds: embedded-in-process
- Worker URLs: embedded://in-process
- Account labels: local-account
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local, projects=/tmp/project
- Method matrix route classes covered: embedded HTTP health, thread create/read/list, turn start

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-bootstrap-thread-turn.txt | healthz/02-steady-state-bootstrap-thread-turn.json | events/02-steady-state-bootstrap-thread-turn.sse | metrics/02-steady-state-bootstrap-thread-turn.json | logs/02-steady-state-bootstrap-thread-turn.log | captured health |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: not exercised in this embedded smoke bundle
- Account-capacity identities agree across health, events, metrics, and logs: not exercised in this embedded smoke bundle
- Bounded handoff success/failure outcomes match client-visible behavior: not exercised in this embedded smoke bundle
- Live active-context methods fail closed instead of moving accounts: not exercised in this embedded smoke bundle
- Backlog and cleanup windows are bounded and observable: not exercised in this embedded smoke bundle

## Decision

- Decision: Reject / no promotion
- Promotion scope: local embedded Docker startup, /healthz, and basic HTTP thread/turn flow
- Excluded method families or route classes: project routing, reconnect, degraded route, account capacity, bounded restoration, live active-context no-handoff, backlog, cleanup, multi-worker remote
- Follow-up required before wider rollout: capture the remaining scenario rows for the exact target topology
