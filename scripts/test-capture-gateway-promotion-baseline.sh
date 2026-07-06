#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
server_dir=$(mktemp -d)
trap 'kill "${server_pid:-}" >/dev/null 2>&1 || true; rm -rf "$bundle_dir" "$server_dir"' EXIT

help_output="$("$repo_root/scripts/capture-gateway-promotion-baseline.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-baseline-capture -- --bundle DIR --gateway-url URL [options]'

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
import sys

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

"$repo_root/scripts/capture-gateway-promotion-baseline.sh" \
  --bundle "$bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --bearer-token test-token \
  --captured-by test-runner >/dev/null

grep -Fq -- '- Captured by: test-runner' "$bundle_path/README.md"
grep -Fq -- '- Capture start:' "$bundle_path/README.md"
grep -Fq '| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | worksheet row 1 |' "$bundle_path/README.md"
grep -Fq '| Baseline before traffic | transcripts/01-baseline.txt | healthz/01-baseline.json | events/01-baseline.sse | metrics/01-baseline.json | logs/01-baseline.log | captured |' "$bundle_path/worksheet.md"
grep -Fq 'gateway build: gw-test' "$bundle_path/healthz/01-baseline.json"
grep -Fq 'worker build: worker-a' "$bundle_path/healthz/01-baseline.json"
grep -Fq 'tenant id: tenant-a' "$bundle_path/healthz/01-baseline.json"
grep -Fq 'project id: project-a' "$bundle_path/healthz/01-baseline.json"
grep -Fq 'account id: acct-a' "$bundle_path/healthz/01-baseline.json"
grep -Fq '"projectWorkerRoutes"' "$bundle_path/healthz/01-baseline.json"

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
just gateway-promotion-baseline-capture -- \
  --bundle "$just_bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --bearer-token test-token \
  --captured-by test-runner >/dev/null
grep -Fq 'gateway build: gw-test' "$just_bundle_path/transcripts/01-baseline.txt"
rm -rf "$just_capture_dir"
