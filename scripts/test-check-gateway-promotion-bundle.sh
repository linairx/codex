#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
trap 'rm -rf "$bundle_dir"' EXIT

check_help_output="$("$repo_root/scripts/check-gateway-promotion-bundle.sh" --help 2>&1)"
printf '%s\n' "$check_help_output" | grep -Fq 'just gateway-promotion-bundle-check -- BUNDLE_DIR'

bundle_path="$("$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-test \
  --worker-build worker-a \
  --worker-build worker-b \
  --worker-url ws://127.0.0.1:9001/v2 \
  --account-id acct-a \
  --worker-url ws://127.0.0.1:9002/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64)"

README_MD="$bundle_path/README.md"
WORKSHEET_MD="$bundle_path/worksheet.md"
DECISION_MD="$bundle_path/decision.md"

perl -0pi -e 'my $row = 0; s#\| ([^|]+) \| \| \| \| \| \| \|#sprintf("| %s | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row %d |", $1, ++$row)#ge' \
  "$README_MD"
perl -0pi -e 's#\| ([^|]+) \| \| \| \| \| \| \|#| $1 | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#g' \
  "$WORKSHEET_MD"
sed -i \
  -e 's#^- Captured by:$#- Captured by: test-runner#' \
  -e 's#^- Tenant/project scope: tenant=tenant-a, projects=project-a$#- Tenant/project scope: tenant=tenant-a, projects=project-a#' \
  -e 's#^- Capture start:$#- Capture start: 2026-06-11T10:00:00Z#' \
  -e 's#^- Capture end:$#- Capture end: 2026-06-11T10:05:00Z#' \
  -e 's#^- Decision file: decision.md$#- Decision file: decision.md#' \
  "$README_MD"
grep -Fq -- '- Captured by: test-runner' "$README_MD"
grep -Fq -- '- Capture start: 2026-06-11T10:00:00Z' "$README_MD"
grep -Fq -- '- Capture end: 2026-06-11T10:05:00Z' "$README_MD"
grep -Fq -- '- Decision file: decision.md' "$README_MD"
sed -i \
  -e 's#| | | | | |#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#g' \
  "$DECISION_MD"
sed -i \
  -e 's#^- Method matrix route classes covered:$#- Method matrix route classes covered: bootstrap discovery, project-aware account routing#' \
  -e 's#^- Route-selection identities agree across health, events, metrics, and audit:$#- Route-selection identities agree across health, events, metrics, and audit: matches health, events, metrics, and audit for project-a#' \
  -e 's#^- Account-capacity identities agree across health, events, metrics, and logs:$#- Account-capacity identities agree across health, events, metrics, and logs: matches worker health, account-capacity events, and logs for acct-a#' \
  -e 's#^- Bounded handoff success/failure outcomes match client-visible behavior:$#- Bounded handoff success/failure outcomes match client-visible behavior: not exercised in this bundle#' \
  -e 's#^- Live active-context methods fail closed instead of moving accounts:$#- Live active-context methods fail closed instead of moving accounts: not exercised in this bundle#' \
  -e 's#^- Backlog and cleanup windows are bounded and observable:$#- Backlog and cleanup windows are bounded and observable: not exercised in this bundle#' \
  -e 's#^- Decision:$#- Decision: Scoped Stage B#' \
  -e 's#^- Promotion scope:$#- Promotion scope: multi-worker remote evidence bundle#' \
  -e 's#^- Excluded method families or route classes:$#- Excluded method families or route classes: live active-context migration#' \
  -e 's#^- Follow-up required before wider rollout:$#- Follow-up required before wider rollout: not exercised in this bundle#' \
  "$WORKSHEET_MD"
