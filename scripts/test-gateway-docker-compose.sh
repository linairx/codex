#!/usr/bin/env sh
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)

default_config="$(docker compose -f "$repo_root/docker-compose.gateway.yml" config)"
remote_config="$(docker compose -f "$repo_root/docker-compose.gateway.yml" --profile remote config)"
dockerignore="$repo_root/.dockerignore"

printf '%s\n' "$default_config" | grep -Fq 'dockerfile: Dockerfile.gateway'
printf '%s\n' "$default_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_AUTH_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'dockerfile: Dockerfile.gateway'
printf '%s\n' "$remote_config" | grep -Fq 'dockerfile: Dockerfile.app-server'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_GATEWAY_REMOTE_AUTH_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'CODEX_APP_SERVER_WS_TOKEN: codex-local-worker-token'
printf '%s\n' "$remote_config" | grep -Fq 'network_mode: service:gateway'
printf '%s\n' "$remote_config" | grep -Fq 'condition: service_started'

grep -Fq '!Dockerfile.app-server' "$dockerignore"
grep -Fq '!scripts/' "$dockerignore"
grep -Fq '!scripts/**' "$dockerignore"
