# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: embedded Docker container startup, health, HTTP thread create/read/list, and HTTP turn start admission.
- Topology id: embedded-local-container
- Gateway build: codex-gateway-local-fb1246afee5c
- Worker builds: embedded
- Tenant/project scopes: tenant=local, projects=/tmp/project
- Method families included: HTTP health, HTTP thread create/read/list, HTTP turn start admission
- Method families excluded: v2 WebSocket compatibility, remote worker routing, multi-worker routing, project-aware account routing, account-capacity failover, reconnect, degraded-route fail-closed, slow-client/backlog, cleanup/delivery-failure, metrics backend reconciliation.

## Reconciliation Summary

- Route-selection evidence: not applicable for embedded local container.
- Account-capacity evidence: not applicable for embedded local container.
- Bounded restoration evidence: not exercised.
- Live active-context no-handoff evidence: not exercised.
- Reconnect/degraded-route evidence: not exercised.
- Slow-client/backlog evidence: not exercised.
- Cleanup/delivery-failure evidence: not exercised.
- Deployment evidence: container `codex-gateway-verify` reached Docker `healthy`; `/healthz` returned `status=ok`; thread create/read/list and turn start admission returned HTTP 200.
- Blocking evidence gaps: no local metrics backend was configured, `/v1/events` had no events for the captured embedded HTTP flow, and the model backend returned OpenAI Responses WebSocket 401 errors because credentials were not configured.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Completed turn execution | Turn should complete against a configured model backend | Turn start returned HTTP 200, but gateway logs show repeated OpenAI Responses WebSocket 401 errors | turn `019f07e3-b62c-71d0-bed2-872575717085` | Run with valid model credentials or a controlled mock backend |
| Metrics reconciliation | Metrics exported for the same capture window | No local `codex_otel` metrics backend configured; `/metrics` returned 404 by design | embedded-local-container | Configure metrics export or query the deployment metrics backend |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