sed -i \
  -e 's#^- Decision:$#- Decision: Scoped Stage B#' \
  -e 's#^- Promotion scope:$#- Promotion scope: multi-worker remote evidence bundle#' \
  -e 's#^- Topology id:$#- Topology id: topo-test#' \
  -e 's#^- Gateway build:$#- Gateway build: gw-test#' \
  -e 's#^- Worker builds:$#- Worker builds: worker-a, worker-b#' \
  -e 's#^- Tenant/project scopes:$#- Tenant/project scopes: tenant=tenant-a, project=project-a#' \
  -e 's#^- Method families included:$#- Method families included: bootstrap discovery, project-aware account routing#' \
  -e 's#^- Method families excluded:$#- Method families excluded: live active-context migration#' \
  -e 's#^- Route-selection evidence:$#- Route-selection evidence: matches health, events, metrics, and audit for project-a#' \
  -e 's#^- Account-capacity evidence:$#- Account-capacity evidence: matches worker health, account-capacity events, and logs for acct-a#' \
  -e 's#^- Bounded restoration evidence:$#- Bounded restoration evidence: not exercised in this bundle#' \
  -e 's#^- Live active-context no-handoff evidence:$#- Live active-context no-handoff evidence: not exercised in this bundle#' \
  -e 's#^- Reconnect/degraded-route evidence:$#- Reconnect/degraded-route evidence: not exercised in this bundle#' \
  -e 's#^- Slow-client/backlog evidence:$#- Slow-client/backlog evidence: not exercised in this bundle#' \
  -e 's#^- Cleanup/delivery-failure evidence:$#- Cleanup/delivery-failure evidence: not exercised in this bundle#' \
  "$DECISION_MD"
grep -Fq -- '- Decision: Scoped Stage B' "$DECISION_MD"
grep -Fq -- '- Topology id: topo-test' "$DECISION_MD"
grep -Fq -- '- Route-selection evidence: matches health, events, metrics, and audit for project-a' "$DECISION_MD"

cat > "$bundle_path/transcripts/01-baseline.txt" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
sample transcript
EOF
cat > "$bundle_path/healthz/01-baseline.json" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
{"ok":true}
EOF
cat > "$bundle_path/events/01-baseline.sse" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
event: gateway/projectWorkerRouteSelected
EOF
cat > "$bundle_path/metrics/01-baseline.json" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
{"metric":"sample"}
EOF
cat > "$bundle_path/logs/01-baseline.log" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
sample log line
EOF

"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null
just gateway-promotion-bundle-check -- "$bundle_path" >/dev/null

for path in \
  "$bundle_path/transcripts/01-baseline.txt" \
  "$bundle_path/healthz/01-baseline.json" \
  "$bundle_path/events/01-baseline.sse" \
  "$bundle_path/metrics/01-baseline.json" \
  "$bundle_path/logs/01-baseline.log"; do
  perl -0pi -e 's#worker build: worker-a#worker build: worker-b#' "$path"
  perl -0pi -e 's#account id: acct-a#account id: none#' "$path"
done
"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null
for path in \
  "$bundle_path/transcripts/01-baseline.txt" \
  "$bundle_path/healthz/01-baseline.json" \
  "$bundle_path/events/01-baseline.sse" \
  "$bundle_path/metrics/01-baseline.json" \
  "$bundle_path/logs/01-baseline.log"; do
  perl -0pi -e 's#account id: none#account id: <none>#' "$path"
done
"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null
for path in \
  "$bundle_path/transcripts/01-baseline.txt" \
  "$bundle_path/healthz/01-baseline.json" \
  "$bundle_path/events/01-baseline.sse" \
  "$bundle_path/metrics/01-baseline.json" \
  "$bundle_path/logs/01-baseline.log"; do
  perl -0pi -e 's#worker build: worker-b#worker build: worker-a#' "$path"
  perl -0pi -e 's#account id: <none>#account id: acct-a#' "$path"
done

perl -0pi -e 's#\| 1 \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 0 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected duplicate topology worker id to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| 0 \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 1 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"

perl -0pi -e 's#\| 1 \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| worker b | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid worker id format to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| worker b \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 1 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"

perl -0pi -e 's#\| 1 \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 1 | TODO-worker | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected placeholder topology row to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| 1 \| TODO-worker \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 1 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision: experimental maybe#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid decision taxonomy value to fail" >&2
  exit 1
fi
sed -i 's#^- Decision: experimental maybe$#- Decision: Scoped Stage B#' "$DECISION_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision: TBD#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected placeholder decision value to fail" >&2
  exit 1
fi
sed -i 's#^- Decision: TBD$#- Decision: Scoped Stage B#' "$DECISION_MD"

