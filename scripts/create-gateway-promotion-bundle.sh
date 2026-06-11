#!/usr/bin/env sh
set -eu

usage() {
  cat <<'EOF'
Usage: scripts/create-gateway-promotion-bundle.sh --output DIR --gateway-build ID --topology-id ID [options]

Options:
  --worker-build VALUE                         Repeat for each worker build.
  --worker-url VALUE                           Repeat for each worker WebSocket URL.
  --account-id VALUE                           Repeat for each worker account id.
  --tenant-id VALUE
  --project-id VALUE                           Repeat for each project.
  --auth-mode VALUE
  --v2-initialize-timeout-seconds VALUE
  --v2-client-send-timeout-seconds VALUE
  --v2-reconnect-retry-backoff-seconds VALUE
  --v2-max-pending-server-requests VALUE
  --v2-max-pending-client-requests VALUE
  --force                                      Overwrite existing template files.
  -h, --help
EOF
}

append_value() {
  if [ -z "$1" ]; then
    printf '%s' "$2"
  else
    printf '%s, %s' "$1" "$2"
  fi
}

scenario_rows() {
  cat <<'EOF'
| Baseline before traffic | | | | | | |
| Steady-state bootstrap and thread/turn | | | | | | |
| Project route selection | | | | | | |
| Reconnect and recovery | | | | | | |
| Degraded-route fail-closed | | | | | | |
| Account-capacity transition | | | | | | |
| Bounded restoration | | | | | | |
| Live active-context no-handoff | | | | | | |
| Slow-client or backlog window | | | | | | |
| Cleanup or delivery failure | | | | | | |
EOF
}

output=
gateway_build=
topology_id=
worker_builds=
worker_urls=
account_ids=
worker_rows=
tenant_id=
project_ids=
auth_mode=
v2_initialize_timeout_seconds=
v2_client_send_timeout_seconds=
v2_reconnect_retry_backoff_seconds=
v2_max_pending_server_requests=
v2_max_pending_client_requests=
force=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output)
      output=$2
      shift 2
      ;;
    --gateway-build)
      gateway_build=$2
      shift 2
      ;;
    --topology-id)
      topology_id=$2
      shift 2
      ;;
    --worker-build)
      worker_builds=$(append_value "$worker_builds" "$2")
      shift 2
      ;;
    --worker-url)
      worker_urls=$(append_value "$worker_urls" "$2")
      shift 2
      ;;
    --account-id)
      account_ids=$(append_value "$account_ids" "$2")
      shift 2
      ;;
    --tenant-id)
      tenant_id=$2
      shift 2
      ;;
    --project-id)
      project_ids=$(append_value "$project_ids" "$2")
      shift 2
      ;;
    --auth-mode)
      auth_mode=$2
      shift 2
      ;;
    --v2-initialize-timeout-seconds)
      v2_initialize_timeout_seconds=$2
      shift 2
      ;;
    --v2-client-send-timeout-seconds)
      v2_client_send_timeout_seconds=$2
      shift 2
      ;;
    --v2-reconnect-retry-backoff-seconds)
      v2_reconnect_retry_backoff_seconds=$2
      shift 2
      ;;
    --v2-max-pending-server-requests)
      v2_max_pending_server_requests=$2
      shift 2
      ;;
    --v2-max-pending-client-requests)
      v2_max_pending_client_requests=$2
      shift 2
      ;;
    --force)
      force=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$output" ] || [ -z "$gateway_build" ] || [ -z "$topology_id" ]; then
  echo "--output, --gateway-build, and --topology-id are required" >&2
  usage >&2
  exit 2
fi

if [ -z "$worker_urls" ]; then
  echo "at least one --worker-url is required" >&2
  usage >&2
  exit 2
fi

worker_rows=$(
  awk -v worker_urls="$worker_urls" -v account_ids="$account_ids" -v auth_mode="$auth_mode" '
    BEGIN {
      worker_count = split(worker_urls, workers, /, */);
      account_count = split(account_ids, accounts, /, */);

      if (account_count > worker_count) {
        print "more --account-id values than --worker-url values" > "/dev/stderr";
        exit 2;
      }

      for (i = 1; i <= worker_count; i++) {
        account_id = (i <= account_count ? accounts[i] : "");
        printf("| %d | %s | %s | %s | |\n", i - 1, workers[i], account_id, auth_mode);
      }
    }
  '
)

