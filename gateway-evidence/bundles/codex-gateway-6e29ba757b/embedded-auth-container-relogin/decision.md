# Gateway Multi-Worker Promotion Decision

- Decision: Reject / no promotion
- Promotion scope: authenticated embedded HTTP smoke only.
- Topology id: embedded-auth-container-relogin
- Gateway build: codex-gateway-6e29ba757b
- Worker builds: embedded
- Tenant/project scopes: tenant=local, projects=/tmp/project
- Method families included: project-aware account routing, embedded HTTP thread creation and turn start/completion smoke.
- Method families excluded: multi-worker routing, recovery, fail-closed behavior, account handoff, backlog, and cleanup.

## Reconciliation Summary

- Route-selection evidence: not applicable: single embedded worker.
- Account-capacity evidence: rate-limit event reported planType=free and hasCredits=false despite the completed turn.
- Bounded restoration evidence: not captured.
- Live active-context no-handoff evidence: not captured.
- Reconnect/degraded-route evidence: not captured; metrics exporter disabled.
- Slow-client/backlog evidence: not captured; metrics exporter disabled.
- Cleanup/delivery-failure evidence: not captured; metrics exporter disabled.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Steady-state bootstrap and thread/turn | completed authenticated turn and lifecycle evidence | HTTP 200 plus thread state changed from active to idle; HTTP SSE contained only rate-limit data | thread=019f5159-d9e7-71e3-942e-0633aa2f7949, turn=019f5159-da6a-7af2-a952-046c3f231162 | Capture lifecycle completion through a supported event surface. |
| Runtime observability | no recurring model-discovery failures and exportable metrics | model-list refresh timed out; `/metrics` returned HTTP 503 because its exporter is disabled | embedded worker 0 | Diagnose the refresh timeout and enable the metrics exporter before promotion. |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
