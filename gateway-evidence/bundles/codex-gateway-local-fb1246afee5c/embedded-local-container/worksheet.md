# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: codex-gateway-local-fb1246afee5c
- Topology id: embedded-local-container
- Worker builds: embedded
- Worker URLs: embedded://in-process
- Account labels: local
- Auth mode: none
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=local, projects=/tmp/project
- Method matrix route classes covered: HTTP health, HTTP thread create/read/list, HTTP turn start admission

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | n/a | healthz/embedded-baseline-before-traffic.json | n/a | metrics/embedded-local-metrics-not-configured.txt | logs/embedded-container.log | Pass: container was healthy and `/healthz.status` was `ok` before client traffic |
| Steady-state bootstrap and thread/turn | transcripts/embedded-thread-create.json, transcripts/embedded-thread-read.json, transcripts/embedded-thread-list.json, transcripts/embedded-turn-start.txt | healthz/embedded-after-traffic.json | events/embedded-after-traffic.sse | metrics/embedded-local-metrics-not-configured.txt | logs/embedded-container.log | Partial pass: gateway accepted thread create/read/list and turn start; model backend later logged OpenAI 401 because credentials were not configured |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: not applicable to embedded single-process validation.
- Account-capacity identities agree across health, events, metrics, and logs: not applicable to embedded single-process validation.
- Bounded handoff success/failure outcomes match client-visible behavior: not exercised.
- Live active-context methods fail closed instead of moving accounts: not exercised.
- Backlog and cleanup windows are bounded and observable: not exercised.
- Metrics reconciliation: blocked for this local run because no `codex_otel` metrics backend was configured; `/metrics` is not an HTTP route.
- Model backend reconciliation: turn start was accepted, but logs show repeated `wss://api.openai.com/v1/responses` 401 responses because the local container did not have valid model credentials.

## Decision

- Decision: Reject / no promotion
- Promotion scope: packaged Docker gateway starts, reports healthy, and accepts baseline HTTP thread/turn traffic in embedded mode.
- Excluded method families or route classes: v2 WebSocket real-client flow, remote worker routing, multi-worker routing, project route selection, account capacity, reconnect, degraded-route, slow-client, backlog, cleanup, metrics backend reconciliation.
- Follow-up required before wider rollout: configure model credentials for completed turn evidence, configure or query a `codex_otel` metrics backend, then repeat the same capture flow for the built-in single-worker remote Compose profile.
