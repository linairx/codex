#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
source_dir=$(mktemp -d)
trap 'rm -rf "$bundle_dir" "$source_dir"' EXIT

help_output="$("$repo_root/scripts/capture-gateway-promotion-artifact.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-artifact-capture -- --bundle DIR --kind KIND --prefix VALUE [options]'

bundle_path="$("$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$bundle_dir" \
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

cat > "$source_dir/transcript.txt" <<'EOF'
client bootstrap ok
thread/start ok
EOF
cat > "$source_dir/metrics.json" <<'EOF'
{"metric":"gateway_project_worker_route_selections","value":1}
EOF

transcript_path="$("$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  --bundle "$bundle_path" \
  --kind transcript \
  --prefix 02-steady-state-project-a \
  --source "$source_dir/transcript.txt" \
  --capture-time 2026-06-11T10:01:00Z)"
metrics_path="$("$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  --bundle "$bundle_path" \
  --kind metrics \
  --prefix 02-steady-state-project-a \
  --source "$source_dir/metrics.json" \
  --capture-time 2026-06-11T10:01:00Z)"
logs_path="$(printf 'codex_gateway.audit gateway project worker route selected\n' | "$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  --bundle "$bundle_path" \
  --kind logs \
  --prefix 02-steady-state-project-a \
  --capture-time 2026-06-11T10:01:00Z)"

for path in "$transcript_path" "$metrics_path" "$logs_path"; do
  grep -Fq 'gateway build: gw-test' "$path"
  grep -Fq 'worker build: worker-a' "$path"
  grep -Fq 'tenant id: tenant-a' "$path"
  grep -Fq 'project id: project-a' "$path"
  grep -Fq 'account id: acct-a' "$path"
  grep -Fq 'capture time: 2026-06-11T10:01:00Z' "$path"
done

grep -Fq 'client bootstrap ok' "$transcript_path"
grep -Fq 'gateway_project_worker_route_selections' "$metrics_path"
grep -Fq 'codex_gateway.audit gateway project worker route selected' "$logs_path"

just_capture_dir=$(mktemp -d)
just_bundle_path="$("$repo_root/scripts/create-gateway-promotion-bundle.sh" \
  --output "$just_capture_dir" \
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
printf 'client flow ok\n' | just gateway-promotion-artifact-capture -- \
  --bundle "$just_bundle_path" \
  --kind transcript \
  --prefix 02-steady-state-project-a \
  --capture-time 2026-06-11T10:01:00Z >/dev/null
grep -Fq 'client flow ok' "$just_bundle_path/transcripts/02-steady-state-project-a.txt"
rm -rf "$just_capture_dir"
