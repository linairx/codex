#!/usr/bin/env sh
set -eu

app_server_bin=${CODEX_APP_SERVER_BIN:-/usr/local/bin/codex-app-server}

if [ "$#" -gt 0 ]; then
  exec "$app_server_bin" "$@"
fi

if [ -z "${CODEX_APP_SERVER_WS_TOKEN:-}" ]; then
  echo "CODEX_APP_SERVER_WS_TOKEN is required when no explicit app-server command is passed" >&2
  exit 1
fi

ws_token_sha256="$(printf '%s' "$CODEX_APP_SERVER_WS_TOKEN" | sha256sum | awk '{print $1}')"

set -- "$app_server_bin"
set -- "$@" --listen "${CODEX_APP_SERVER_LISTEN:-ws://0.0.0.0:9001}"
set -- "$@" --session-source "${CODEX_APP_SERVER_SESSION_SOURCE:-app-server}"
set -- "$@" --ws-auth capability-token
set -- "$@" --ws-token-sha256 "$ws_token_sha256"

if [ -n "${CODEX_APP_SERVER_CLIENT_NAME:-}" ]; then
  set -- "$@" --client-name "${CODEX_APP_SERVER_CLIENT_NAME}"
fi

if [ -n "${CODEX_APP_SERVER_CLIENT_VERSION:-}" ]; then
  set -- "$@" --client-version "${CODEX_APP_SERVER_CLIENT_VERSION}"
fi

exec "$@"