sed -i 's#^- Capture end: 2026-06-11T10:05:00Z$#- Capture end: not-a-timestamp#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid capture end timestamp to fail" >&2
  exit 1
fi
sed -i 's#^- Capture end: not-a-timestamp$#- Capture end: 2026-06-11T10:05:00Z#' "$README_MD"

sed -i 's#^- Capture start: 2026-06-11T10:00:00Z$#- Capture start: 2026-02-30T10:00:00Z#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid capture start date to fail" >&2
  exit 1
fi
sed -i 's#^- Capture start: 2026-02-30T10:00:00Z$#- Capture start: 2026-06-11T10:00:00Z#' "$README_MD"

sed -i 's#^- Capture end: 2026-06-11T10:05:00Z$#- Capture end: 2026-06-11T09:59:59Z#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected inverted capture window to fail" >&2
  exit 1
fi
sed -i 's#^- Capture end: 2026-06-11T09:59:59Z$#- Capture end: 2026-06-11T10:05:00Z#' "$README_MD"

sed -i 's#^- Capture end: 2026-06-11T10:05:00Z$#- Capture end: 2026-02-30T10:05:00Z#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid capture end date to fail" >&2
  exit 1
fi
sed -i 's#^- Capture end: 2026-02-30T10:05:00Z$#- Capture end: 2026-06-11T10:05:00Z#' "$README_MD"

sed -i 's#^- Capture end: 2026-06-11T10:05:00Z$#- Capture end: 2026-06-11T09:59:59Z#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture end before capture start to fail" >&2
  exit 1
fi
sed -i 's#^- Capture end: 2026-06-11T09:59:59Z$#- Capture end: 2026-06-11T10:05:00Z#' "$README_MD"

perl -0pi -e 's#worksheet row 1#capture entry 1#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid worksheet row reference to fail" >&2
  exit 1
fi
perl -0pi -e 's#capture entry 1#worksheet row 1#' "$README_MD"

sed -i 's#^- Northbound auth: bearer$#- Northbound auth:#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing runtime auth to fail" >&2
  exit 1
fi
sed -i 's#^- Northbound auth:$#- Northbound auth: bearer#' "$README_MD"

sed -i 's#^- Tenant/project scope: tenant=tenant-a, projects=project-a$#- Tenant/project scope:#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing README tenant/project scope to fail" >&2
  exit 1
fi
sed -i 's#^- Tenant/project scope:$#- Tenant/project scope: tenant=tenant-a, projects=project-a#' "$README_MD"

sed -i 's#^- Tenant/project scope: tenant=tenant-a, projects=project-a$#- Tenant/project scope: tenant=tenant-a, projects=project-b#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched worksheet tenant/project scope to fail" >&2
  exit 1
fi
sed -i 's#^- Tenant/project scope: tenant=tenant-a, projects=project-b$#- Tenant/project scope: tenant=tenant-a, projects=project-a#' "$WORKSHEET_MD"

sed -i 's#^- Tenant/project scopes: tenant=tenant-a, projects=project-a$#- Tenant/project scopes: tenant=tenant-a, projects=project-b#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision tenant/project scope to fail" >&2
  exit 1
fi
sed -i 's#^- Tenant/project scopes: tenant=tenant-a, projects=project-b$#- Tenant/project scopes: tenant=tenant-a, projects=project-a#' "$DECISION_MD"

sed -i 's#^- Gateway build: gw-test$#- Gateway build: gw-alt#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision gateway build to fail" >&2
  exit 1
fi
sed -i 's#^- Gateway build: gw-alt$#- Gateway build: gw-test#' "$DECISION_MD"

sed -i 's#^- Topology id: topo-test$#- Topology id: topo-alt#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched worksheet topology id to fail" >&2
  exit 1
fi
sed -i 's#^- Topology id: topo-alt$#- Topology id: topo-test#' "$WORKSHEET_MD"

sed -i 's#^- Promotion scope: multi-worker remote evidence bundle$#- Promotion scope: scoped Stage B bundle#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision promotion scope to fail" >&2
  exit 1
