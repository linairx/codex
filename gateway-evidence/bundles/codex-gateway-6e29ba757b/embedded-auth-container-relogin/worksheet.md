# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: codex-gateway-6e29ba757b
- Topology id: embedded-auth-container-relogin
- Worker builds: embedded
- Worker URLs: embedded://in-process
- Account labels: chatgpt-authenticated-relogin
- Auth mode: chatgpt
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local, projects=/tmp/project
- Method matrix route classes covered: project-aware account routing, embedded HTTP thread creation and turn start/completion smoke.

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-thread-turn-completed.txt | healthz/02-thread-turn-completed.json | events/02-thread-turn-completed.sse | metrics/02-thread-turn-completed.json | logs/02-thread-turn-completed.log | HTTP 200; thread returned to idle |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: not applicable: single embedded worker.
- Account-capacity identities agree across health, events, metrics, and logs: rate-limit event reported planType=free and hasCredits=false despite the completed turn.
- Bounded handoff success/failure outcomes match client-visible behavior: not captured.
- Live active-context methods fail closed instead of moving accounts: not captured.
- Backlog and cleanup windows are bounded and observable: not captured; metrics exporter disabled.

## Decision

- Decision: Reject / no promotion
- Promotion scope: authenticated embedded HTTP smoke only.
- Excluded method families or route classes: multi-worker routing, recovery, fail-closed behavior, account handoff, backlog, and cleanup.
- Follow-up required before wider rollout: expose metrics, capture a lifecycle-complete event stream, and eliminate or diagnose model-list refresh timeouts.
