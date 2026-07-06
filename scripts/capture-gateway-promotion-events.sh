#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-gateway-promotion-events.sh --bundle DIR --gateway-url URL [options]
       just gateway-promotion-events-capture -- --bundle DIR --gateway-url URL [options]

Options:
  --prefix VALUE              Artifact filename prefix. Default: 02-events.
  --duration-seconds VALUE    Maximum SSE capture duration. Default: 30.
  --tenant-id VALUE           Override the bundle tenant id.
  --project-id VALUE          Override the first bundle project id.
  --worker-build VALUE        Override the first topology worker build.
  --worker-id VALUE           Override the first topology worker id.
  --account-id VALUE          Override the first topology account id.
  --bearer-token VALUE        Bearer token for the gateway events request.
  --capture-time VALUE        ISO UTC capture timestamp to use in artifact metadata.
  -h, --help
EOF
}

bundle=
gateway_url=
prefix=02-events
duration_seconds=30
tenant_id=
project_id=
worker_build=
worker_id=
account_id=
bearer_token=
capture_time=

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
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$bundle" ] || [ -z "$gateway_url" ]; then
  echo "--bundle and --gateway-url are required" >&2
  usage >&2
  exit 2
fi

for required in README.md events; do
  if [ ! -e "$bundle/$required" ]; then
    echo "missing bundle path: $bundle/$required" >&2
    exit 1
  fi
done

case "$prefix" in
  ""|*/*|*..*)
    echo "--prefix must be a simple artifact filename prefix" >&2
    exit 2
    ;;
esac

case "$duration_seconds" in
  ''|*[!0-9]*)
    echo "--duration-seconds must be a positive integer" >&2
    exit 2
    ;;
esac

if [ "$duration_seconds" -eq 0 ]; then
  echo "--duration-seconds must be greater than zero" >&2
  exit 2
fi

scope_value() {
  path=$1
  prefix_value=$2
  awk -v prefix="$prefix_value" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path" | sed 's/^ *//; s/ *$//'
}

first_topology_field() {
  field=$1
  awk -F'|' -v field="$field" '
    function trim(value) {
      gsub(/^ +| +$/, "", value)
      return value
    }

    /^## Topology$/ { in_topology = 1; next }
    in_topology && /^## / { exit }
    in_topology && /^\|/ {
      worker_id = trim($2)
      build_id = trim($3)
      account = trim($5)

      if (worker_id == "Worker id" || worker_id == "---" || worker_id == "") {
        next
      }

      if (field == "worker_id") {
        print worker_id
      } else if (field == "worker_build") {
        print build_id
      } else if (field == "account_id") {
        print account
      }
      exit
    }
  ' "$bundle/README.md"
}

gateway_build=$(scope_value "$bundle/README.md" '- Gateway build:')
readme_scope=$(scope_value "$bundle/README.md" '- Tenant/project scope:')

if [ -z "$tenant_id" ]; then
  tenant_id=$(printf '%s' "$readme_scope" | sed -n 's/^tenant=\([^,]*\), projects=.*/\1/p')
fi

if [ -z "$project_id" ]; then
  projects=$(printf '%s' "$readme_scope" | sed -n 's/^tenant=[^,]*, projects=\(.*\)$/\1/p')
  project_id=$(printf '%s' "$projects" | awk -F',' '{ gsub(/^ +| +$/, "", $1); print $1 }')
fi

if [ -z "$worker_build" ]; then
  worker_build=$(first_topology_field worker_build)
fi

if [ -z "$worker_id" ]; then
  worker_id=$(first_topology_field worker_id)
fi

if [ -z "$account_id" ]; then
  account_id=$(first_topology_field account_id)
fi

if [ -z "$account_id" ]; then
  account_id=none
fi

for value_name in gateway_build tenant_id project_id worker_build worker_id account_id; do
  value=$(eval "printf '%s' \"\${$value_name}\"")
  if [ -z "$value" ]; then
    echo "could not infer $value_name; pass it explicitly" >&2
    exit 2
  fi
done

if [ -z "$capture_time" ]; then
  capture_time=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
fi
gateway_url=${gateway_url%/}
events_url="$gateway_url/v1/events"
events_path="$bundle/events/$prefix.sse"

{
  cat <<EOF
gateway build: $gateway_build
worker build: $worker_build
tenant id: $tenant_id
project id: $project_id
worker id: $worker_id
account id: $account_id
capture time: $capture_time
EOF

  curl_args=(
    -fsS
    -N
    --max-time "$duration_seconds"
    -H "x-codex-tenant-id: $tenant_id"
    -H "x-codex-project-id: $project_id"
  )
  if [ -n "$bearer_token" ]; then
    curl_args+=(-H "Authorization: Bearer $bearer_token")
  fi

  curl_status=0
  curl "${curl_args[@]}" "$events_url" || curl_status=$?
  if [ "$curl_status" -ne 0 ] && [ "$curl_status" -ne 28 ]; then
    exit "$curl_status"
  fi
} > "$events_path"

printf '%s\n' "$events_path"