fi
sed -i 's#^- Promotion scope: scoped Stage B bundle$#- Promotion scope: multi-worker remote evidence bundle#' "$DECISION_MD"

sed -i 's#^- Worker builds: worker-a, worker-b$#- Worker builds: worker-a, worker-c#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched worksheet worker builds to fail" >&2
  exit 1
fi
sed -i 's#^- Worker builds: worker-a, worker-c$#- Worker builds: worker-a, worker-b#' "$WORKSHEET_MD"

sed -i 's#^- Worker builds: worker-a, worker-b$#- Worker builds:#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing README worker builds to fail" >&2
  exit 1
fi
sed -i 's#^- Worker builds:$#- Worker builds: worker-a, worker-b#' "$README_MD"

sed -i 's#^- Worker builds: worker-a, worker-b$#- Worker builds: worker-a, worker-c#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched README worker builds to fail" >&2
  exit 1
fi
sed -i 's#^- Worker builds: worker-a, worker-c$#- Worker builds: worker-a, worker-b#' "$README_MD"

sed -i 's#^- Worker builds: worker-a, worker-b$#- Worker builds: worker-a, worker-c#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision worker builds to fail" >&2
  exit 1
fi
sed -i 's#^- Worker builds: worker-a, worker-c$#- Worker builds: worker-a, worker-b#' "$DECISION_MD"

sed -i 's#^- Excluded method families or route classes: live active-context migration$#- Excluded method families or route classes: bounded account handoff#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched worksheet excluded method families to fail" >&2
  exit 1
fi
sed -i 's#^- Excluded method families or route classes: bounded account handoff$#- Excluded method families or route classes: live active-context migration#' "$WORKSHEET_MD"

sed -i 's#^- Method families excluded: live active-context migration$#- Method families excluded: bounded account handoff#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision excluded method families to fail" >&2
  exit 1
fi
sed -i 's#^- Method families excluded: bounded account handoff$#- Method families excluded: live active-context migration#' "$DECISION_MD"

sed -i 's#^- Method families included: bootstrap discovery, project-aware account routing$#- Method families included: bootstrap discovery, worker discovery#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched decision method families included to fail" >&2
  exit 1
fi
sed -i 's#^- Method families included: bootstrap discovery, worker discovery$#- Method families included: bootstrap discovery, project-aware account routing#' "$DECISION_MD"

sed -i 's#^- Route-selection evidence: matches health, events, metrics, and audit for project-a$#- Route-selection evidence: mismatched route-selection evidence#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched route-selection evidence to fail" >&2
  exit 1
fi
sed -i 's#^- Route-selection evidence: mismatched route-selection evidence$#- Route-selection evidence: matches health, events, metrics, and audit for project-a#' "$DECISION_MD"

sed -i 's#^- Worker URLs: ws://127.0.0.1:9001/v2, ws://127.0.0.1:9002/v2$#- Worker URLs:#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing worksheet worker URLs to fail" >&2
  exit 1
fi
sed -i 's#^- Worker URLs:$#- Worker URLs: ws://127.0.0.1:9001/v2, ws://127.0.0.1:9002/v2#' "$WORKSHEET_MD"

sed -i 's#^- Account labels: acct-a$#- Account labels:#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing worksheet account labels to fail" >&2
  exit 1
fi
sed -i 's#^- Account labels:$#- Account labels: acct-a#' "$WORKSHEET_MD"

sed -i 's#^- Route-selection identities agree across health, events, metrics, and audit: matches health, events, metrics, and audit for project-a$#- Route-selection identities agree across health, events, metrics, and audit:#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing worksheet reconciliation summary to fail" >&2
  exit 1
fi
sed -i 's#^- Route-selection identities agree across health, events, metrics, and audit:$#- Route-selection identities agree across health, events, metrics, and audit: matches health, events, metrics, and audit for project-a#' "$WORKSHEET_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision:#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing worksheet decision to fail" >&2
  exit 1
fi
sed -i 's#^- Decision:$#- Decision: Scoped Stage B#' "$WORKSHEET_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision: experimental maybe#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid worksheet decision taxonomy value to fail" >&2
  exit 1
