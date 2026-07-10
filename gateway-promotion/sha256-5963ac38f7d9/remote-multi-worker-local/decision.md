# Gateway Multi-Worker Promotion Decision

- Decision: Scoped Stage B
- Promotion scope: local Docker multi-worker project-aware routing smoke for tenant-a project-a/project-b
- Topology id: remote-multi-worker-local
- Gateway build: sha256-5963ac38f7d9
- Worker builds: sha256-ddb5ca60b296, sha256-ddb5ca60b296
- Tenant/project scopes: tenant=tenant-a, projects=project-a, project-b
- Method families included: project-aware account routing, multi-worker HTTP thread create/read/list, turn start, reconnect recovery, degraded-route fail-closed, account-capacity transition, bounded restoration, live active-context no-handoff, slow-client backlog, cleanup delivery failure
- Method families excluded: wider production rollout

## Reconciliation Summary

- Route-selection evidence: tenant-a/project-a routes to worker 0 acct-a and tenant-a/project-b routes to worker 1 acct-b in captured health and transcripts
- Account-capacity evidence: targeted account-capacity transition test passed with exhausted-account retry evidence
- Bounded restoration evidence: targeted bounded restoration test passed with replacement-worker handoff evidence
- Live active-context no-handoff evidence: targeted active-context no-handoff test passed with handoff-failure event evidence
- Reconnect/degraded-route evidence: targeted reconnect, degraded-route, slow-client backlog, and cleanup delivery-failure tests passed
- Slow-client/backlog evidence: targeted reconnect, degraded-route, slow-client backlog, and cleanup delivery-failure tests passed
- Cleanup/delivery-failure evidence: targeted reconnect, degraded-route, slow-client backlog, and cleanup delivery-failure tests passed

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| live canary replay | targeted fault-matrix evidence should reproduce against live gateway canary | local targeted tests passed; live gateway canary replay remains pending | sha256-5963ac38f7d9, remote-multi-worker-local | replay 04-10 fault matrix against live gateway canary before wider rollout |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
