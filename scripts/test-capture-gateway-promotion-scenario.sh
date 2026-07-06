#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
server_dir=$(mktemp -d)
source_dir=$(mktemp -d)
trap 'kill "${server_pid:-}" >/dev/null 2>&1 || true; rm -rf "$bundle_dir" "$server_dir" "$source_dir"' EXIT

capture_time=2026-06-11T10:02:00Z
prefix=02-steady-state-project-a
scenario="Steady-state bootstrap and thread/turn"

help_output="$("$repo_root/scripts/capture-gateway-promotion-scenario.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-scenario-capture -- --bundle DIR --gateway-url URL --prefix VALUE --scenario VALUE'

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

cat > "$source_dir/metrics.json" <<'EOF'
{"metric":"gateway_project_worker_route_selections","tenant_id":"tenant-a","project_id":"project-a","account_id":"acct-a","value":1}
EOF
cat > "$source_dir/logs.log" <<'EOF'
codex_gateway.audit gateway project worker route selected tenant_id=tenant-a project_id=project-a account_id=acct-a
EOF

"$repo_root/scripts/capture-gateway-promotion-scenario.sh" \
  --bundle "$bundle_path" \
  --gateway-url "$gateway_url" \
  --prefix "$prefix" \
  --scenario "$scenario" \
  --metrics-source "$source_dir/metrics.json" \
  --logs-source "$source_dir/logs.log" \
  --duration-seconds 1 \
  --bearer-token test-token \
  --capture-time "$capture_time" \
  -- sh -c 'printf "%s\n" "client bootstrap ok" "thread/start ok"' >/dev/null

grep -Fq 'client bootstrap ok' "$bundle_path/transcripts/$prefix.txt"
grep -Fq 'gateway/projectWorkerRouteSelected' "$bundle_path/events/$prefix.sse"
grep -Fq '"projectWorkerRoutes"' "$bundle_path/healthz/$prefix.json"
grep -Fq 'gateway_project_worker_route_selections' "$bundle_path/metrics/$prefix.json"
grep -Fq 'codex_gateway.audit' "$bundle_path/logs/$prefix.log"
grep -Fq "capture time: $capture_time" "$bundle_path/transcripts/$prefix.txt"
grep -Fq "capture time: $capture_time" "$bundle_path/events/$prefix.sse"
grep -Fq "capture time: $capture_time" "$bundle_path/healthz/$prefix.json"
grep -Fq "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | worksheet row 2 |" "$bundle_path/README.md"
grep -Fq "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | captured health |" "$bundle_path/worksheet.md"

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
just gateway-promotion-scenario-capture -- \
  --bundle "$just_bundle_path" \
  --gateway-url "$gateway_url" \
  --prefix "$prefix" \
  --scenario "$scenario" \
  --transcript-source "$bundle_path/transcripts/$prefix.txt" \
  --metrics-source "$source_dir/metrics.json" \
  --logs-source "$source_dir/logs.log" \
  --duration-seconds 1 \
  --bearer-token test-token \
  --capture-time "$capture_time" >/dev/null
grep -Fq 'client bootstrap ok' "$just_bundle_path/transcripts/$prefix.txt"
rm -rf "$just_capture_dir"