fi
sed -i 's#^- Decision: experimental maybe$#- Decision: Scoped Stage B#' "$WORKSHEET_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision: Release-quality multi-worker#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected worksheet decision mismatch to fail" >&2
  exit 1
fi
sed -i 's#^- Decision: Release-quality multi-worker$#- Decision: Scoped Stage B#' "$WORKSHEET_MD"

sed -i 's#^- Decision: Scoped Stage B$#- Decision: Release-quality multi-worker#' "$WORKSHEET_MD"
sed -i 's#^- Decision: Scoped Stage B$#- Decision: Release-quality multi-worker#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected release-quality decision with excluded families to fail" >&2
  exit 1
fi
sed -i 's#^- Excluded method families or route classes: live active-context migration$#- Excluded method families or route classes: none#' "$WORKSHEET_MD"
sed -i 's#^- Method families excluded: live active-context migration$#- Method families excluded: none#' "$DECISION_MD"
sed -i \
  -e 's#^- Bounded handoff success/failure outcomes match client-visible behavior: not exercised in this bundle$#- Bounded handoff success/failure outcomes match client-visible behavior: validated for release-quality bundle#' \
  -e 's#^- Live active-context methods fail closed instead of moving accounts: not exercised in this bundle$#- Live active-context methods fail closed instead of moving accounts: validated for release-quality bundle#' \
  -e 's#^- Backlog and cleanup windows are bounded and observable: not exercised in this bundle$#- Backlog and cleanup windows are bounded and observable: validated for release-quality bundle#' \
  "$WORKSHEET_MD"
sed -i \
  -e 's#^- Bounded restoration evidence: not exercised in this bundle$#- Bounded restoration evidence: validated for release-quality bundle#' \
  -e 's#^- Live active-context no-handoff evidence: not exercised in this bundle$#- Live active-context no-handoff evidence: validated for release-quality bundle#' \
  -e 's#^- Reconnect/degraded-route evidence: not exercised in this bundle$#- Reconnect/degraded-route evidence: validated for release-quality bundle#' \
  -e 's#^- Slow-client/backlog evidence: not exercised in this bundle$#- Slow-client/backlog evidence: validated for release-quality bundle#' \
  -e 's#^- Cleanup/delivery-failure evidence: not exercised in this bundle$#- Cleanup/delivery-failure evidence: validated for release-quality bundle#' \
  "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected release-quality decision with unlabeled topology worker to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| 1 \| worker-b \| ws://127.0.0.1:9002/v2 \|  \| bearer \| \|#| 1 | worker-b | ws://127.0.0.1:9002/v2 | acct-b | bearer | |#' \
  "$README_MD"
sed -i 's#^- Account labels: acct-a$#- Account labels: acct-a, acct-b#' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected release-quality decision with follow-up required before wider rollout to fail" >&2
  exit 1
fi
sed -i 's#^- Follow-up required before wider rollout: not exercised in this bundle$#- Follow-up required before wider rollout: none#' "$WORKSHEET_MD"
"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null
perl -0pi -e 's#\| 1 \| worker-b \| ws://127.0.0.1:9002/v2 \| acct-b \| bearer \| \|#| 1 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |#' \
  "$README_MD"
sed -i 's#^- Account labels: acct-a, acct-b$#- Account labels: acct-a#' "$WORKSHEET_MD"
sed -i 's#^- Follow-up required before wider rollout: none$#- Follow-up required before wider rollout: not exercised in this bundle#' "$WORKSHEET_MD"
sed -i 's#^- Decision: Release-quality multi-worker$#- Decision: Scoped Stage B#' "$WORKSHEET_MD"
sed -i 's#^- Decision: Release-quality multi-worker$#- Decision: Scoped Stage B#' "$DECISION_MD"
sed -i 's#^- Excluded method families or route classes: none$#- Excluded method families or route classes: live active-context migration#' "$WORKSHEET_MD"
sed -i 's#^- Method families excluded: none$#- Method families excluded: live active-context migration#' "$DECISION_MD"
sed -i \
  -e 's#^- Bounded handoff success/failure outcomes match client-visible behavior: validated for release-quality bundle$#- Bounded handoff success/failure outcomes match client-visible behavior: not exercised in this bundle#' \
  -e 's#^- Live active-context methods fail closed instead of moving accounts: validated for release-quality bundle$#- Live active-context methods fail closed instead of moving accounts: not exercised in this bundle#' \
  -e 's#^- Backlog and cleanup windows are bounded and observable: validated for release-quality bundle$#- Backlog and cleanup windows are bounded and observable: not exercised in this bundle#' \
  "$WORKSHEET_MD"
