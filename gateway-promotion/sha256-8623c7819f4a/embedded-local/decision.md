# Gateway Multi-Worker Promotion Decision

- Decision: Scoped Stage B
- Promotion scope: embedded in-process gateway with mounted auth for thread create, turn create, `/v1/events` turn-result consumption, and remote mock multi-worker route-class evidence through cleanup and delivery failure.
- Topology id: embedded-local
- Gateway build: sha256-8623c7819f4a
- Worker builds: embedded-in-process, codex-gateway remote mock workers, codex-gateway remote mock workers
- Tenant/project scopes: tenant=default, projects=project-a
- Method families included: `thread/*`, `turn/*`, `item/*`, `account/rateLimits/*`, `mcpServer/startupStatus/*`, project-aware account routing, cleanup and delivery failure
- Method families excluded: none in the captured route-class checklist; release-quality traffic remains scoped until the exact deployment topology is replayed outside the embedded-local/mock evidence bundle.

## Reconciliation Summary

- Route-selection evidence: tenant=default project=project-a selected worker/account identities match health, gateway/projectWorkerRouteSelected events, gateway_project_worker_route_selections metrics, and codex_gateway.audit logs in the verified remote multi-worker route-selection scenario.
- Account-capacity evidence: remote multi-worker tests covered thread/start failover from exhausted account acct-a to replacement account acct-b, account lease cooldown events, and active thread fail-closed behavior when worker 1/account acct-b was exhausted.
- Bounded restoration evidence: remote multi-worker tests covered thread/read restoration from exhausted worker 1/account acct-b to replacement worker 0/account acct-a, and thread/resume fail-closed behavior when no replacement worker restored thread-worker-b.
- Live active-context no-handoff evidence: remote multi-worker tests covered turn/steer and turn/interrupt against an active turn pinned to exhausted worker 1/account acct-b, and both emitted gateway/accountActiveThreadHandoffFailed instead of moving the live turn context.
- Reconnect/degraded-route evidence: remote multi-worker slow-client and cleanup/delivery observability tests covered a server-request backlog on worker 1, client_send_timed_out connection closure, notification send failure accounting, answered server-request delivery failure, failed cleanup rejection delivery, and failed synthesized cleanup resolution delivery.
- Slow-client/backlog evidence: remote multi-worker slow-client and cleanup/delivery observability tests covered a server-request backlog on worker 1, client_send_timed_out connection closure, notification send failure accounting, answered server-request delivery failure, failed cleanup rejection delivery, and failed synthesized cleanup resolution delivery.
- Cleanup/delivery-failure evidence: remote multi-worker slow-client and cleanup/delivery observability tests covered a server-request backlog on worker 1, client_send_timed_out connection closure, notification send failure accounting, answered server-request delivery failure, failed cleanup rejection delivery, and failed synthesized cleanup resolution delivery.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Async turn result retrieval | Turn creation may return `inProgress`; final answer must be consumed from events/session state | `/v1/events` emitted deltas `gateway`, ` smoke`, ` ok`, then `item/completed` with `gateway smoke ok` and `turn/completed` status `completed` | thread `019f3ad9-35a1-7d82-810c-08636bd0ba09`, turn `019f3ad9-35c4-7721-ba26-5fc4884ee991` | Ensure downstream callers subscribe to `/v1/events` or read session state for final turn output |
| Release-quality deployment replay | Wider rollout requires the completed checklist to be replayed on the exact production multi-worker topology | Project-aware account routing, reconnect recovery, degraded-route fail-closed diagnostics, account-capacity transitions, bounded restoration, live active-context no-handoff, slow-client backlog, cleanup, and delivery-failure scenarios are covered by embedded-local plus remote mock evidence | route-selection tenant `default`, project `project-a`, workers `0`/`1`, accounts `acct-a`/`acct-b` | Rerun the completed checklist against the exact production worker URLs, account labels, auth mode, timeout values, and pending-request limits before release-quality promotion |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
