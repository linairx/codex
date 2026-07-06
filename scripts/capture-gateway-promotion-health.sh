#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/capture-gateway-promotion-health.sh --bundle DIR --gateway-url URL [options]
       just gateway-promotion-health-capture -- --bundle DIR --gateway-url URL [options]

Options:
  --prefix VALUE          Artifact filename prefix. Default: 02-steady-state-project-a.
  --scenario VALUE        Evidence scenario row to update. Default: Steady-state bootstrap and thread/turn.
  --tenant-id VALUE       Override the bundle tenant id.
  --project-id VALUE      Override the first bundle project id.
  --worker-build VALUE    Override the first topology worker build.
  --worker-id VALUE       Override the first topology worker id.
  --account-id VALUE      Override the first topology account id.
  --bearer-token VALUE    Bearer token for the gateway health request.
  --capture-time VALUE    ISO UTC capture timestamp to use in artifact metadata.
  --no-row-update         Only write the health artifact.
  -h, --help
EOF
}

bundle=
gateway_url=
prefix=02-steady-state-project-a
scenario="Steady-state bootstrap and thread/turn"
tenant_id=
project_id=
worker_build=
worker_id=
account_id=
bearer_token=
capture_time=
row_update=true

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
    --no-row-update)
      row_update=false
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

if [ -z "$bundle" ] || [ -z "$gateway_url" ]; then
  echo "--bundle and --gateway-url are required" >&2
  usage >&2
  exit 2
fi

for required in README.md worksheet.md healthz; do
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
health_url="$gateway_url/healthz"
health_body=$(mktemp)
trap 'rm -f "$health_body"' EXIT

curl_args=(-fsS -H "x-codex-tenant-id: $tenant_id" -H "x-codex-project-id: $project_id")
if [ -n "$bearer_token" ]; then
  curl_args+=(-H "Authorization: Bearer $bearer_token")
fi

if ! curl "${curl_args[@]}" "$health_url" > "$health_body"; then
  echo "failed to fetch $health_url" >&2
  exit 1
fi

health_path="$bundle/healthz/$prefix.json"
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
  cat "$health_body"
} > "$health_path"

replace_scenario_row() {
  path=$1
  replacement=$2
  tmp=$(mktemp)
  awk -v scenario="$scenario" -v replacement="$replacement" '
    BEGIN { replaced = 0 }
    $0 ~ "^\\| " scenario " \\|" {
      print replacement
      replaced = 1
      next
    }
    { print }
    END {
      if (!replaced) {
        print "missing scenario row: " scenario > "/dev/stderr"
        exit 1
      }
    }
  ' "$path" > "$tmp"
  mv "$tmp" "$path"
}

scenario_row_number() {
  path=$1
  awk -F'|' -v scenario="$scenario" '
    function trim(value) {
      gsub(/^ +| +$/, "", value)
      return value
    }

    /^## (Evidence Index|Captures)$/ { in_captures = 1; next }
    in_captures && /^## / { exit }

    in_captures && /^\|/ {
      row_scenario = trim($2)
      if (row_scenario == "Scenario" || row_scenario == "---" || row_scenario == "") {
        next
      }
      row_number += 1
      if (row_scenario == scenario) {
        print row_number
        found = 1
        exit
      }
    }
    END {
      if (!found) {
        print "missing scenario row: " scenario > "/dev/stderr"
        exit 1
      }
    }
  ' "$path"
}

if [ "$row_update" = true ]; then
  worksheet_row_number=$(scenario_row_number "$bundle/README.md")
  replace_scenario_row \
    "$bundle/README.md" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | worksheet row $worksheet_row_number |"
  replace_scenario_row \
    "$bundle/worksheet.md" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | captured health |"
fi

printf '%s\n' "$health_path"
