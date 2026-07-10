# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: local embedded Docker startup, /healthz, and basic HTTP thread/turn flow
- Topology id: embedded-local-container
- Gateway build: sha256-5963ac38f7d9
- Worker builds: embedded-in-process
- Tenant/project scopes: tenant=local, projects=/tmp/project
- Method families included: embedded HTTP health, thread create/read/list, turn start
- Method families excluded: project routing, reconnect, degraded route, account capacity, bounded restoration, live active-context no-handoff, backlog, cleanup, multi-worker remote

## Reconciliation Summary

- Route-selection evidence: not exercised in this embedded smoke bundle
- Account-capacity evidence: not exercised in this embedded smoke bundle
- Bounded restoration evidence: not exercised in this embedded smoke bundle
- Live active-context no-handoff evidence: not exercised in this embedded smoke bundle
- Reconnect/degraded-route evidence: not exercised in this embedded smoke bundle
- Slow-client/backlog evidence: not exercised in this embedded smoke bundle
- Cleanup/delivery-failure evidence: not exercised in this embedded smoke bundle

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| remaining promotion scenarios | full promotion evidence | only baseline and steady-state smoke captured | sha256-5963ac38f7d9, embedded-local-container | capture remaining scenarios before wider rollout |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
