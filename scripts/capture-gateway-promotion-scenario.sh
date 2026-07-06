#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

usage() {
  cat <<'EOF'
Usage: scripts/capture-gateway-promotion-scenario.sh --bundle DIR --gateway-url URL --prefix VALUE --scenario VALUE --metrics-source FILE --logs-source FILE [options] [-- COMMAND...]
       just gateway-promotion-scenario-capture -- --bundle DIR --gateway-url URL --prefix VALUE --scenario VALUE --metrics-source FILE --logs-source FILE [options] [-- COMMAND...]

Options:
  --transcript-source FILE    Import transcript body from FILE. Required unless COMMAND is provided.
  --metrics-source FILE       Import metrics body from FILE.
  --logs-source FILE          Import logs body from FILE.
  --duration-seconds VALUE    /v1/events capture duration. Default: 120.
  --tenant-id VALUE           Override the bundle tenant id.
  --project-id VALUE          Override the first bundle project id.
  --worker-build VALUE        Override the first topology worker build.
  --worker-id VALUE           Override the first topology worker id.
  --account-id VALUE          Override the first topology account id.
  --bearer-token VALUE        Bearer token for gateway health/events requests.
  --capture-time VALUE        ISO UTC capture timestamp to use for all scenario artifacts.
  -h, --help

COMMAND stdout/stderr is captured as the transcript artifact when
--transcript-source is omitted.
EOF
}

bundle=
gateway_url=
prefix=
scenario=
transcript_source=
metrics_source=
logs_source=
duration_seconds=120
tenant_id=
project_id=
worker_build=
worker_id=
account_id=
bearer_token=
capture_time=
scenario_command=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bundle)
      bundle=$2
      shift 2
      ;;
    --gateway-url)
      gateway_url=$2
      shift 2
      ;;
    --prefix)
      prefix=$2
      shift 2
      ;;
    --scenario)
      scenario=$2
      shift 2
      ;;
    --transcript-source)
      transcript_source=$2
      shift 2
      ;;
    --metrics-source)
      metrics_source=$2
      shift 2
      ;;
    --logs-source)
      logs_source=$2
      shift 2
      ;;
    --duration-seconds)
      duration_seconds=$2
      shift 2
      ;;
    --tenant-id)
      tenant_id=$2
      shift 2
      ;;
    --project-id)
      project_id=$2
      shift 2
      ;;
    --worker-build)
      worker_build=$2
      shift 2
      ;;
    --worker-id)
      worker_id=$2
      shift 2
      ;;
    --account-id)
      account_id=$2
      shift 2
      ;;
    --bearer-token)
      bearer_token=$2
      shift 2
      ;;
    --capture-time)
      capture_time=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      scenario_command=("$@")
      break
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$bundle" ] || [ -z "$gateway_url" ] || [ -z "$prefix" ] || [ -z "$scenario" ]; then
  echo "--bundle, --gateway-url, --prefix, and --scenario are required" >&2
  usage >&2
  exit 2
fi

if [ -z "$metrics_source" ] || [ -z "$logs_source" ]; then
  echo "--metrics-source and --logs-source are required" >&2
  usage >&2
  exit 2
fi

if [ -z "$transcript_source" ] && [ "${#scenario_command[@]}" -eq 0 ]; then
  echo "--transcript-source is required when COMMAND is not provided" >&2
  usage >&2
  exit 2
fi

case "$duration_seconds" in
  ''|*[!0-9]*)
    echo "--duration-seconds must be a positive integer" >&2
    exit 2
    ;;
esac
if [ "$duration_seconds" -le 0 ]; then
  echo "--duration-seconds must be a positive integer" >&2
  exit 2
fi

for source in "$metrics_source" "$logs_source"; do
  if [ ! -f "$source" ]; then
    echo "missing source file: $source" >&2
    exit 1
  fi
done
if [ -n "$transcript_source" ] && [ ! -f "$transcript_source" ]; then
  echo "missing source file: $transcript_source" >&2
  exit 1
fi

if [ -z "$capture_time" ]; then
  capture_time=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
fi

common_args=(
  --bundle "$bundle"
  --prefix "$prefix"
  --capture-time "$capture_time"
)
scope_args=()
if [ -n "$tenant_id" ]; then
  scope_args+=(--tenant-id "$tenant_id")
fi
if [ -n "$project_id" ]; then
  scope_args+=(--project-id "$project_id")
fi
if [ -n "$worker_build" ]; then
  scope_args+=(--worker-build "$worker_build")
fi
if [ -n "$worker_id" ]; then
  scope_args+=(--worker-id "$worker_id")
fi
if [ -n "$account_id" ]; then
  scope_args+=(--account-id "$account_id")
fi
auth_args=()
if [ -n "$bearer_token" ]; then
  auth_args+=(--bearer-token "$bearer_token")
fi

tmp_dir=$(mktemp -d)
events_status="$tmp_dir/events.status"
command_status=0
trap 'rm -rf "$tmp_dir"' EXIT

"$repo_root/scripts/capture-gateway-promotion-events.sh" \
  "${common_args[@]}" \
  "${scope_args[@]}" \
  "${auth_args[@]}" \
  --gateway-url "$gateway_url" \
  --duration-seconds "$duration_seconds" > "$tmp_dir/events.path" 2> "$tmp_dir/events.err" &
events_pid=$!

if [ "${#scenario_command[@]}" -gt 0 ]; then
  transcript_source="$tmp_dir/transcript.txt"
  set +e
  "${scenario_command[@]}" > "$transcript_source" 2>&1
  command_status=$?
  set -e
fi

if ! wait "$events_pid"; then
  cat "$tmp_dir/events.err" >&2
  echo "failed to capture scenario events" >&2
  exit 1
fi
printf 'ok\n' > "$events_status"

if [ "$command_status" -ne 0 ]; then
  echo "scenario command failed with exit code $command_status" >&2
  exit "$command_status"
fi

"$repo_root/scripts/capture-gateway-promotion-health.sh" \
  "${common_args[@]}" \
  "${scope_args[@]}" \
  "${auth_args[@]}" \
  --gateway-url "$gateway_url" \
  --scenario "$scenario" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  "${common_args[@]}" \
  "${scope_args[@]}" \
  --kind transcript \
  --source "$transcript_source" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  "${common_args[@]}" \
  "${scope_args[@]}" \
  --kind metrics \
  --source "$metrics_source" >/dev/null

"$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
  "${common_args[@]}" \
  "${scope_args[@]}" \
  --kind logs \
  --source "$logs_source" >/dev/null

printf '%s\n' "$bundle"
