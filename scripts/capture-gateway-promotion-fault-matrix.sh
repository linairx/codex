#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

usage() {
  cat <<'EOF'
Usage: scripts/capture-gateway-promotion-fault-matrix.sh --bundle DIR [options]
       just gateway-promotion-fault-matrix-capture -- --bundle DIR [options]

Runs the codex-gateway fault-injection test matrix for the remaining
multi-worker promotion scenarios and imports repeatable evidence summaries into
an existing promotion bundle.

Options:
  --capture-time VALUE  ISO UTC timestamp for generated artifacts. Defaults to
                        the bundle README capture end timestamp.
  --skip-tests          Generate artifacts from the mapped test names without
                        executing the tests. Intended only for script smoke tests.
  --dry-run             Print the mapped scenarios and tests without writing.
  -h, --help
EOF
}

bundle=
capture_time=
skip_tests=false
dry_run=false

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bundle)
      bundle=$2
      shift 2
      ;;
    --capture-time)
      capture_time=$2
      shift 2
      ;;
    --skip-tests)
      skip_tests=true
      shift
      ;;
    --dry-run)
      dry_run=true
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

if [ -z "$bundle" ]; then
  echo "--bundle is required" >&2
  usage >&2
  exit 2
fi

for required in README.md worksheet.md decision.md healthz events metrics logs transcripts; do
  if [ ! -e "$bundle/$required" ]; then
    echo "missing bundle path: $bundle/$required" >&2
    exit 1
  fi
done

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
tenant_id=$(printf '%s' "$readme_scope" | sed -n 's/^tenant=\([^,]*\), projects=.*/\1/p')
projects=$(printf '%s' "$readme_scope" | sed -n 's/^tenant=[^,]*, projects=\(.*\)$/\1/p')
project_id=$(printf '%s' "$projects" | awk -F',' '{ gsub(/^ +| +$/, "", $1); print $1 }')
worker_build=$(first_topology_field worker_build)
worker_id=$(first_topology_field worker_id)
account_id=$(first_topology_field account_id)

if [ -z "$account_id" ]; then
  account_id=none
fi

if [ -z "$capture_time" ]; then
  capture_time=$(scope_value "$bundle/README.md" '- Capture end:')
fi

for value_name in gateway_build tenant_id project_id worker_build worker_id account_id capture_time; do
  value=$(eval "printf '%s' \"\${$value_name}\"")
  if [ -z "$value" ]; then
    echo "could not infer $value_name; pass --capture-time or fix bundle metadata" >&2
    exit 2
  fi
done

write_metadata_file() {
  path=$1
  body=$2
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
    printf '%s\n' "$body"
  } > "$path"
}

capture_artifact() {
  kind=$1
  prefix=$2
  source=$3
  "$repo_root/scripts/capture-gateway-promotion-artifact.sh" \
    --bundle "$bundle" \
    --kind "$kind" \
    --prefix "$prefix" \
    --source "$source" \
    --capture-time "$capture_time" >/dev/null
}

run_test() {
  filter=$1
  output=$2
  {
    printf 'command: just test -p codex-gateway %s\n' "$filter"
    printf 'scenario capture time: %s\n\n' "$capture_time"
  } > "$output"

  if [ "$skip_tests" = true ]; then
    printf 'skipped test execution by --skip-tests\n' >> "$output"
    return 0
  fi

  set +e
  (
    cd "$repo_root"
    just test -p codex-gateway "$filter"
  ) >> "$output" 2>&1
  status=$?
  return "$status"
}

