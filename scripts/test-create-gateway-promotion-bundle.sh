#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
just_bundle_dir=$(mktemp -d)
trap 'rm -rf "$bundle_dir" "$just_bundle_dir"' EXIT

create_help_output="$("$repo_root/scripts/create-gateway-promotion-bundle.sh" --help)"
printf '%s\n' "$create_help_output" | grep -Fq 'just gateway-promotion-bundle-create -- --output DIR --gateway-build ID --topology-id ID [options]'
printf '%s\n' "$create_help_output" | grep -Fq -- '--account-id VALUE                           Repeat for each worker account id; at least one is required.'

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
grep -Fq -- '- Captured by:' "$bundle_path/README.md"
grep -Fq -- '- Tenant/project scope: tenant=tenant-a, projects=project-a' "$bundle_path/README.md"
grep -Fq -- '- Capture start:' "$bundle_path/README.md"
grep -Fq -- '- Capture end:' "$bundle_path/README.md"
grep -Fq -- '- Decision file: decision.md' "$bundle_path/README.md"
grep -Fq -- '- Gateway build: gw-test' "$bundle_path/README.md"
grep -Fq -- '- Topology id: topo-test' "$bundle_path/README.md"
grep -Fq -- '- Worker builds: worker-a, worker-b' "$bundle_path/README.md"
grep -Fq -- '- Topology id: topo-test' "$bundle_path/worksheet.md"
grep -Fq -- '- Tenant/project scope: tenant=tenant-a, projects=project-a' "$bundle_path/worksheet.md"
grep -Fq -- '- Tenant/project scopes: tenant=tenant-a, projects=project-a' "$bundle_path/decision.md"
grep -Fq -- '- Method matrix route classes covered:' "$bundle_path/worksheet.md"
grep -Fq -- '- Decision:' "$bundle_path/worksheet.md"
grep -Fq -- '- Topology id: topo-test' "$bundle_path/decision.md"
grep -Fq -- '- Route-selection evidence:' "$bundle_path/decision.md"
grep -Fq -- '- Cleanup/delivery-failure evidence:' "$bundle_path/decision.md"
grep -Fq '| Baseline before traffic | | | | | | |' "$bundle_path/worksheet.md"
grep -Fq '| Cleanup or delivery failure | | | | | | |' "$bundle_path/worksheet.md"
grep -Fq '## Decision' "$bundle_path/worksheet.md"
grep -Fq -- '- Decision:' "$bundle_path/worksheet.md"

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

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null 2>&1; then
  echo "expected missing tenant/project scope values to fail" >&2
  exit 1
fi

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --tenant-id tenant-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null 2>&1; then
  echo "expected missing project-id values to fail" >&2
  exit 1
fi

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null 2>&1; then
  echo "expected missing account-id value to fail" >&2
  exit 1
fi

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 \
  --v2-max-pending-client-requests 64 >/dev/null 2>&1; then
  echo "expected missing auth-mode value to fail" >&2
  exit 1
fi

if "$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
  --gateway-build gw-test \
  --topology-id topo-invalid \
  --worker-build worker-a \
  --worker-url ws://127.0.0.1:9001/v2 \
  --tenant-id tenant-a \
  --project-id project-a \
  --auth-mode bearer \
  --v2-initialize-timeout-seconds 30 \
  --v2-client-send-timeout-seconds 10 \
  --v2-reconnect-retry-backoff-seconds 1 \
  --v2-max-pending-server-requests 64 >/dev/null 2>&1; then
  echo "expected missing v2 pending-limit value to fail" >&2
  exit 1
fi

just_bundle_path="$(
  just gateway-promotion-bundle-create -- \
    --output "$just_bundle_dir" \
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
    --v2-max-pending-client-requests 64
)"

grep -Fq '| 0 | worker-a | ws://127.0.0.1:9001/v2 | acct-a | bearer | |' "$just_bundle_path/README.md"
grep -Fq -- '- Topology id: topo-test' "$just_bundle_path/README.md"
grep -Fq -- '- Topology id: topo-test' "$just_bundle_path/worksheet.md"
grep -Fq -- '- Tenant/project scope: tenant=tenant-a, projects=project-a' "$just_bundle_path/README.md"
grep -Fq -- '- Tenant/project scopes: tenant=tenant-a, projects=project-a' "$just_bundle_path/decision.md"
grep -Fq -- '- Decision file: decision.md' "$just_bundle_path/README.md"
