# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: sha256-8623c7819f4a
- Topology id: embedded-local
- Worker builds: embedded-in-process
- Worker URLs: embedded://in-process
- Account labels: local-account
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local-tenant, projects=local-project
- Method matrix route classes covered: `thread/*`, `turn/*`, `item/*`, `account/rateLimits/*`, `mcpServer/startupStatus/*` as observed through the live SSE stream.

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |
| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-project-a.txt | healthz/02-steady-state-project-a.json | events/02-steady-state-project-a.sse | metrics/02-steady-state-project-a.json | logs/02-steady-state-project-a.log | captured health |
| Mounted-auth live turn and event consumption | transcripts/03-auth-mounted-live-smoke.txt | healthz/03-auth-mounted-live-smoke.json | events/03-auth-mounted-live-smoke.sse | metrics/03-auth-mounted-live-smoke.json | logs/03-auth-mounted-live-smoke.log | passed |
| Project route selection | transcripts/04-project-route-selection.txt | healthz/04-project-route-selection.json | events/04-project-route-selection.sse | metrics/04-project-route-selection.json | logs/04-project-route-selection.log | gap: not captured |
| Reconnect and recovery | transcripts/05-reconnect-and-recovery.txt | healthz/05-reconnect-and-recovery.json | events/05-reconnect-and-recovery.sse | metrics/05-reconnect-and-recovery.json | logs/05-reconnect-and-recovery.log | gap: not captured |
| Degraded-route fail-closed | transcripts/06-degraded-route-fail-closed.txt | healthz/06-degraded-route-fail-closed.json | events/06-degraded-route-fail-closed.sse | metrics/06-degraded-route-fail-closed.json | logs/06-degraded-route-fail-closed.log | gap: not captured |
| Account-capacity transition | transcripts/07-account-capacity-transition.txt | healthz/07-account-capacity-transition.json | events/07-account-capacity-transition.sse | metrics/07-account-capacity-transition.json | logs/07-account-capacity-transition.log | gap: not captured |
| Bounded restoration | transcripts/08-bounded-restoration.txt | healthz/08-bounded-restoration.json | events/08-bounded-restoration.sse | metrics/08-bounded-restoration.json | logs/08-bounded-restoration.log | gap: not captured |
| Live active-context no-handoff | transcripts/09-live-active-context-no-handoff.txt | healthz/09-live-active-context-no-handoff.json | events/09-live-active-context-no-handoff.sse | metrics/09-live-active-context-no-handoff.json | logs/09-live-active-context-no-handoff.log | gap: not captured |
| Slow-client or backlog window | transcripts/10-slow-client-backlog-window.txt | healthz/10-slow-client-backlog-window.json | events/10-slow-client-backlog-window.sse | metrics/10-slow-client-backlog-window.json | logs/10-slow-client-backlog-window.log | gap: not captured |
| Cleanup or delivery failure | transcripts/11-cleanup-delivery-failure.txt | healthz/11-cleanup-delivery-failure.json | events/11-cleanup-delivery-failure.sse | metrics/11-cleanup-delivery-failure.json | logs/11-cleanup-delivery-failure.log | gap: not captured |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: embedded single-worker capture does not exercise project worker routing; wider promotion still requires multi-worker route-selection evidence.
- Account-capacity identities agree across health, events, metrics, and logs: mounted auth was accepted for a live turn; health stayed ok with zero pending server requests, container logs were empty for the post-turn observation window, and no 401 or permission denied errors were observed.
- Bounded handoff success/failure outcomes match client-visible behavior: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Live active-context methods fail closed instead of moving accounts: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Backlog and cleanup windows are bounded and observable: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Async turn consumption path: `/v1/events` emitted `item/agentMessage/delta` fragments that reconstructed `gateway smoke ok`, followed by `item/completed` and `turn/completed` for turn `019f3ad9-35c4-7721-ba26-5fc4884ee991`.

## Decision

- Decision: Reject / no promotion
- Promotion scope: embedded in-process gateway with mounted auth for thread create, turn create, and `/v1/events` turn-result consumption.
- Excluded method families or route classes: multi-worker handoff, reconnect recovery, degraded-route fail-closed, account-capacity transition, bounded restoration, slow-client backlog, cleanup, and delivery failure remain outside this partial pass.
- Follow-up required before wider rollout: implement or document downstream consumers around `/v1/events` for async turn results rather than relying on the turn creation response to contain the final answer.