replace_scenario_row() {
  path=$1
  scenario=$2
  replacement=$3
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
  scenario=$2
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

scenario_matrix=$(cat <<'EOF'
04-reconnect-recovery|Reconnect and recovery|websocket_upgrade_reconnects_missing_worker_before_turn_start|worker reconnects before turn/start and preserves sticky routing|gateway/v2WorkerReconnect, outcome=success
05-degraded-route-fail-closed|Degraded-route fail-closed|visible_thread_route_recovery_fails_closed_during_reconnect_backoff|thread route recovery fails closed while reconnect backoff is active|gateway/v2FailClosedRequest, reconnectBackoffActive=true
06-account-capacity-transition|Account-capacity transition|thread_start_marks_exhausted_account_and_retries_available_account|thread/start marks the exhausted account and retries an eligible account|gateway/accountCapacityChanged, accountCapacity=exhausted
07-bounded-restoration|Bounded restoration|thread_read_uses_replacement_worker_when_pinned_account_is_exhausted|thread/read restores bounded context through a replacement worker|gateway/accountThreadHandoffSucceeded
08-live-active-context-no-handoff|Live active-context no-handoff|active_thread_account_exhaustion_publishes_handoff_failure_event|active thread methods publish handoff failure instead of moving live context|gateway/accountActiveThreadHandoffFailed
09-slow-client-backlog-window|Slow-client or backlog window|observe_v2_connection_records_client_send_timeout_outcome|slow client exposes bounded server-request backlog and send timeout health|gateway/v2ConnectionServerRequestBacklog, outcome=client_send_timed_out
10-cleanup-delivery-failure|Cleanup or delivery failure|websocket_upgrade_logs_worker_cleanup_ids_when_worker_disconnects_with_pending_connection_server_request|worker disconnect cleanup emits cleanup ids and delivery-failure accounting|gateway/v2ServerRequestCleanup
EOF
)

if [ "$dry_run" = true ]; then
  printf '%s\n' "$scenario_matrix"
  exit 0
fi

tmp_dir=$(mktemp -d)
trap 'rm -rf "$tmp_dir"' EXIT
overall_status=0

while IFS='|' read -r prefix scenario test_filter summary event_summary; do
  [ -n "$prefix" ] || continue

  transcript_source="$tmp_dir/$prefix.transcript.txt"
  metrics_source="$tmp_dir/$prefix.metrics.json"
  logs_source="$tmp_dir/$prefix.logs.log"

  set +e
  run_test "$test_filter" "$transcript_source"
  test_status=$?
  set -e
  if [ "$skip_tests" = true ]; then
    result=skipped
  elif [ "$test_status" -eq 0 ]; then
    result=passed
  else
    result=failed
    overall_status=1
  fi

  cat > "$metrics_source" <<EOF
{
  "scenario": "$scenario",
  "source": "codex-gateway targeted test",
  "testFilter": "$test_filter",
  "summary": "$summary",
  "eventSummary": "$event_summary",
  "result": "$result"
}
EOF

  cat > "$logs_source" <<EOF
codex_gateway.audit promotion fault matrix scenario="$scenario" test_filter="$test_filter" summary="$summary" event_summary="$event_summary" result="$result"
EOF

  capture_artifact transcript "$prefix" "$transcript_source"
  capture_artifact metrics "$prefix" "$metrics_source"
  capture_artifact logs "$prefix" "$logs_source"

  write_metadata_file "$bundle/healthz/$prefix.json" "$(cat <<EOF
{
  "scenario": "$scenario",
  "source": "codex-gateway targeted test",
  "testFilter": "$test_filter",
  "summary": "$summary",
  "result": "$result"
}
EOF
)"

  write_metadata_file "$bundle/events/$prefix.sse" "$(cat <<EOF
event: gateway/promotionFaultMatrix
data: {"scenario":"$scenario","testFilter":"$test_filter","summary":"$summary","eventSummary":"$event_summary","result":"$result"}
EOF
)"

  worksheet_row_number=$(scenario_row_number "$bundle/README.md" "$scenario")
  replace_scenario_row \
    "$bundle/README.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | worksheet row $worksheet_row_number |"
  replace_scenario_row \
    "$bundle/worksheet.md" \
    "$scenario" \
    "| $scenario | transcripts/$prefix.txt | healthz/$prefix.json | events/$prefix.sse | metrics/$prefix.json | logs/$prefix.log | targeted test $result |"
done <<< "$scenario_matrix"

if [ "$overall_status" -ne 0 ]; then
  echo "one or more codex-gateway fault-matrix tests failed; artifacts were captured in $bundle" >&2
  exit "$overall_status"
fi

"$repo_root/scripts/check-gateway-promotion-bundle.sh" "$bundle" >/dev/null
printf '%s\n' "$bundle"
