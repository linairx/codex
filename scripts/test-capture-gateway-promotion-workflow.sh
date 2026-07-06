#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
server_dir=$(mktemp -d)
source_dir=$(mktemp -d)
trap 'kill "${server_pid:-}" >/dev/null 2>&1 || true; rm -rf "$bundle_dir" "$server_dir" "$source_dir"' EXIT

capture_time=2026-06-11T10:01:00Z

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

cat > "$server_dir/server.py" <<'PY'
import http.server
import json
import socketserver
import time

class Handler(http.server.BaseHTTPRequestHandler):
    def require_scope(self):
        if self.headers.get("x-codex-tenant-id") != "tenant-a":
            self.send_response(400)
            self.end_headers()
            return False
        if self.headers.get("x-codex-project-id") != "project-a":
            self.send_response(400)
            self.end_headers()
            return False
        if self.headers.get("authorization") != "Bearer test-token":
            self.send_response(401)
            self.end_headers()
            return False
        return True

    def do_GET(self):
        if self.path == "/healthz":
            if not self.require_scope():
                return
            body = {
                "status": "ok",
                "runtimeMode": "remote",
                "v2Compatibility": "remoteSingleWorker",
                "remoteWorkers": [
                    {
                        "workerId": 0,
                        "websocketUrl": "ws://127.0.0.1:9001/v2",
                        "accountId": "acct-a",
                        "healthy": True,
                    }
                ],
                "projectWorkerRoutes": [
                    {
                        "tenantId": "tenant-a",
                        "projectId": "project-a",
                        "workerId": 0,
                        "accountId": "acct-a",
                        "accountRoutingEligible": True,
                    }
                ],
                "workerPool": {
                    "accountCount": 1,
                    "leasedAccountCount": 1,
                    "availableAccountCount": 0,
                },
            }
            payload = json.dumps(body).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return

        if self.path == "/v1/events":
            if not self.require_scope():
                return
            payload = (
                "event: gateway/projectWorkerRouteSelected\n"
                "data: {\"tenantId\":\"tenant-a\",\"projectId\":\"project-a\",\"workerId\":0,\"accountId\":\"acct-a\"}\n\n"
            ).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.send_header("cache-control", "no-cache")
            self.end_headers()
            self.wfile.write(payload)
            self.wfile.flush()
            time.sleep(0.1)
            return

        self.send_response(404)
        self.end_headers()

    def log_message(self, format, *args):
        pass

with socketserver.TCPServer(("127.0.0.1", 0), Handler) as server:
    print(server.server_address[1], flush=True)
    server.serve_forever()
PY

python3 "$server_dir/server.py" > "$server_dir/port" &
server_pid=$!
for _ in $(seq 1 50); do
  if [ -s "$server_dir/port" ]; then
    break
  fi
  sleep 0.1
done
port=$(cat "$server_dir/port")
gateway_url="http://127.0.0.1:$port"

"$repo_root/scripts/capture-gateway-promotion-baseline.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --bearer-token test-token \
  --captured-by workflow-test \
  --capture-time "$capture_time" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-health.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --bearer-token test-token \
  --capture-time "$capture_time" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-health.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --prefix 03-project-route-selection \
  --scenario "Project route selection" \
  --bearer-token test-token \
  --capture-time "$capture_time" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-events.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --prefix 02-steady-state-project-a \
  --duration-seconds 1 \
  --bearer-token test-token \
  --capture-time "$capture_time" >/dev/null

"$repo_root/scripts/test-capture-gateway-promotion-scenario.sh" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-events.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --prefix 03-project-route-selection \
  --duration-seconds 1 \
  --bearer-token test-token \
  --capture-time "$capture_time" >/dev/null

cat > "$source_dir/transcript.txt" <<'EOF'
client bootstrap ok
thread/start ok
project route selection observed
EOF
cat > "$source_dir/metrics.json" <<'EOF'
{"metric":"gateway_project_worker_route_selections","tenant_id":"tenant-a","project_id":"project-a","account_id":"acct-a","value":1}
EOF
cat > "$source_dir/logs.log" <<'EOF'
codex_gateway.audit gateway project worker route selected tenant_id=tenant-a project_id=project-a account_id=acct-a
EOF

for prefix in 02-steady-state-project-a 03-project-route-selection; do
  "$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
    --bundle "$bundle_path" \
    --kind transcript \
    --prefix "$prefix" \
    --source "$source_dir/transcript.txt" \
    --capture-time "$capture_time" >/dev/null
  "$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
    --bundle "$bundle_path" \
    --kind metrics \
    --prefix "$prefix" \
    --source "$source_dir/metrics.json" \
    --capture-time "$capture_time" >/dev/null
  "$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
    --bundle "$bundle_path" \
    --kind logs \
    --prefix "$prefix" \
    --source "$source_dir/logs.log" \
    --capture-time "$capture_time" >/dev/null
done

replace_field() {
  path=$1
  prefix=$2
  replacement=$3
  tmp=$(mktemp)
  awk -v prefix="$prefix" -v replacement="$replacement" '
    index($0, prefix) == 1 {
      print prefix " " replacement
      next
    }
    { print }
  ' "$path" > "$tmp"
  mv "$tmp" "$path"
}

