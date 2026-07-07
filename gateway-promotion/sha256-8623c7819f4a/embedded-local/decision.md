# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: embedded in-process gateway with mounted auth for thread creation, turn creation, and `/v1/events` turn-result consumption.
- Topology id: embedded-local
- Gateway build: sha256-8623c7819f4a
- Worker builds: embedded-in-process
- Tenant/project scopes: tenant=local-tenant, projects=local-project
- Method families included: `thread/*`, `turn/*`, `item/*`, `account/rateLimits/*`, `mcpServer/startupStatus/*` as observed through the live SSE stream.
- Method families excluded: multi-worker handoff, reconnect recovery, degraded-route fail-closed, account-capacity transition, bounded restoration, slow-client backlog, cleanup/delivery failure.

## Reconciliation Summary

- Route-selection evidence: embedded single-worker capture does not exercise project worker routing; route-selection evidence is required before any multi-worker promotion.
- Account-capacity evidence: live mounted-auth smoke completed without 401 or permission denied; `/healthz` stayed ok with zero pending server requests and empty pending kind counts.
- Bounded restoration evidence: not exercised in this mounted-auth embedded smoke; required before wider promotion.
- Live active-context no-handoff evidence: not exercised in this mounted-auth embedded smoke; required before wider promotion.
- Reconnect/degraded-route evidence: not exercised in this mounted-auth embedded smoke; required before wider promotion.
- Slow-client/backlog evidence: not exercised in this mounted-auth embedded smoke; required before wider promotion.
- Cleanup/delivery-failure evidence: not exercised in this mounted-auth embedded smoke; required before wider promotion.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Async turn result retrieval | Turn creation may return `inProgress`; final answer must be consumed from events/session state | `/v1/events` emitted deltas `gateway`, ` smoke`, ` ok`, then `item/completed` with `gateway smoke ok` and `turn/completed` status `completed` | thread `019f3ad9-35a1-7d82-810c-08636bd0ba09`, turn `019f3ad9-35c4-7721-ba26-5fc4884ee991` | Ensure downstream callers subscribe to `/v1/events` or read session state for final turn output |
| Project-aware account routing | Promotion gate requires `project-aware account routing` coverage and `gateway/projectWorkerRouteSelected` evidence | Embedded mounted-auth smoke did not exercise project worker routing; `scripts/check-gateway-promotion-bundle.sh` stops on the missing coverage requirement | topology `embedded-local`, worker `0`, account `local-account` | Capture a remote or multi-worker route-selection scenario with health, events, metrics, and audit logs |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
