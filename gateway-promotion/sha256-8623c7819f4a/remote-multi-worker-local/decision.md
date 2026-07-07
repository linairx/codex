# Gateway Multi-Worker Promotion Decision

- Decision: Scoped Stage B
- Promotion scope: remote multi-worker local health plus project-aware routing smoke
- Topology id: remote-multi-worker-local
- Gateway build: sha256-8623c7819f4a
- Worker builds: sha256-ddb5ca60b296, sha256-ddb5ca60b296
- Tenant/project scopes: tenant=tenant-a, projects=project-a, project-b
- Method families included: multi-worker health baseline, account-label completeness, worker-pool leases, project-aware account routing, HTTP thread list/create/read, HTTP turn start acceptance
- Method families excluded: reconnect/degraded routing, bounded account handoff, live active-context fail-closed, slow-client backlog, cleanup delivery failure, full model turn completion with authenticated upstream OpenAI responses

## Reconciliation Summary

- Route-selection evidence: health records project-a on worker 0 / acct-a and project-b on worker 1 / acct-b; events, metrics, and audit logs include project-a route selection.
- Account-capacity evidence: startup health and audit logs show two labeled healthy worker slots leased to acct-a and acct-b; dynamic capacity transition remains scoped follow-up.
- Bounded restoration evidence: bounded restoration was not exercised in this local Stage B capture and remains follow-up.
- Live active-context no-handoff evidence: active-context no-handoff behavior was not exercised in this local Stage B capture and remains follow-up.
- Reconnect/degraded-route evidence: reconnect, slow-client backlog, and cleanup delivery-failure windows were not exercised in this local Stage B capture and remain follow-up.
- Slow-client/backlog evidence: reconnect, slow-client backlog, and cleanup delivery-failure windows were not exercised in this local Stage B capture and remain follow-up.
- Cleanup/delivery-failure evidence: reconnect, slow-client backlog, and cleanup delivery-failure windows were not exercised in this local Stage B capture and remain follow-up.
- Account-label evidence: health records two healthy labeled workers and `remoteAccountLabelsComplete=true`.
- Worker-pool evidence: health records two leased accounts, two bound worker slots, and two healthy worker slots.
- Event evidence: the project-a SSE stream contains `gateway/projectWorkerRouteSelected`; audit logs contain project-route selection for both project-a and project-b.
- Turn evidence: both project-scoped HTTP turn starts returned `inProgress`; downstream model completion was outside scope because the local workers lacked upstream OpenAI credentials.

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Direct Docker DNS worker URLs with auth | Authenticated remote workers reachable as `ws://codex-gateway-mw-worker-*:9001/v2` | Gateway refuses to send auth tokens over non-loopback `ws://` URLs | workers=0,1 accounts=acct-a,acct-b | Use `wss://`, loopback transport, or another deployment-native secure worker URL |
| Full model turn completion | Turns complete through upstream OpenAI responses | Local workers returned upstream 401 retry events because no OpenAI bearer credentials were configured | threads=019f3d59-5f41-7fa1-9a3c-1c10698d0871, 019f3d59-5fa6-7330-8628-cb00afe33774 | Repeat with authenticated upstream model traffic |
| Telemetry metrics | Gateway metrics exported for the capture window | Metrics files contain local health/log reconciliation because this local gateway has no `/metrics` endpoint | topology=remote-multi-worker-local | Export metrics from the configured telemetry backend |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode, timeout values, pending-request limits, and method families listed above.
