#!/usr/bin/env sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
compose_file="$repo_root/docker-compose.gateway.yml"

default_config="$(docker compose -f "$compose_file" config)"
remote_config="$(docker compose -f "$compose_file" --profile remote config)"
dockerignore="$repo_root/.dockerignore"

printf '%s\n' "$default_config" | grep -Fq 'dockerfile: Dockerfile.gateway'
printf '%s\n' "$default_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_AUTH_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'dockerfile: Dockerfile.gateway'
printf '%s\n' "$remote_config" | grep -Fq 'dockerfile: Dockerfile.app-server'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_AUTH_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_ACCOUNT_POOL_ACCOUNT_IDS:'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_ACCOUNT_POOL_LOGIN_STATE_PATHS:'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_ACCOUNT_LOGIN_STATE_PATHS:'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_APP_SERVER_WS_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'network_mode: service:gateway'
printf '%s\n' "$remote_config" | grep -Fq 'condition: service_started'

grep -Fq '!Dockerfile.app-server' "$dockerignore"
grep -Fq '!scripts/' "$dockerignore"
grep -Fq '!scripts/**' "$dockerignore"

if [ "${CODEX_GATEWAY_DOCKER_COMPOSE_BUILD:-0}" = "1" ]; then
  docker compose -f "$compose_file" build gateway app-server
fi

if [ "${CODEX_GATEWAY_DOCKER_COMPOSE_RUN:-0}" != "1" ]; then
  exit 0
fi

if ! docker info >/dev/null 2>&1; then
  echo "docker daemon is not reachable" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for gateway compose smoke checks" >&2
  exit 1
fi

project_name="codex-gateway-smoke-$$"
smoke_port="${CODEX_GATEWAY_SMOKE_PORT:-18080}"
health_url="http://127.0.0.1:${smoke_port}/healthz"

compose() {
  CODEX_GATEWAY_PORT="$smoke_port" docker compose -p "$project_name" -f "$compose_file" "$@"
}

cleanup() {
  compose --profile remote down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

fetch_healthz() {
  python3 - "$health_url" <<'PY'
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=1) as response:
    sys.stdout.write(response.read().decode("utf-8"))
PY
}

wait_for_healthz() {
  jq_filter=$1
  label=$2
  deadline=$((SECONDS + 60))
  last_body=""

  while [ "$SECONDS" -lt "$deadline" ]; do
    if last_body=$(fetch_healthz 2>/dev/null); then
      if printf '%s\n' "$last_body" | jq -e "$jq_filter" >/dev/null; then
        return 0
      fi
    fi
    sleep 1
  done

  printf 'timed out waiting for %s at %s\n' "$label" "$health_url" >&2
  if [ -n "$last_body" ]; then
    printf 'last /healthz body:\n%s\n' "$last_body" >&2
  fi
  return 1
}

compose up -d --no-build gateway
wait_for_healthz \
  '.status == "ok" and .runtimeMode == "embedded" and .v2Compatibility == "embedded" and .v2Transport.maxPendingServerRequests != null' \
  "embedded gateway health"
compose down -v --remove-orphans

CODEX_GATEWAY_RUNTIME=remote \
CODEX_GATEWAY_REMOTE_WEBSOCKET_URLS=ws://127.0.0.1:9001/v2 \
CODEX_GATEWAY_REMOTE_ACCOUNT_IDS=acct-a \
CODEX_GATEWAY_REMOTE_ACCOUNT_LOGIN_STATE_PATHS=/codex-home/acct-a \
CODEX_GATEWAY_REMOTE_ACCOUNT_POOL_ACCOUNT_IDS=acct-a,acct-b \
CODEX_GATEWAY_REMOTE_ACCOUNT_POOL_LOGIN_STATE_PATHS=/codex-home/acct-a,/codex-home/acct-b \
  compose --profile remote up -d --no-build gateway app-server
wait_for_healthz \
  '.runtimeMode == "remote" and .v2Compatibility == "remoteSingleWorker" and (.remoteWorkers | length) == 1 and .remoteWorkers[0].websocketUrl == "ws://127.0.0.1:9001/v2" and .remoteWorkers[0].accountId == "acct-a" and .remoteWorkers[0].healthy == true and .v2Transport.reconnectRetryBackoffSeconds != null and .workerPool.accountCount == 2 and .workerPool.leasedAccountCount == 1 and .workerPool.availableAccountCount == 1 and .workerPool.accountLoginStatePathCount == 2 and .workerPool.workerAccountLoginStatePathCount == 1 and .workerPool.poolAccountLoginStatePathCount == 1 and .workerPool.workerSlots[0].accountLoginStatePath == "/codex-home/acct-a" and (.workerPool.accounts[] | select(.accountId == "acct-b" and .leaseState == "available" and .accountLoginStatePath == "/codex-home/acct-b"))' \
  "remote gateway health"
