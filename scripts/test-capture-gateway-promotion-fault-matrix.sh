#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
trap 'rm -rf "$bundle_dir"' EXIT

capture_time=2026-06-11T10:04:00Z

help_output="$("$repo_root/scripts/capture-gateway-promotion-fault-matrix.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-fault-matrix-capture -- --bundle DIR'

replace_scenario_row() {
  path=$1
  scenario=$2
  replacement=$3
  tmp=$(mktemp)
  awk -v scenario="$scenario" -v replacement="$replacement" '
    BEGIN { replaced = 0 }
    $0 ~ "^\\| " scenario " \\|" {
      print replacement
      replaced = 1
      next
    }
    { print }
    END {
      if (!replaced) {
        print "missing scenario row: " scenario > "/dev/stderr"
        exit 1
      }
    }
  ' "$path" > "$tmp"
  mv "$tmp" "$path"
}

metadata() {
  cat <<EOF
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: 0
account id: acct-a
capture time: $capture_time
EOF
}

write_artifact_set() {
  bundle=$1
  row_number=$2
  prefix=$3
  scenario=$4
  result=$5

  {
    metadata
    printf '%s\n' "client transcript for $scenario"
  } > "$bundle/transcripts/$prefix.txt"
  {
    metadata
    printf '%s\n' '{"projectWorkerRoutes":[{"tenantId":"tenant-a","projectId":"project-a","workerId":0,"accountId":"acct-a","accountRoutingEligible":true}]}'
  } > "$bundle/healthz/$prefix.json"
  {
    metadata
    printf '%s\n' 'event: gateway/projectWorkerRouteSelected'
    printf '%s\n' 'data: {"tenantId":"tenant-a","projectId":"project-a","workerId":0,"accountId":"acct-a"}'
  } > "$bundle/events/$prefix.sse"
  {
    metadata
    printf '%s\n' '{"metric":"gateway_project_worker_route_selections","tenant_id":"tenant-a","project_id":"project-a","account_id":"acct-a","value":1}'
  } > "$bundle/metrics/$prefix.json"
  {
    metadata
    printf '%s\n' 'codex_gateway.audit gateway project worker route selected tenant_id=tenant-a project_id=project-a account_id=acct-a'
  } > "$bundle/logs/$prefix.log"

  replace_scenario_row \
    "$bundle/README.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | worksheet row $row_number |"
  replace_scenario_row \
    "$bundle/worksheet.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | $result |"
}

