#!/usr/bin/env sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
entrypoint="$repo_root/scripts/gateway-docker-entrypoint.sh"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fake_gateway="$tmp_dir/fake-codex-gateway"
cat >"$fake_gateway" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$@"
EOF
chmod +x "$fake_gateway"

fake_wget="$tmp_dir/wget"
cat >"$fake_wget" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$*" >> "${WGET_CALLS_FILE:?}"
exit 0
EOF
chmod +x "$fake_wget"

embedded_output="$(
  CODEX_GATEWAY_BIN="$fake_gateway" \
  PATH="$tmp_dir:$PATH" \
  CODEX_GATEWAY_LISTEN=0.0.0.0:8080 \
  CODEX_GATEWAY_RUNTIME=embedded \
  CODEX_GATEWAY_SESSION_SOURCE=gateway \
  CODEX_GATEWAY_REQUESTS_PER_MINUTE=120 \
  CODEX_GATEWAY_V2_MAX_PENDING_CLIENT_REQUESTS=128 \
  sh "$entrypoint"
)"

expected_embedded_output=$(cat <<'EOF'
--listen
0.0.0.0:8080
--runtime
embedded
--session-source
gateway
--requests-per-minute
120
--v2-max-pending-client-requests
128
EOF
)

if [ "$embedded_output" != "$expected_embedded_output" ]; then
  printf '%s\n' "unexpected embedded entrypoint output" >&2
  printf 'expected:\n%s\n' "$expected_embedded_output" >&2
  printf 'actual:\n%s\n' "$embedded_output" >&2
  exit 1
fi

wait_calls_file="$tmp_dir/wget.calls"
touch "$wait_calls_file"

remote_output="$(
  CODEX_GATEWAY_BIN="$fake_gateway" \
  PATH="$tmp_dir:$PATH" \
  WGET_CALLS_FILE="$wait_calls_file" \
  CODEX_GATEWAY_RUNTIME=remote \
  CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS=ws://127.0.0.1:9001/v2 \
  sh "$entrypoint"
)"

expected_remote_output=$(cat <<'EOF'
--listen
0.0.0.0:8080
--runtime
remote
--remote-websocket-url
ws://127.0.0.1:9001/v2
EOF
)

if [ "$remote_output" != "$expected_remote_output" ]; then
  printf '%s\n' "unexpected remote entrypoint output" >&2
  printf 'expected:\n%s\n' "$expected_remote_output" >&2
  printf 'actual:\n%s\n' "$remote_output" >&2
  exit 1
fi

if ! grep -Fq 'http://127.0.0.1:9001/readyz' "$wait_calls_file"; then
  echo "expected the gateway entrypoint to wait for the local worker readyz endpoint" >&2
  exit 1
fi

passthrough_output="$(
  CODEX_GATEWAY_BIN="$fake_gateway" \
  PATH="$tmp_dir:$PATH" \
  sh "$entrypoint" --listen 127.0.0.1:9090 --runtime remote
)"

expected_passthrough_output=$(cat <<'EOF'
--listen
127.0.0.1:9090
--runtime
remote
EOF
)

if [ "$passthrough_output" != "$expected_passthrough_output" ]; then
  printf '%s\n' "unexpected passthrough entrypoint output" >&2
  printf 'expected:\n%s\n' "$expected_passthrough_output" >&2
  printf 'actual:\n%s\n' "$passthrough_output" >&2
  exit 1
fi