sed -i \
  -e 's#^- Bounded restoration evidence: validated for release-quality bundle$#- Bounded restoration evidence: not exercised in this bundle#' \
  -e 's#^- Live active-context no-handoff evidence: validated for release-quality bundle$#- Live active-context no-handoff evidence: not exercised in this bundle#' \
  -e 's#^- Reconnect/degraded-route evidence: validated for release-quality bundle$#- Reconnect/degraded-route evidence: not exercised in this bundle#' \
  -e 's#^- Slow-client/backlog evidence: validated for release-quality bundle$#- Slow-client/backlog evidence: not exercised in this bundle#' \
  -e 's#^- Cleanup/delivery-failure evidence: validated for release-quality bundle$#- Cleanup/delivery-failure evidence: not exercised in this bundle#' \
  "$DECISION_MD"

sed -i 's#^- Decision file: decision.md$#- Decision file: reports/decision.md#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected decision file reference mismatch to fail" >&2
  exit 1
fi
sed -i 's#^- Decision file: reports/decision.md$#- Decision file: decision.md#' "$README_MD"

perl -0pi -e 's#\| 0 \| worker-a \| ws://127.0.0.1:9001/v2 \| acct-a \| bearer \| \|#| 0 | worker-a | ws://127.0.0.1:9001/v2 | acct-a |  | |#' \
  "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing topology auth mode to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| 0 \| worker-a \| ws://127.0.0.1:9001/v2 \| acct-a \|  \| \|#| 0 | worker-a | ws://127.0.0.1:9001/v2 | acct-a | bearer | |#' \
  "$README_MD"

sed -i 's#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#| | | | | |#' \
  "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing blocking mismatch row to fail" >&2
  exit 1
fi
sed -i 's#| | | | | |#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#' \
  "$DECISION_MD"

sed -i 's#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#| Baseline before traffic | expected | | tenant-a/project-a/worker-a | rerun after fix |#' \
  "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing blocking mismatch cell to fail" >&2
  exit 1
fi
sed -i 's#| Baseline before traffic | expected | | tenant-a/project-a/worker-a | rerun after fix |#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#' \
  "$DECISION_MD"

sed -i '/| Baseline before traffic | expected | observed | tenant-a\/project-a\/worker-a | rerun after fix |/a | Follow-up scenario | expected | TODO | tenant-a/project-a/worker-a | rerun after fix |' \
  "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected placeholder blocking mismatch in later row to fail" >&2
  exit 1
fi
sed -i '/| Follow-up scenario | expected | TODO | tenant-a\/project-a\/worker-a | rerun after fix |/d' \
  "$DECISION_MD"

touch "$bundle_dir/escape.txt"
perl -0pi -e 's#transcripts/01-baseline.txt#../escape.txt#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected invalid referenced artifact path to fail" >&2
  exit 1
fi
perl -0pi -e 's#\.\./escape.txt#transcripts/01-baseline.txt#' "$README_MD"

ln -s "$bundle_dir/escape.txt" "$bundle_path/transcripts/escape-link.txt"
perl -0pi -e 's#transcripts/01-baseline.txt#transcripts/escape-link.txt#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected symlink escape to fail" >&2
  exit 1
fi
perl -0pi -e 's#transcripts/escape-link.txt#transcripts/01-baseline.txt#' "$README_MD"
rm "$bundle_path/transcripts/escape-link.txt"

cp "$bundle_path/transcripts/01-baseline.txt" "$bundle_path/transcripts/02-alt-baseline.txt"
perl -0pi -e 's#transcripts/01-baseline.txt \| healthz/01-baseline.json#transcripts/02-alt-baseline.txt | healthz/01-baseline.json#' \
  "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched artifact references to fail" >&2
  exit 1