prepare_bundle() {
  output=$1
  bundle_path="$("$repo_root/scripts/create-gateway-promotion-bundle.sh" \
    --output "$output" \
    --gateway-build gw-test \
    --topology-id topo-test \
    --worker-build worker-a \
    --worker-url ws://127.0.0.1:9001/v2 \
    --account-id acct-a \
    --tenant-id tenant-a \
    --project-id project-a \
    --auth-mode bearer \
    --v2-initialize-timeout-seconds 30 \
    --v2-client-send-timeout-seconds 10 \
    --v2-reconnect-retry-backoff-seconds 1 \
    --v2-max-pending-server-requests 64 \
    --v2-max-pending-client-requests 64)"

  awk -v capture_time="$capture_time" '
    /^- Captured by:/ { print "- Captured by: test-runner"; next }
    /^- Capture start:/ { print "- Capture start: " capture_time; next }
    /^- Capture end:/ { print "- Capture end: " capture_time; next }
    { print }
  ' "$bundle_path/README.md" > "$bundle_path/README.md.tmp"
  mv "$bundle_path/README.md.tmp" "$bundle_path/README.md"

  cat > "$bundle_path/worksheet.md" <<EOF
# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: gw-test
- Topology id: topo-test
- Worker builds: worker-a
- Worker URLs: ws://127.0.0.1:9001/v2
- Account labels: acct-a
- Auth mode: bearer
- v2 timeout values: initialize=30, client_send=10, reconnect_backoff=1
- Pending request limits: server=64, client=64
- Tenant/project scope: tenant=tenant-a, projects=project-a
- Method matrix route classes covered: project-aware account routing, account capacity, bounded restoration, live active-context, reconnect recovery, backlog cleanup

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
| Baseline before traffic | | | | | | |
| Steady-state bootstrap and thread/turn | | | | | | |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit: targeted capture identities match tenant-a/project-a/acct-a
- Account-capacity identities agree across health, events, metrics, and logs: targeted tests cover exhausted and eligible account transitions
- Bounded handoff success/failure outcomes match client-visible behavior: targeted tests cover bounded handoff outcomes
- Live active-context methods fail closed instead of moving accounts: targeted tests cover active context fail-closed behavior
- Backlog and cleanup windows are bounded and observable: targeted tests cover reconnect, backlog, timeout, and cleanup accounting

## Decision

- Decision: Scoped Stage B
- Promotion scope: tenant=tenant-a, projects=project-a
- Excluded method families or route classes: wider production rollout
- Follow-up required before wider rollout: replay against a live gateway canary
EOF

  cat > "$bundle_path/decision.md" <<EOF
# Gateway Multi-Worker Promotion Decision

- Decision: Scoped Stage B
- Promotion scope: tenant=tenant-a, projects=project-a
- Topology id: topo-test
- Gateway build: gw-test
- Worker builds: worker-a
- Tenant/project scopes: tenant=tenant-a, projects=project-a
- Method families included: project-aware account routing, account capacity, bounded restoration, live active-context, reconnect recovery, backlog cleanup
- Method families excluded: wider production rollout

## Reconciliation Summary

- Route-selection evidence: targeted capture identities match tenant-a/project-a/acct-a
- Account-capacity evidence: targeted tests cover exhausted and eligible account transitions
- Bounded restoration evidence: targeted tests cover bounded handoff outcomes
- Live active-context no-handoff evidence: targeted tests cover active context fail-closed behavior
- Reconnect/degraded-route evidence: targeted tests cover reconnect, backlog, timeout, and cleanup accounting
- Slow-client/backlog evidence: targeted tests cover reconnect, backlog, timeout, and cleanup accounting
- Cleanup/delivery-failure evidence: targeted tests cover reconnect, backlog, timeout, and cleanup accounting

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| Scoped Stage B | deterministic targeted harness | live canary replay still required | tenant-a/project-a/acct-a | run live gateway replay before wider rollout |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
EOF

  write_artifact_set "$bundle_path" 1 01-baseline "Baseline before traffic" captured
  write_artifact_set "$bundle_path" 2 02-steady-state-project-a "Steady-state bootstrap and thread/turn" captured
  write_artifact_set "$bundle_path" 3 03-project-route-selection "Project route selection" captured
  printf '%s\n' "$bundle_path"
}

bundle_path=$(prepare_bundle "$bundle_dir/direct")
"$repo_root/scripts/capture-gateway-promotion-fault-matrix.sh" \
  --bundle "$bundle_path" \
  --capture-time "$capture_time" \
  --skip-tests >/dev/null

grep -Fq 'skipped test execution by --skip-tests' "$bundle_path/transcripts/04-reconnect-recovery.txt"
grep -Fq '"testFilter": "websocket_upgrade_reconnects_missing_worker_before_turn_start"' "$bundle_path/metrics/04-reconnect-recovery.json"
grep -Fq 'event: gateway/promotionFaultMatrix' "$bundle_path/events/10-cleanup-delivery-failure.sse"
grep -Fq '| Reconnect and recovery | transcripts/04-reconnect-recovery.txt | healthz/04-reconnect-recovery.json | events/04-reconnect-recovery.sse | metrics/04-reconnect-recovery.json | logs/04-reconnect-recovery.log | worksheet row 4 |' "$bundle_path/README.md"
grep -Fq '| Cleanup or delivery failure | transcripts/10-cleanup-delivery-failure.txt | healthz/10-cleanup-delivery-failure.json | events/10-cleanup-delivery-failure.sse | metrics/10-cleanup-delivery-failure.json | logs/10-cleanup-delivery-failure.log | targeted test skipped |' "$bundle_path/worksheet.md"

just_bundle_path=$(prepare_bundle "$bundle_dir/just")
just gateway-promotion-fault-matrix-capture -- \
  --bundle "$just_bundle_path" \
  --capture-time "$capture_time" \
  --skip-tests >/dev/null
grep -Fq 'gateway/accountActiveThreadHandoffFailed' "$just_bundle_path/events/08-live-active-context-no-handoff.sse"