replace_row() {
  path=$1
  scenario=$2
  replacement=$3
  tmp=$(mktemp)
  awk -v scenario="$scenario" -v replacement="$replacement" '
    $0 ~ "^\\| " scenario " \\|" {
      print replacement
      next
    }
    { print }
  ' "$path" > "$tmp"
  mv "$tmp" "$path"
}

replace_empty_table_row() {
  path=$1
  replacement=$2
  tmp=$(mktemp)
  awk -v replacement="$replacement" '
    $0 == "| | | | | |" {
      print replacement
      next
    }
    { print }
  ' "$path" > "$tmp"
  mv "$tmp" "$path"
}

scenario_row_number() {
  path=$1
  scenario=$2
  awk -F'|' -v scenario="$scenario" '
    function trim(value) {
      gsub(/^ +| +$/, "", value)
      return value
    }

    /^## (Evidence Index|Captures)$/ { in_captures = 1; next }
    in_captures && /^## / { exit }

    in_captures && /^\|/ {
      row_scenario = trim($2)
      if (row_scenario == "Scenario" || row_scenario == "---" || row_scenario == "") {
        next
      }
      row_number += 1
      if (row_scenario == scenario) {
        print row_number
        exit
      }
    }
  ' "$path"
}

fill_scenario() {
  scenario=$1
  prefix=$2
  result=$3
  row_number=$(scenario_row_number "$bundle_path/README.md" "$scenario")
  replace_row \
    "$bundle_path/README.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | worksheet row $row_number |"
  replace_row \
    "$bundle_path/worksheet.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | $result |"
}

for scenario in \
  "Reconnect and recovery" \
  "Degraded-route fail-closed" \
  "Account-capacity transition" \
  "Bounded restoration" \
  "Live active-context no-handoff" \
  "Slow-client or backlog window" \
  "Cleanup or delivery failure"; do
  fill_scenario "$scenario" 03-project-route-selection "captured in scoped Stage B evidence"
done

grep -Fq '| Project route selection | transcripts/03-project-route-selection.txt | healthz/03-project-route-selection.json | events/03-project-route-selection.sse | metrics/03-project-route-selection.json | logs/03-project-route-selection.log | worksheet row 3 |' "$bundle_path/README.md"

replace_field "$bundle_path/worksheet.md" '- Method matrix route classes covered:' 'project-aware account routing, single-worker remote compatibility'
replace_field "$bundle_path/worksheet.md" '- Route-selection identities agree across health, events, metrics, and audit:' 'tenant-a/project-a/acct-a route-selection identities match in captured artifacts'
replace_field "$bundle_path/worksheet.md" '- Account-capacity identities agree across health, events, metrics, and logs:' 'tenant-a/project-a/acct-a account evidence is present for scoped Stage B'
replace_field "$bundle_path/worksheet.md" '- Bounded handoff success/failure outcomes match client-visible behavior:' 'bounded handoff evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/worksheet.md" '- Live active-context methods fail closed instead of moving accounts:' 'active-context no-handoff evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/worksheet.md" '- Backlog and cleanup windows are bounded and observable:' 'backlog and cleanup evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/worksheet.md" '- Decision:' 'Scoped Stage B'
replace_field "$bundle_path/worksheet.md" '- Promotion scope:' 'single-worker remote evidence capture workflow'
replace_field "$bundle_path/worksheet.md" '- Excluded method families or route classes:' 'multi-worker release-quality'
replace_field "$bundle_path/worksheet.md" '- Follow-up required before wider rollout:' 'collect live multi-worker traffic evidence'

replace_field "$bundle_path/decision.md" '- Decision:' 'Scoped Stage B'
replace_field "$bundle_path/decision.md" '- Promotion scope:' 'single-worker remote evidence capture workflow'
replace_field "$bundle_path/decision.md" '- Method families included:' 'project-aware account routing, single-worker remote compatibility'
replace_field "$bundle_path/decision.md" '- Method families excluded:' 'multi-worker release-quality'
replace_field "$bundle_path/decision.md" '- Route-selection evidence:' 'tenant-a/project-a/acct-a route-selection identities match in captured artifacts'
replace_field "$bundle_path/decision.md" '- Account-capacity evidence:' 'tenant-a/project-a/acct-a account evidence is present for scoped Stage B'
replace_field "$bundle_path/decision.md" '- Bounded restoration evidence:' 'bounded handoff evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/decision.md" '- Live active-context no-handoff evidence:' 'active-context no-handoff evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/decision.md" '- Reconnect/degraded-route evidence:' 'backlog and cleanup evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/decision.md" '- Slow-client/backlog evidence:' 'backlog and cleanup evidence is represented by scoped Stage B artifacts'
replace_field "$bundle_path/decision.md" '- Cleanup/delivery-failure evidence:' 'backlog and cleanup evidence is represented by scoped Stage B artifacts'
replace_empty_table_row \
  "$bundle_path/decision.md" \
  "| None for scoped Stage B | Expected matching route identities | Observed matching route identities | tenant-a/project-a/acct-a | Collect live multi-worker traffic evidence |"

"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle_path" >/dev/null

just gateway-promotion-bundle-check -- "$bundle_path" >/dev/null
