#!/usr/bin/env sh
set -eu

gateway_bin=${CODEX_GATEWAY_BIN:-/usr/local/bin/codex-gateway}

wait_for_local_worker_readyz() {
  readyz_url=$1
  attempt=0
  while :; do
    if wget -qO- "$readyz_url" >/dev/null 2>&1; then
      return 0
    fi
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 30 ]; then
      echo "timed out waiting for local app-server at $readyz_url" >&2
      exit 1
    fi
    sleep 1
  done
}

if [ "$#" -gt 0 ]; then
  exec "$gateway_bin" "$@"
fi

set -- "$gateway_bin"

set -- "$@" --listen "${CODEX_GATEWAY_LISTEN:-0.0.0.0:8080}"
set -- "$@" --runtime "${CODEX_GATEWAY_RUNTIME:-embedded}"

if [ "${CODEX_GATEWAY_RUNTIME:-embedded}" = remote ] &&
  [ -n "${CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS:-}" ]; then
  old_ifs=${IFS}
  IFS=,
  for websocket_url in ${CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS}; do
    case "$websocket_url" in
      ws://127.0.0.1:9001* | ws://localhost:9001*)
        readyz_base=${websocket_url#ws://}
        readyz_base=${readyz_base%%/*}
        wait_for_local_worker_readyz "http://${readyz_base}/readyz"
        break
        ;;
    esac
  done
  IFS=${old_ifs}
fi

if [ -n "${CODEX_GATEWAY_SESSION_SOURCE:-}" ]; then
  set -- "$@" --session-source "${CODEX_GATEWAY_SESSION_SOURCE}"
fi

if [ -n "${CODEX_GATEWAY_CLIENT_NAME:-}" ]; then
  set -- "$@" --client-name "${CODEX_GATEWAY_CLIENT_NAME}"
fi

if [ -n "${CODEX_GATEWAY_CLIENT_VERSION:-}" ]; then
  set -- "$@" --client-version "${CODEX_GATEWAY_CLIENT_VERSION}"
fi

if [ -n "${CODEX_GATEWAY_EXEC_SERVER_URL:-}" ]; then
  set -- "$@" --exec-server-url "${CODEX_GATEWAY_EXEC_SERVER_URL}"
fi

if [ -n "${CODEX_GATEWAY_BEARER_TOKEN:-}" ]; then
  set -- "$@" --bearer-token "${CODEX_GATEWAY_BEARER_TOKEN}"
fi

case "${CODEX_GATEWAY_NO_AUDIT_LOGS:-}" in
  1 | true | TRUE | yes | YES | on | ON)
    set -- "$@" --no-audit-logs
    ;;
esac

if [ -n "${CODEX_GATEWAY_REQUESTS_PER_MINUTE:-}" ]; then
  set -- "$@" --requests-per-minute "${CODEX_GATEWAY_REQUESTS_PER_MINUTE}"
fi

if [ -n "${CODEX_GATEWAY_TURN_STARTS_PER_MINUTE:-}" ]; then
  set -- "$@" --turn-starts-per-minute "${CODEX_GATEWAY_TURN_STARTS_PER_MINUTE}"
fi

if [ -n "${CODEX_GATEWAY_V2_INITIALIZE_TIMEOUT_SECONDS:-}" ]; then
  set -- "$@" --v2-initialize-timeout-seconds "${CODEX_GATEWAY_V2_INITIALIZE_TIMEOUT_SECONDS}"
fi

if [ -n "${CODEX_GATEWAY_V2_CLIENT_SEND_TIMEOUT_SECONDS:-}" ]; then
  set -- "$@" --v2-client-send-timeout-seconds "${CODEX_GATEWAY_V2_CLIENT_SEND_TIMEOUT_SECONDS}"
fi

if [ -n "${CODEX_GATEWAY_V2_RECONNECT_RETRY_BACKOFF_SECONDS:-}" ]; then
  set -- "$@" --v2-reconnect-retry-backoff-seconds "${CODEX_GATEWAY_V2_RECONNECT_RETRY_BACKOFF_SECONDS}"
fi

if [ -n "${CODEX_GATEWAY_V2_MAX_PENDING_SERVER_REQUESTS:-}" ]; then
  set -- "$@" --v2-max-pending-server-requests "${CODEX_GATEWAY_V2_MAX_PENDING_SERVER_REQUESTS}"
fi

if [ -n "${CODEX_GATEWAY_V2_MAX_PENDING_CLIENT_REQUESTS:-}" ]; then
  set -- "$@" --v2-max-pending-client-requests "${CODEX_GATEWAY_V2_MAX_PENDING_CLIENT_REQUESTS}"
fi

if [ -n "${CODEX_GATEWAY_REMOTE_AUTH_TOKEN:-}" ]; then
  set -- "$@" --remote-auth-token "${CODEX_GATEWAY_REMOTE_AUTH_TOKEN}"
fi

old_ifs=${IFS}
IFS=,
for websocket_url in ${CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS:-}; do
  if [ -n "${websocket_url}" ]; then
    set -- "$@" --remote-websocket-url "${websocket_url}"
  fi
done

for account_id in ${CODEX_GATEWAY_REMOTE_ACCOUNT_IDS:-}; do
  if [ -n "${account_id}" ]; then
    set -- "$@" --remote-account-id "${account_id}"
  fi
done
IFS=${old_ifs}

exec "$@"
