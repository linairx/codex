#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
trap 'rm -rf "$bundle_dir"' EXIT

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

sed -i \
  -e 's#| | | | | | |#| 0 | worker-a | ws://127.0.0.1:9001/v2 | acct-a | bearer | sample |#g' \
  "$README_MD"
sed -i \
  -e 's#| | | | | | |#| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | ok |#g' \
  "$WORKSHEET_MD"
sed -i \
  -e 's#| | | | | |#| Baseline before traffic | expected | observed | tenant-a/project-a/worker-a | rerun after fix |#g' \
  "$DECISION_MD"

"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null

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
