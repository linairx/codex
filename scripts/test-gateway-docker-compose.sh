#!/usr/bin/env sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
compose_file="$repo_root/docker-compose.gateway.yml"

default_config="$(docker compose -f "$compose_file" config)"
remote_config="$(docker compose -f "$compose_file" --profile remote config)"
multi_worker_config="$(docker compose -f "$compose_file" --profile remote-multi-worker config)"
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
printf '%s\n' "$multi_worker_config" | grep -Fq 'gateway-multi-worker:'
printf '%s\n' "$multi_worker_config" | grep -Fq 'app-server-a:'
printf '%s\n' "$multi_worker_config" | grep -Fq 'app-server-b:'
printf '%s\n' "$multi_worker_config" | grep -Fq 'ws://127.0.0.1:19001/v2'
printf '%s\n' "$multi_worker_config" | grep -Fq 'ws://127.0.0.1:19002/v2'
printf '%s\n' "$multi_worker_config" | grep -Fq 'socat TCP-LISTEN:19001,bind=127.0.0.1,reuseaddr,fork TCP:app-server-a:9001'
printf '%s\n' "$multi_worker_config" | grep -Fq 'socat TCP-LISTEN:19002,bind=127.0.0.1,reuseaddr,fork TCP:app-server-b:9001'
printf '%s\n' "$multi_worker_config" | grep -Fq 'condition: service_healthy'
printf '%s\n' "$multi_worker_config" | grep -Fq 'published: "8081"'

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
  CODEX_GATEWAY_PORT="$smoke_port" \
  CODEX_GATEWAY_MULTI_WORKER_PORT="${CODEX_GATEWAY_MULTI_WORKER_PORT:-}" \
    docker compose -p "$project_name" -f "$compose_file" "$@"
}

cleanup() {
  compose --profile remote --profile remote-multi-worker down -v --remove-orphans >/dev/null 2>&1 || true
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

create_thread() {
  project_id=$1
  curl --fail --silent --show-error \
    -H 'content-type: application/json' \
    -H 'x-codex-tenant-id: compose-smoke' \
    -H "x-codex-project-id: $project_id" \
    --data '{"cwd":"/tmp/'"$project_id"'","ephemeral":true}' \
    "${health_url%/healthz}/v1/threads"
}

read_thread() {
  project_id=$1
  thread_id=$2

  curl --fail --silent --show-error --max-time 10 \
    -H 'x-codex-tenant-id: compose-smoke' \
    -H "x-codex-project-id: $project_id" \
    "${health_url%/healthz}/v1/threads/$thread_id"
}

wait_for_healthz() {
  jq_filter=$1
  label=$2
  deadline=$(($(date +%s) + 60))
  last_body=""

  while [ "$(date +%s)" -lt "$deadline" ]; do
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
compose --profile remote down -v --remove-orphans

CODEX_GATEWAY_MULTI_WORKER_PORT="$smoke_port" \
CODEX_GATEWAY_V2_RECONNECT_RETRY_BACKOFF_SECONDS=1 \
  compose --profile remote-multi-worker up -d --no-build \
    gateway-multi-worker app-server-a app-server-b
wait_for_healthz \
  '.runtimeMode == "remote" and .v2Compatibility == "remoteMultiWorker" and (.remoteWorkers | length) == 2 and all(.remoteWorkers[]; .healthy == true) and .remoteAccountLabelsComplete == true and .workerPool.accountCount == 2 and .workerPool.leasedAccountCount == 2 and .workerPool.healthyWorkerSlotCount == 2 and (.remoteWorkers[] | select(.workerId == 0 and .websocketUrl == "ws://127.0.0.1:19001/v2" and .accountId == "acct-a")) and (.remoteWorkers[] | select(.workerId == 1 and .websocketUrl == "ws://127.0.0.1:19002/v2" and .accountId == "acct-b"))' \
  "remote multi-worker gateway health"

create_thread "project-a" >/dev/null
thread_b=$(create_thread "project-b")
thread_b_id=$(printf '%s\n' "$thread_b" | jq -er '.thread.id')
wait_for_healthz \
  '(.projectWorkerRoutes[] | select(.tenantId == "compose-smoke" and .projectId == "project-a" and .workerId == 0)) and (.projectWorkerRoutes[] | select(.tenantId == "compose-smoke" and .projectId == "project-b" and .workerId == 1))' \
  "project routes pinned to separate workers"

compose --profile remote-multi-worker stop app-server-b
wait_for_healthz \
  '(.remoteWorkers[] | select(.workerId == 1 and .healthy == false and .reconnecting == true))' \
  "worker B reconnecting after stop"

if read_thread "project-b" "$thread_b_id"; then
  echo "thread/read unexpectedly succeeded while worker B was stopped" >&2
  exit 1
fi

compose --profile remote-multi-worker up -d --no-build app-server-b
deadline=$(($(date +%s) + 60))
recovered_thread=""
while [ "$(date +%s)" -lt "$deadline" ]; do
  if recovered_thread=$(create_thread "project-b" 2>/dev/null); then
    printf '%s\n' "$recovered_thread" | jq -e '.thread.id | type == "string" and length > 0' >/dev/null
    break
  fi
  sleep 1
done

if [ "${recovered_thread:-}" = "" ]; then
  echo "timed out waiting for thread/start to recover through worker B" >&2
  exit 1
fi

wait_for_healthz \
  '(.remoteWorkers[] | select(.workerId == 1 and .healthy == true and .reconnecting == false))' \
  "worker B reconnect recovery"
