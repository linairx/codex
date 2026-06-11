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

"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null

rm "$bundle_path/decision.md"
if "$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null 2>&1; then
  echo "expected missing decision.md to fail" >&2
  exit 1
fi
