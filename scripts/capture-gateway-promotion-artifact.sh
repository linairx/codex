#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-gateway-promotion-artifact.sh --bundle DIR --kind KIND --prefix VALUE [options]
       just gateway-promotion-artifact-capture -- --bundle DIR --kind KIND --prefix VALUE [options]

Options:
  --kind VALUE          One of transcript, metrics, or logs.
  --source FILE         Read artifact body from FILE. Defaults to stdin.
  --tenant-id VALUE     Override the bundle tenant id.
  --project-id VALUE    Override the first bundle project id.
  --worker-build VALUE  Override the first topology worker build.
  --worker-id VALUE     Override the first topology worker id.
  --account-id VALUE    Override the first topology account id.
  --capture-time VALUE  ISO UTC capture timestamp to use in artifact metadata.
  -h, --help
EOF
}

bundle=
kind=
prefix=
source=
tenant_id=
project_id=
worker_build=
worker_id=
account_id=
capture_time=

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bundle)
      bundle=$2
      shift 2
      ;;
    --kind)
      kind=$2
      shift 2
      ;;
    --prefix)
      prefix=$2
      shift 2
      ;;
    --source)
      source=$2
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

if [ -z "$bundle" ] || [ -z "$kind" ] || [ -z "$prefix" ]; then
  echo "--bundle, --kind, and --prefix are required" >&2
  usage >&2
  exit 2
fi

case "$prefix" in
  ""|*/*|*..*)
    echo "--prefix must be a simple artifact filename prefix" >&2
    exit 2
    ;;
esac

case "$kind" in
  transcript)
    directory=transcripts
    extension=txt
    ;;
  metrics)
    directory=metrics
    extension=json
    ;;
  logs)
    directory=logs
    extension=log
    ;;
  *)
    echo "--kind must be one of: transcript, metrics, logs" >&2
    exit 2
    ;;
esac

for required in README.md "$directory"; do
  if [ ! -e "$bundle/$required" ]; then
    echo "missing bundle path: $bundle/$required" >&2
    exit 1
  fi
done

if [ -n "$source" ] && [ ! -f "$source" ]; then
  echo "missing source file: $source" >&2
  exit 1
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

if [ -z "$capture_time" ]; then
  capture_time=$(date -u '+%Y-%m-%dT%H:%M:%SZ')
fi

for value_name in gateway_build tenant_id project_id worker_build worker_id account_id capture_time; do
  value=$(eval "printf '%s' \"\${$value_name}\"")
  if [ -z "$value" ]; then
    echo "could not infer $value_name; pass it explicitly" >&2
    exit 2
  fi
done

artifact_path="$bundle/$directory/$prefix.$extension"
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
  if [ -n "$source" ]; then
    cat "$source"
  else
    cat
  fi
} > "$artifact_path"

printf '%s\n' "$artifact_path"
