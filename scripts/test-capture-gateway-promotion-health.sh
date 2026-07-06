#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
server_dir=$(mktemp -d)
trap 'kill "${server_pid:-}" >/dev/null 2>&1 || true; rm -rf "$bundle_dir" "$server_dir"' EXIT

help_output="$("$repo_root/scripts/capture-gateway-promotion-health.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-health-capture -- --bundle DIR --gateway-url URL [options]'

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

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/healthz":
            self.send_response(404)
            self.end_headers()
            return
        if self.headers.get("x-codex-tenant-id") != "tenant-a":
            self.send_response(400)
            self.end_headers()
            return
        if self.headers.get("x-codex-project-id") != "project-a":
            self.send_response(400)
            self.end_headers()
            return
        if self.headers.get("authorization") != "Bearer test-token":
            self.send_response(401)
            self.end_headers()
            return
        body = {
            "status": "ok",
            "runtimeMode": "remote",
            "v2Connections": {"requestCounts": [{"method": "thread/start", "ok": 1}]},
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

health_path="$("$repo_root/scripts/capture-gateway-promotion-health.sh" \
  --bundle "$bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --bearer-token test-token)"

grep -Fq 'gateway build: gw-test' "$health_path"
grep -Fq 'worker build: worker-a' "$health_path"
grep -Fq 'tenant id: tenant-a' "$health_path"
grep -Fq 'project id: project-a' "$health_path"
grep -Fq 'account id: acct-a' "$health_path"
grep -Fq '"v2Connections"' "$health_path"
grep -Fq '| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-project-a.txt | healthz/02-steady-state-project-a.json | events/02-steady-state-project-a.sse | metrics/02-steady-state-project-a.json | logs/02-steady-state-project-a.log | worksheet row 2 |' "$bundle_path/README.md"
grep -Fq '| Steady-state bootstrap and thread/turn | transcripts/02-steady-state-project-a.txt | healthz/02-steady-state-project-a.json | events/02-steady-state-project-a.sse | metrics/02-steady-state-project-a.json | logs/02-steady-state-project-a.log | captured health |' "$bundle_path/worksheet.md"

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
just gateway-promotion-health-capture -- \
  --bundle "$just_bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --bearer-token test-token >/dev/null
grep -Fq '"v2Connections"' "$just_bundle_path/healthz/02-steady-state-project-a.json"
rm -rf "$just_capture_dir"
