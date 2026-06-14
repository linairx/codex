#!/usr/bin/env sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
entrypoint="$repo_root/scripts/app-server-docker-entrypoint.sh"
tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT

fake_app_server="$tmp_dir/fake-codex-app-server"
cat >"$fake_app_server" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$@"
EOF
chmod +x "$fake_app_server"

output="$(
  CODEX_APP_SERVER_BIN="$fake_app_server" \
  CODEX_APP_SERVER_WS_TOKEN=secret-token \
  CODEX_APP_SERVER_CLIENT_NAME=codex-app-server \
  CODEX_APP_SERVER_CLIENT_VERSION=1.2.3 \
  sh "$entrypoint"
)"

expected_output=$(cat <<'EOF'
--listen
ws://0.0.0.0:9001
--session-source
app-server
--ws-auth
capability-token
--ws-token-sha256
930bbdc51b6aed5c2a5678fd6e28dee7a05e8a4b643cfc0b4427c3efb86c0d94
--client-name
codex-app-server
--client-version
1.2.3
EOF
)

if [ "$output" != "$expected_output" ]; then
  printf '%s\n' "unexpected app-server entrypoint output" >&2
  printf 'expected:\n%s\n' "$expected_output" >&2
  printf 'actual:\n%s\n' "$output" >&2
  exit 1
fi

if CODEX_APP_SERVER_BIN="$fake_app_server" sh "$entrypoint" >/dev/null 2>&1; then
  echo "expected missing CODEX_APP_SERVER_WS_TOKEN to fail" >&2
  exit 1
fi
