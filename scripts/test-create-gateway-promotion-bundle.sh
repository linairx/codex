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

grep -Fq '| 0 | worker-a | ws://127.0.0.1:9001/v2 | acct-a | bearer | |' "$bundle_path/README.md"
grep -Fq '| 1 | worker-b | ws://127.0.0.1:9002/v2 |  | bearer | |' "$bundle_path/README.md"
grep -Fq '| Baseline before traffic | | | | | | |' "$bundle_path/worksheet.md"
grep -Fq '| Cleanup or delivery failure | | | | | | |' "$bundle_path/worksheet.md"

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --worker-url ws://127.0.0.1:9002/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null 2>&1; then
  echo "expected mismatched worker-build/worker-url values to fail" >&2
  exit 1
fi