fi
perl -0pi -e 's#transcripts/02-alt-baseline.txt \| healthz/01-baseline.json#transcripts/01-baseline.txt | healthz/01-baseline.json#' \
  "$WORKSHEET_MD"

cat > "$bundle_path/transcripts/01-baseline.txt" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
sample transcript
EOF
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing capture metadata to fail" >&2
  exit 1
fi

cat > "$bundle_path/transcripts/01-baseline.txt" <<'EOF'
gateway build: gw-test
worker build: worker-a
tenant id: tenant-a
project id: project-a
worker id: worker-0
account id: acct-a
capture time: 2026-06-11T10:00:00Z
sample transcript
EOF

perl -0pi -e 's#worker build: worker-a#worker build:#' "$bundle_path/transcripts/01-baseline.txt"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected empty capture metadata to fail" >&2
  exit 1
fi
perl -0pi -e 's#worker build:#worker build: worker-a#' "$bundle_path/transcripts/01-baseline.txt"

perl -0pi -e 's#tenant id: tenant-a#tenant id: TBD#' "$bundle_path/transcripts/01-baseline.txt"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected placeholder capture metadata to fail" >&2
  exit 1
fi
perl -0pi -e 's#tenant id: TBD#tenant id: tenant-a#' "$bundle_path/transcripts/01-baseline.txt"

perl -0pi -e 's#gateway build: gw-test#gateway build: gw-alt#' "$bundle_path/transcripts/01-baseline.txt"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture gateway build outside bundle metadata to fail" >&2
  exit 1
fi
perl -0pi -e 's#gateway build: gw-alt#gateway build: gw-test#' "$bundle_path/transcripts/01-baseline.txt"

perl -0pi -e 's#project id: project-a#project id: project-c#' "$bundle_path/transcripts/01-baseline.txt"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture project outside bundle scope to fail" >&2
  exit 1
fi
perl -0pi -e 's#project id: project-c#project id: project-a#' "$bundle_path/transcripts/01-baseline.txt"

perl -0pi -e 's#account id: acct-a#account id: acct-c#' "$bundle_path/transcripts/01-baseline.txt"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture account outside bundle topology to fail" >&2
  exit 1
fi
perl -0pi -e 's#account id: acct-c#account id: acct-a#' "$bundle_path/transcripts/01-baseline.txt"

for path in \
  "$bundle_path/transcripts/01-baseline.txt" \
  "$bundle_path/healthz/01-baseline.json" \
  "$bundle_path/events/01-baseline.sse" \
  "$bundle_path/metrics/01-baseline.json" \
  "$bundle_path/logs/01-baseline.log"; do
  perl -0pi -e 's#worker build: worker-a#worker build: worker-b#' "$path"
done
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture worker/account pair outside bundle topology to fail" >&2
  exit 1
fi
for path in \
  "$bundle_path/transcripts/01-baseline.txt" \
  "$bundle_path/healthz/01-baseline.json" \
  "$bundle_path/events/01-baseline.sse" \
  "$bundle_path/metrics/01-baseline.json" \
  "$bundle_path/logs/01-baseline.log"; do
  perl -0pi -e 's#worker build: worker-b#worker build: worker-a#' "$path"
done

perl -0pi -e 's#worker id: worker-0#worker id: worker-9#' "$bundle_path/healthz/01-baseline.json"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched capture metadata to fail" >&2
  exit 1
fi
perl -0pi -e 's#worker id: worker-9#worker id: worker-0#' "$bundle_path/healthz/01-baseline.json"

perl -0pi -e 's#capture time: 2026-06-11T10:00:00Z#capture time: 2026-06-11T09:59:59Z#' "$bundle_path/logs/01-baseline.log"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected capture time outside window to fail" >&2
  exit 1
fi
perl -0pi -e 's#capture time: 2026-06-11T09:59:59Z#capture time: 2026-06-11T10:00:00Z#' "$bundle_path/logs/01-baseline.log"

perl -0pi -e 's#capture time: 2026-06-11T10:00:00Z#capture time: 2026-06-11T10:00:00+00:00#' "$bundle_path/logs/01-baseline.log"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected malformed capture time format to fail" >&2
  exit 1
