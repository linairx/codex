# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: embedded in-process gateway with mounted auth for thread create, turn create, and `/v1/events` turn-result consumption.
- Topology id: embedded-local
- Gateway build: sha256-8623c7819f4a
- Worker builds: embedded-in-process, codex-gateway remote mock workers, codex-gateway remote mock workers
- Tenant/project scopes: tenant=default, projects=project-a
- Method families included: `thread/*`, `turn/*`, `item/*`, `account/rateLimits/*`, `mcpServer/startupStatus/*`, project-aware account routing
- Method families excluded: multi-worker handoff, reconnect recovery, degraded-route fail-closed, account-capacity transition, bounded restoration, slow-client backlog, cleanup, and delivery failure remain outside this partial pass.

## Reconciliation Summary

- Route-selection evidence: tenant=default project=project-a selected worker/account identities match health, gateway/projectWorkerRouteSelected events, gateway_project_worker_route_selections metrics, and codex_gateway.audit logs in the verified remote multi-worker route-selection scenario.
- Account-capacity evidence: mounted auth was accepted for a live turn; health stayed ok with zero pending server requests, container logs were empty for the post-turn observation window, and no 401 or permission denied errors were observed.
- Bounded restoration evidence: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Live active-context no-handoff evidence: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Reconnect/degraded-route evidence: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Slow-client/backlog evidence: not exercised in the embedded mounted-auth smoke; required before wider promotion.
- Cleanup/delivery-failure evidence: not exercised in the embedded mounted-auth smoke; required before wider promotion.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Async turn result retrieval | Turn creation may return `inProgress`; final answer must be consumed from events/session state | `/v1/events` emitted deltas `gateway`, ` smoke`, ` ok`, then `item/completed` with `gateway smoke ok` and `turn/completed` status `completed` | thread `019f3ad9-35a1-7d82-810c-08636bd0ba09`, turn `019f3ad9-35c4-7721-ba26-5fc4884ee991` | Ensure downstream callers subscribe to `/v1/events` or read session state for final turn output |
| Remaining multi-worker promotion scenarios | Wider rollout requires route selection plus handoff, reconnect/degraded routing, account-capacity, bounded restoration, active-context no-handoff, backlog, cleanup, and delivery-failure evidence | Project-aware account routing is now covered by the verified remote multi-worker route-selection scenario; the other promotion scenarios remain gap artifacts in rows 5-11 | route-selection tenant `default`, project `project-a`, workers `0`/`1`, accounts `acct-a`/`acct-b` | Capture the remaining rows 5-11 with live or remote multi-worker evidence before promotion |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
