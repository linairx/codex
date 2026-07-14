# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: embedded Compose deployment smoke and baseline HTTP thread/turn admission only
- Topology id: embedded-compose-18081
- Gateway build: codex-gateway-6e29ba757b
- Worker builds: embedded
- Tenant/project scopes: tenant=local, projects=/tmp/project
- Method families included: healthz, HTTP thread creation, HTTP turn admission
- Method families excluded: completed model turns, remote-worker routing, multi-worker account routing, reconnect, degraded routes, bounded restoration, slow-client, and cleanup scenarios

## Reconciliation Summary

- Route-selection evidence: not applicable to embedded topology
- Account-capacity evidence: not applicable to embedded topology
- Bounded restoration evidence: not captured
- Live active-context no-handoff evidence: not captured
- Reconnect/degraded-route evidence: not captured
- Slow-client/backlog evidence: not captured
- Cleanup/delivery-failure evidence: gateway accepted the turn, but the model backend returned HTTP 401 before a completed turn could be observed

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Steady-state bootstrap and thread/turn | A completed model-backed turn | Gateway returned HTTP 200 with an in-progress turn, then the model backend returned HTTP 401 | tenant=local, project=/tmp/project | Configure valid model credentials and repeat the complete traffic evidence window |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
