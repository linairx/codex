#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bundle_dir=$(mktemp -d)
server_dir=$(mktemp -d)
trap 'kill "${server_pid:-}" >/dev/null 2>&1 || true; rm -rf "$bundle_dir" "$server_dir"' EXIT

help_output="$("$repo_root/scripts/capture-gateway-promotion-events.sh" --help)"
printf '%s\n' "$help_output" | grep -Fq 'just gateway-promotion-events-capture -- --bundle DIR --gateway-url URL [options]'

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
import socketserver
import time

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path != "/v1/events":
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

        payload = (
            "event: gateway/projectWorkerRouteSelected\n"
            "data: {\"tenantId\":\"tenant-a\",\"projectId\":\"project-a\",\"accountId\":\"acct-a\"}\n\n"
        ).encode("utf-8")
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.send_header("cache-control", "no-cache")
        self.end_headers()
        self.wfile.write(payload)
        self.wfile.flush()
        time.sleep(0.1)

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

events_path="$("$repo_root/scripts/capture-gateway-promotion-events.sh" \
  --bundle "$bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --duration-seconds 1 \
  --bearer-token test-token)"

grep -Fq 'gateway build: gw-test' "$events_path"
grep -Fq 'worker build: worker-a' "$events_path"
grep -Fq 'tenant id: tenant-a' "$events_path"
grep -Fq 'project id: project-a' "$events_path"
grep -Fq 'account id: acct-a' "$events_path"
grep -Fq 'event: gateway/projectWorkerRouteSelected' "$events_path"

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
just gateway-promotion-events-capture -- \
  --bundle "$just_bundle_path" \
  --gateway-url "http://127.0.0.1:$port" \
  --duration-seconds 1 \
  --bearer-token test-token >/dev/null
grep -Fq 'gateway/projectWorkerRouteSelected' "$just_bundle_path/events/02-events.sse"
rm -rf "$just_capture_dir"