bundle="$output/$gateway_build/$topology_id"
if [ -d "$bundle" ] && [ "$(find "$bundle" -mindepth 1 -maxdepth 1 | wc -l)" -gt 0 ] && [ "$force" != true ]; then
  echo "$bundle is not empty; pass --force to write into it" >&2
  exit 1
fi

mkdir -p "$bundle/transcripts" "$bundle/healthz" "$bundle/events" "$bundle/metrics" "$bundle/logs"

write_template() {
  path=$1
  if [ -e "$path" ] && [ "$force" != true ]; then
    echo "$path already exists; pass --force to overwrite" >&2
    exit 1
  fi
  cat > "$path"
}

{
  cat <<EOF
# Gateway Promotion Evidence Bundle

- Gateway build: $gateway_build
- Worker builds: $worker_builds
- Captured by:
- Capture start:
- Capture end:
- Decision file: decision.md

## Topology

| Worker id | WebSocket URL | Account id | Auth mode | Notes |
| --- | --- | --- | --- | --- |
$worker_rows

## Runtime Configuration

- Gateway listen address:
- Northbound auth: $auth_mode
- Downstream auth:
- v2 initialize timeout: $v2_initialize_timeout_seconds
- v2 client send timeout: $v2_client_send_timeout_seconds
- v2 reconnect retry backoff: $v2_reconnect_retry_backoff_seconds
- v2 max pending server requests: $v2_max_pending_server_requests
- v2 max pending client requests: $v2_max_pending_client_requests

## Evidence Index

| Scenario | Transcript | Health | Events | Metrics | Logs | Worksheet row |
| --- | --- | --- | --- | --- | --- | --- |
EOF
  scenario_rows
} | write_template "$bundle/README.md"

{
  cat <<EOF
# Gateway Multi-Worker Promotion Evidence

## Scope

- Gateway build: $gateway_build
- Worker builds: $worker_builds
- Worker URLs: $worker_urls
- Account labels: $account_ids
- Auth mode: $auth_mode
- v2 timeout values: initialize=$v2_initialize_timeout_seconds, client_send=$v2_client_send_timeout_seconds, reconnect_backoff=$v2_reconnect_retry_backoff_seconds
- Pending request limits: server=$v2_max_pending_server_requests, client=$v2_max_pending_client_requests
- Tenant/project scope: tenant=$tenant_id, projects=$project_ids
- Method matrix route classes covered:

## Captures

| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |
| --- | --- | --- | --- | --- | --- | --- |
EOF
  scenario_rows
  cat <<'EOF'

## Reconciliation

- Route-selection identities agree across health, events, metrics, and audit:
- Account-capacity identities agree across health, events, metrics, and logs:
- Bounded handoff success/failure outcomes match client-visible behavior:
- Live active-context methods fail closed instead of moving accounts:
- Backlog and cleanup windows are bounded and observable:
EOF
} | write_template "$bundle/worksheet.md"

{
  cat <<EOF
# Gateway Multi-Worker Promotion Decision

- Decision:
- Promotion scope:
- Topology id: $topology_id
- Gateway build: $gateway_build
- Worker builds: $worker_builds
- Tenant/project scopes: tenant=$tenant_id, projects=$project_ids
- Method families included:
- Method families excluded:

## Reconciliation Summary

- Route-selection evidence:
- Account-capacity evidence:
- Bounded restoration evidence:
- Live active-context no-handoff evidence:
- Reconnect/degraded-route evidence:
- Slow-client/backlog evidence:
- Cleanup/delivery-failure evidence:

## Blocking Mismatches

| Scenario | Expected | Observed | Affected ids | Required follow-up |
| --- | --- | --- | --- | --- |
| | | | | |

## Invalidation Rules

This decision applies only to the topology, builds, account labels, auth mode,
timeout values, pending-request limits, and method families listed above.
EOF
} | write_template "$bundle/decision.md"

printf '%s\n' "$bundle"