fi
perl -0pi -e 's#capture time: 2026-06-11T10:00:00\+00:00#capture time: 2026-06-11T10:00:00Z#' "$bundle_path/logs/01-baseline.log"

perl -0pi -e 's#capture time: 2026-06-11T10:00:00Z#capture time: 2026-02-30T10:00:00Z#' "$bundle_path/logs/01-baseline.log"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected impossible artifact capture date to fail" >&2
  exit 1
fi
perl -0pi -e 's#capture time: 2026-02-30T10:00:00Z#capture time: 2026-06-11T10:00:00Z#' "$bundle_path/logs/01-baseline.log"

sed -i 's#^- Capture end: 2026-06-11T10:05:00Z$#- Capture end:#' "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing capture end metadata to fail" >&2
  exit 1
fi

sed -i 's#^- Capture end:$#- Capture end: 2026-06-11T10:05:00Z#' "$README_MD"

sed -i 's#^- Method families excluded: live active-context migration$#- Method families excluded:#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing decision metadata to fail" >&2
  exit 1
fi

sed -i 's#^- Method families excluded:$#- Method families excluded: live active-context migration#' "$DECISION_MD"

sed -i 's#^- Route-selection evidence: matches health, events, metrics, and audit for project-a$#- Route-selection evidence:#' "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing reconciliation summary to fail" >&2
  exit 1
fi

sed -i 's#^- Route-selection evidence:$#- Route-selection evidence: matches health, events, metrics, and audit for project-a#' "$DECISION_MD"

sed -i '/^This decision applies only to the topology, builds, account labels, auth mode,$/,/^timeout values, pending-request limits, and method families listed above\.$/d' \
  "$DECISION_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing invalidation rules body to fail" >&2
  exit 1
fi
cat >> "$DECISION_MD" <<'EOF'
This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
EOF

perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Baseline before traffic (mismatch) | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected mismatched worksheet scenario to fail" >&2
  exit 1
fi

perl -0pi -e 's#\| Baseline before traffic \(mismatch\) \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"

perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json |  | ok |#' \
  "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing worksheet log reference to fail" >&2
  exit 1
fi

perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \|  \| ok \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"

perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | TBD |#' \
  "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected placeholder worksheet result to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| TBD \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"

perl -0pi -e 's#\| Cleanup or delivery failure \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| worksheet row 10 \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 10 |#' \
  "$README_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected duplicate README scenario to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| worksheet row 10 \|#| Cleanup or delivery failure | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 10 |#' \
  "$README_MD"

perl -0pi -e 's#\| Cleanup or delivery failure \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected duplicate worksheet scenario to fail" >&2
  exit 1
fi
perl -0pi -e 's#\| Baseline before traffic \| transcripts/01-baseline.txt \| healthz/01-baseline.json \| events/01-baseline.sse \| metrics/01-baseline.json \| logs/01-baseline.log \| ok \|#| Cleanup or delivery failure | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#' \
  "$WORKSHEET_MD"

sed -i '/| Slow-client or backlog window |/d' "$README_MD"
sed -i '/| Slow-client or backlog window |/d' "$WORKSHEET_MD"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing required scenario to fail" >&2
  exit 1
fi
sed -i '/| Live active-context no-handoff |/a | Slow-client or backlog window | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 9 |' \
  "$README_MD"
sed -i '/| Live active-context no-handoff |/a | Slow-client or backlog window | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |' \
  "$WORKSHEET_MD"

rm "$bundle_path/metrics/01-baseline.json"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing metrics artifact to fail" >&2
  exit 1
fi

rm "$bundle_path/decision.md"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing decision.md to fail" >&2
  exit 1
fi

"$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-test-2 \
  --worker-build worker-a \
  --worker-build worker-b \
  --worker-url ws://127.0.0.1:9001/v2 \
  --account-id acct-a \
  --worker-url ws://127.0.0.1:9002/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null

rm "$bundle_dir/gw-test/topo-test-2/worksheet.md"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_dir/gw-test/topo-test-2" >/dev/null 2>&1; then
  echo "expected missing worksheet.md to fail" >&2
  exit 1
fi
