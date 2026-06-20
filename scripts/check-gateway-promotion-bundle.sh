#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/check-gateway-promotion-bundle.sh BUNDLE_DIR
       just gateway-promotion-bundle-check -- BUNDLE_DIR
EOF
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

if [ "$#" -ne 1 ]; then
  usage >&2
  exit 2
fi

bundle=$1

if [ ! -d "$bundle" ]; then
  echo "missing bundle directory: $bundle" >&2
  exit 1
fi

require_file() {
  path=$1
  if [ ! -f "$path" ]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_dir() {
  path=$1
  if [ ! -d "$path" ]; then
    echo "missing required directory: $path" >&2
    exit 1
  fi
}

require_nonempty_dir() {
  path=$1
  if ! find "$path" -mindepth 1 -type f -print -quit | grep -q .; then
    echo "missing captured files in directory: $path" >&2
    exit 1
  fi
}

require_file "$bundle/README.md"
require_file "$bundle/worksheet.md"
require_file "$bundle/decision.md"
require_dir "$bundle/transcripts"
require_dir "$bundle/healthz"
require_dir "$bundle/events"
require_dir "$bundle/metrics"
require_dir "$bundle/logs"
require_nonempty_dir "$bundle/transcripts"
require_nonempty_dir "$bundle/healthz"
require_nonempty_dir "$bundle/events"
require_nonempty_dir "$bundle/metrics"
require_nonempty_dir "$bundle/logs"

require_line() {
  path=$1
  expected=$2
  if ! grep -Fq "$expected" "$path"; then
    echo "missing required content in $path: $expected" >&2
    exit 1
  fi
}

evidence_index_scenarios=$(mktemp)
worksheet_scenarios=$(mktemp)
evidence_index_refs=$(mktemp)
worksheet_refs=$(mktemp)
evidence_index_scenario_set=$(mktemp)
worksheet_scenario_set=$(mktemp)
trap 'rm -f "$evidence_index_scenarios" "$worksheet_scenarios" "$evidence_index_refs" "$worksheet_refs" "$evidence_index_scenario_set" "$worksheet_scenario_set"' EXIT

require_line "$bundle/README.md" '# Gateway Promotion Evidence Bundle'
require_line "$bundle/README.md" '## Topology'
require_line "$bundle/README.md" '## Runtime Configuration'
require_line "$bundle/README.md" '## Evidence Index'
require_line "$bundle/README.md" '| Worker id | Build id | WebSocket URL | Account id | Auth mode | Notes |'
require_line "$bundle/worksheet.md" '# Gateway Multi-Worker Promotion Evidence'
require_line "$bundle/worksheet.md" '## Scope'
require_line "$bundle/worksheet.md" '## Captures'
require_line "$bundle/worksheet.md" '## Reconciliation'
require_line "$bundle/worksheet.md" '## Decision'
require_line "$bundle/worksheet.md" '| Scenario | Client transcript | Health snapshot | Events | Metrics | Logs | Result |'
require_line "$bundle/decision.md" '# Gateway Multi-Worker Promotion Decision'
require_line "$bundle/decision.md" '## Reconciliation Summary'
require_line "$bundle/decision.md" '## Blocking Mismatches'
require_line "$bundle/decision.md" '## Invalidation Rules'

reject_placeholder_value() {
  path=$1
  label=$2
  value=$3
  case ${value,,} in
    *tbd*|*todo*|*fixme*|*placeholder*)
      echo "placeholder value in $path: $label$value" >&2
      exit 1
      ;;
  esac
}

require_populated_field() {
  path=$1
  prefix=$2
  value=$(awk -v prefix="$prefix" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path")
  value=$(printf '%s' "$value" | sed 's/^ *//; s/ *$//')
  if [ -z "$value" ]; then
    echo "missing populated field in $path: $prefix" >&2
    exit 1
  fi
  reject_placeholder_value "$path" "$prefix" "$value"
}

reject_release_quality_gap() {
  path=$1
  field=$2
  value=$3
  case ${value,,} in
    *"not exercised"*|*"not validated"*|*"untested"*|*"missing evidence"*|*"excluded"*)
      echo "release-quality multi-worker decisions cannot include unvalidated evidence markers in $path: $field$value" >&2
      exit 1
      ;;
  esac
}

comma_list_contains() {
  haystack=$1
  needle=$2
  printf '%s\n' "$haystack" | tr ',' '\n' | sed 's/^ *//; s/ *$//' | grep -Fxq "$needle"
}

normalize_account_id() {
  value=$1
  case ${value,,} in
    none|"<none>")
      printf ''
      ;;
    *)
      printf '%s' "$value"
      ;;
  esac
}

require_populated_field "$bundle/decision.md" '- Decision:'
require_populated_field "$bundle/decision.md" '- Promotion scope:'
require_populated_field "$bundle/decision.md" '- Topology id:'
require_populated_field "$bundle/decision.md" '- Gateway build:'
require_populated_field "$bundle/decision.md" '- Worker builds:'
require_populated_field "$bundle/decision.md" '- Tenant/project scopes:'
require_populated_field "$bundle/decision.md" '- Method families included:'
require_populated_field "$bundle/decision.md" '- Method families excluded:'
require_populated_field "$bundle/decision.md" '- Route-selection evidence:'
require_populated_field "$bundle/decision.md" '- Account-capacity evidence:'
require_populated_field "$bundle/decision.md" '- Bounded restoration evidence:'
require_populated_field "$bundle/decision.md" '- Live active-context no-handoff evidence:'
require_populated_field "$bundle/decision.md" '- Reconnect/degraded-route evidence:'
require_populated_field "$bundle/decision.md" '- Slow-client/backlog evidence:'
require_populated_field "$bundle/decision.md" '- Cleanup/delivery-failure evidence:'
require_populated_field "$bundle/README.md" '- Captured by:'
require_populated_field "$bundle/README.md" '- Topology id:'
require_populated_field "$bundle/README.md" '- Worker builds:'
require_populated_field "$bundle/README.md" '- Tenant/project scope:'
require_populated_field "$bundle/README.md" '- Capture start:'
require_populated_field "$bundle/README.md" '- Capture end:'
require_populated_field "$bundle/README.md" '- Decision file:'
require_populated_field "$bundle/worksheet.md" '- Gateway build:'
require_populated_field "$bundle/worksheet.md" '- Topology id:'
require_populated_field "$bundle/worksheet.md" '- Worker builds:'
require_populated_field "$bundle/worksheet.md" '- Worker URLs:'
require_populated_field "$bundle/worksheet.md" '- Account labels:'
require_populated_field "$bundle/worksheet.md" '- Auth mode:'
require_populated_field "$bundle/worksheet.md" '- v2 timeout values:'
require_populated_field "$bundle/worksheet.md" '- Pending request limits:'
require_populated_field "$bundle/worksheet.md" '- Tenant/project scope:'
require_populated_field "$bundle/worksheet.md" '- Method matrix route classes covered:'
require_populated_field "$bundle/worksheet.md" '- Route-selection identities agree across health, events, metrics, and audit:'
require_populated_field "$bundle/worksheet.md" '- Account-capacity identities agree across health, events, metrics, and logs:'
require_populated_field "$bundle/worksheet.md" '- Bounded handoff success/failure outcomes match client-visible behavior:'
require_populated_field "$bundle/worksheet.md" '- Live active-context methods fail closed instead of moving accounts:'
require_populated_field "$bundle/worksheet.md" '- Backlog and cleanup windows are bounded and observable:'
require_populated_field "$bundle/worksheet.md" '- Decision:'
require_populated_field "$bundle/worksheet.md" '- Promotion scope:'
require_populated_field "$bundle/worksheet.md" '- Excluded method families or route classes:'
require_populated_field "$bundle/worksheet.md" '- Follow-up required before wider rollout:'

scope_value() {
  path=$1
  prefix=$2
  awk -v prefix="$prefix" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path" | sed 's/^ *//; s/ *$//'
}

topology_field_values() {
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
      websocket_url = trim($4)
      account_id = trim($5)

      if (worker_id == "Worker id" || worker_id == "---" || worker_id == "") {
        next
      }

      if (field == "builds") {
        values = values (values == "" ? "" : ", ") build_id
      } else if (field == "urls") {
        values = values (values == "" ? "" : ", ") websocket_url
      } else if (field == "accounts" && account_id != "") {
        values = values (values == "" ? "" : ", ") account_id
      }
    }
    END { print values }
  ' "$bundle/README.md"
}

topology_unlabeled_worker_count() {
  awk -F'|' '
    function trim(value) {
      gsub(/^ +| +$/, "", value)
      return value
    }

    /^## Topology$/ { in_topology = 1; next }
    in_topology && /^## / { exit }
    in_topology && /^\|/ {
      worker_id = trim($2)
      account_id = trim($5)

      if (worker_id == "Worker id" || worker_id == "---" || worker_id == "") {
        next
      }

      if (account_id == "") {
        count += 1
      }
    }
    END { print count + 0 }
  ' "$bundle/README.md"
}

topology_has_worker_account_pair() {
  expected_worker_build=$1
  expected_account_id=$(normalize_account_id "$2")
  awk -F'|' -v expected_worker_build="$expected_worker_build" -v expected_account_id="$expected_account_id" '
    function trim(value) {
      gsub(/^ +| +$/, "", value)
      return value
    }

    /^## Topology$/ { in_topology = 1; next }
    in_topology && /^## / { exit }
    in_topology && /^\|/ {
      worker_id = trim($2)
      build_id = trim($3)
      account_id = trim($5)

      if (worker_id == "Worker id" || worker_id == "---" || worker_id == "") {
        next
      }

      if (build_id == expected_worker_build && account_id == expected_account_id) {
        found = 1
      }
    }
    END { exit found ? 0 : 1 }
  ' "$bundle/README.md"
}

worker_builds() {
  scope_value "$bundle/worksheet.md" '- Worker builds:'
}

worker_urls() {
  scope_value "$bundle/worksheet.md" '- Worker URLs:'
}

account_labels() {
  scope_value "$bundle/worksheet.md" '- Account labels:'
}

auth_mode() {
  scope_value "$bundle/worksheet.md" '- Auth mode:'
}

timeout_values() {
  scope_value "$bundle/worksheet.md" '- v2 timeout values:'
}

pending_limits() {
  scope_value "$bundle/worksheet.md" '- Pending request limits:'
}

readme_topology_builds=$(topology_field_values builds)
readme_topology_urls=$(topology_field_values urls)
readme_topology_accounts=$(topology_field_values accounts)
readme_unlabeled_worker_count=$(topology_unlabeled_worker_count)
readme_gateway_build=$(scope_value "$bundle/README.md" '- Gateway build:')
readme_topology_id=$(scope_value "$bundle/README.md" '- Topology id:')
readme_worker_builds=$(scope_value "$bundle/README.md" '- Worker builds:')
worksheet_gateway_build=$(scope_value "$bundle/worksheet.md" '- Gateway build:')
worksheet_topology_id=$(scope_value "$bundle/worksheet.md" '- Topology id:')
decision_gateway_build=$(scope_value "$bundle/decision.md" '- Gateway build:')
decision_topology_id=$(scope_value "$bundle/decision.md" '- Topology id:')
worksheet_builds=$(worker_builds)
decision_worker_builds=$(scope_value "$bundle/decision.md" '- Worker builds:')
worksheet_urls=$(worker_urls)
worksheet_accounts=$(account_labels)
worksheet_auth_mode=$(auth_mode)
worksheet_timeout_values=$(timeout_values)
worksheet_pending_limits=$(pending_limits)

if [ "$readme_gateway_build" != "$worksheet_gateway_build" ] || [ "$readme_gateway_build" != "$decision_gateway_build" ]; then
  echo "gateway build values do not match across README, worksheet, and decision files in $bundle" >&2
  exit 1
fi

if [ "$readme_topology_id" != "$worksheet_topology_id" ] || [ "$readme_topology_id" != "$decision_topology_id" ]; then
  echo "topology id values do not match across README, worksheet, and decision files in $bundle" >&2
  exit 1
fi

if [ "$readme_worker_builds" != "$readme_topology_builds" ]; then
  echo "worker build values do not match between README metadata and topology rows in $bundle" >&2
  exit 1
fi

if [ "$readme_topology_builds" != "$worksheet_builds" ]; then
  echo "worker build values do not match between README and worksheet in $bundle" >&2
  exit 1
fi

if [ "$readme_topology_builds" != "$decision_worker_builds" ]; then
  echo "worker build values do not match across README, worksheet, and decision files in $bundle" >&2
  exit 1
fi

if [ "$readme_topology_urls" != "$worksheet_urls" ]; then
  echo "worker URL values do not match between README and worksheet in $bundle" >&2
  exit 1
fi

if [ "$readme_topology_accounts" != "$worksheet_accounts" ]; then
  echo "account label values do not match between README and worksheet in $bundle" >&2
  exit 1
fi

if [ "$(scope_value "$bundle/README.md" '- Northbound auth:')" != "$worksheet_auth_mode" ]; then
  echo "auth mode values do not match between README and worksheet in $bundle" >&2
  exit 1
fi

readme_timeout_values=$(cat <<EOF
initialize=$(scope_value "$bundle/README.md" '- v2 initialize timeout:'), client_send=$(scope_value "$bundle/README.md" '- v2 client send timeout:'), reconnect_backoff=$(scope_value "$bundle/README.md" '- v2 reconnect retry backoff:')
EOF
)

if [ "$readme_timeout_values" != "$worksheet_timeout_values" ]; then
  echo "v2 timeout values do not match between README and worksheet in $bundle" >&2
  exit 1
fi

readme_pending_limits=$(cat <<EOF
server=$(scope_value "$bundle/README.md" '- v2 max pending server requests:'), client=$(scope_value "$bundle/README.md" '- v2 max pending client requests:')
EOF
)

if [ "$readme_pending_limits" != "$worksheet_pending_limits" ]; then
  echo "pending request limits do not match between README and worksheet in $bundle" >&2
  exit 1
fi

readme_scope=$(scope_value "$bundle/README.md" '- Tenant/project scope:')
worksheet_scope=$(scope_value "$bundle/worksheet.md" '- Tenant/project scope:')
decision_scope=$(scope_value "$bundle/decision.md" '- Tenant/project scopes:')

if [ "$readme_scope" != "$worksheet_scope" ] || [ "$readme_scope" != "$decision_scope" ]; then
  echo "tenant/project scope values do not match across README, worksheet, and decision files in $bundle" >&2
  exit 1
fi

worksheet_decision_value=$(awk '/^- Decision:/ { sub("^- Decision: ", "", $0); print; exit }' "$bundle/worksheet.md" | sed 's/^ *//; s/ *$//')
case "$worksheet_decision_value" in
  "Release-quality multi-worker" | "Scoped Stage B" | "Reject / no promotion")
    ;;
  *)
    echo "invalid worksheet decision value in $bundle/worksheet.md: $worksheet_decision_value" >&2
    exit 1
    ;;
esac

decision_value=$(awk '/^- Decision:/ { sub("^- Decision: ", "", $0); print; exit }' "$bundle/decision.md" | sed 's/^ *//; s/ *$//')
if [ "$worksheet_decision_value" != "$decision_value" ]; then
  echo "worksheet decision value does not match decision.md in $bundle" >&2
  exit 1
fi

worksheet_route_classes=$(scope_value "$bundle/worksheet.md" '- Method matrix route classes covered:')
decision_method_families_included=$(scope_value "$bundle/decision.md" '- Method families included:')
if [ "$worksheet_route_classes" != "$decision_method_families_included" ]; then
  echo "method family coverage values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi
if ! comma_list_contains "$worksheet_route_classes" "project-aware account routing"; then
  echo "project-aware account routing must be included in worksheet route-class coverage in $bundle" >&2
  exit 1
fi
if ! comma_list_contains "$decision_method_families_included" "project-aware account routing"; then
  echo "project-aware account routing must be included in decision method coverage in $bundle" >&2
  exit 1
fi

worksheet_route_selection_summary=$(scope_value "$bundle/worksheet.md" '- Route-selection identities agree across health, events, metrics, and audit:')
decision_route_selection_summary=$(scope_value "$bundle/decision.md" '- Route-selection evidence:')
if [ "$worksheet_route_selection_summary" != "$decision_route_selection_summary" ]; then
  echo "route-selection evidence values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_account_capacity_summary=$(scope_value "$bundle/worksheet.md" '- Account-capacity identities agree across health, events, metrics, and logs:')
decision_account_capacity_summary=$(scope_value "$bundle/decision.md" '- Account-capacity evidence:')
if [ "$worksheet_account_capacity_summary" != "$decision_account_capacity_summary" ]; then
  echo "account-capacity evidence values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_bounded_handoff_summary=$(scope_value "$bundle/worksheet.md" '- Bounded handoff success/failure outcomes match client-visible behavior:')
decision_bounded_restoration_summary=$(scope_value "$bundle/decision.md" '- Bounded restoration evidence:')
if [ "$worksheet_bounded_handoff_summary" != "$decision_bounded_restoration_summary" ]; then
  echo "bounded handoff evidence values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_live_active_context_summary=$(scope_value "$bundle/worksheet.md" '- Live active-context methods fail closed instead of moving accounts:')
decision_live_active_context_summary=$(scope_value "$bundle/decision.md" '- Live active-context no-handoff evidence:')
if [ "$worksheet_live_active_context_summary" != "$decision_live_active_context_summary" ]; then
  echo "live active-context evidence values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_backlog_cleanup_summary=$(scope_value "$bundle/worksheet.md" '- Backlog and cleanup windows are bounded and observable:')
decision_reconnect_degraded_summary=$(scope_value "$bundle/decision.md" '- Reconnect/degraded-route evidence:')
decision_slow_client_backlog_summary=$(scope_value "$bundle/decision.md" '- Slow-client/backlog evidence:')
decision_cleanup_delivery_failure_summary=$(scope_value "$bundle/decision.md" '- Cleanup/delivery-failure evidence:')
if [ "$worksheet_backlog_cleanup_summary" != "$decision_reconnect_degraded_summary" ] || \
  [ "$worksheet_backlog_cleanup_summary" != "$decision_slow_client_backlog_summary" ] || \
  [ "$worksheet_backlog_cleanup_summary" != "$decision_cleanup_delivery_failure_summary" ]; then
  echo "backlog/cleanup evidence values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_promotion_scope=$(scope_value "$bundle/worksheet.md" '- Promotion scope:')
decision_promotion_scope=$(scope_value "$bundle/decision.md" '- Promotion scope:')
if [ "$worksheet_promotion_scope" != "$decision_promotion_scope" ]; then
  echo "promotion scope values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi

worksheet_excluded_families=$(scope_value "$bundle/worksheet.md" '- Excluded method families or route classes:')
decision_excluded_families=$(scope_value "$bundle/decision.md" '- Method families excluded:')
if [ "$worksheet_excluded_families" != "$decision_excluded_families" ]; then
  echo "excluded method family values do not match between worksheet and decision files in $bundle" >&2
  exit 1
fi
worksheet_follow_up=$(scope_value "$bundle/worksheet.md" '- Follow-up required before wider rollout:')

if [ "$decision_value" = "Release-quality multi-worker" ]; then
  if [ "$readme_unlabeled_worker_count" -ne 0 ]; then
    echo "release-quality multi-worker decisions require every topology worker to have an account label in $bundle/README.md" >&2
    exit 1
  fi

  case ${decision_excluded_families,,} in
    none)
      ;;
    *)
      echo "release-quality multi-worker decisions cannot exclude method families in $bundle" >&2
      exit 1
      ;;
  esac

  case ${worksheet_follow_up,,} in
    none)
      ;;
    *)
      echo "release-quality multi-worker decisions cannot require follow-up before wider rollout in $bundle/worksheet.md" >&2
      exit 1
      ;;
  esac

  reject_release_quality_gap "$bundle/worksheet.md" '- Route-selection identities agree across health, events, metrics, and audit:' "$worksheet_route_selection_summary"
  reject_release_quality_gap "$bundle/worksheet.md" '- Account-capacity identities agree across health, events, metrics, and logs:' "$worksheet_account_capacity_summary"
  reject_release_quality_gap "$bundle/worksheet.md" '- Bounded handoff success/failure outcomes match client-visible behavior:' "$worksheet_bounded_handoff_summary"
  reject_release_quality_gap "$bundle/worksheet.md" '- Live active-context methods fail closed instead of moving accounts:' "$worksheet_live_active_context_summary"
  reject_release_quality_gap "$bundle/worksheet.md" '- Backlog and cleanup windows are bounded and observable:' "$worksheet_backlog_cleanup_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Route-selection evidence:' "$decision_route_selection_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Account-capacity evidence:' "$decision_account_capacity_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Bounded restoration evidence:' "$decision_bounded_restoration_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Live active-context no-handoff evidence:' "$decision_live_active_context_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Reconnect/degraded-route evidence:' "$decision_reconnect_degraded_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Slow-client/backlog evidence:' "$decision_slow_client_backlog_summary"
  reject_release_quality_gap "$bundle/decision.md" '- Cleanup/delivery-failure evidence:' "$decision_cleanup_delivery_failure_summary"
elif [ "$decision_value" = "Reject / no promotion" ]; then
  case ${worksheet_follow_up,,} in
    none)
      echo "reject / no promotion decisions must name required follow-up before wider rollout in $bundle/worksheet.md" >&2
      exit 1
      ;;
  esac
fi

decision_file=$(awk '/^- Decision file:/ { sub("^- Decision file: ", "", $0); print; exit }' "$bundle/README.md" | sed 's/^ *//; s/ *$//')
if [ "$decision_file" != "decision.md" ]; then
  echo "unexpected decision file reference in $bundle/README.md: $decision_file" >&2
  exit 1
fi

require_timestamp_field() {
  path=$1
  prefix=$2
  value=$(awk -v prefix="$prefix" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path")
  value=$(printf '%s' "$value" | sed 's/^ *//; s/ *$//')
  if [ -z "$value" ]; then
    echo "missing timestamp in $path: $prefix" >&2
    exit 1
  fi
  case "$value" in
    ????-??-??T??:??:??Z)
      ;;
    *)
      echo "invalid timestamp format in $path: $prefix$value" >&2
      exit 1
      ;;
  esac
}

require_timestamp_field "$bundle/README.md" '- Capture start:'
require_timestamp_field "$bundle/README.md" '- Capture end:'

timestamp_value() {
  path=$1
  prefix=$2
  awk -v prefix="$prefix" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path" | sed 's/^ *//; s/ *$//'
}

capture_time_epoch() {
  capture_time=$1
  case "$capture_time" in
    ????-??-??T??:??:??Z)
      ;;
    *)
      echo "invalid capture time format: $capture_time" >&2
      exit 1
      ;;
  esac

  if ! date -u -d "$capture_time" +%s >/dev/null 2>&1; then
    echo "invalid capture time value: $capture_time" >&2
    exit 1
  fi

  date -u -d "$capture_time" +%s
}

capture_start=$(timestamp_value "$bundle/README.md" '- Capture start:')
capture_end=$(timestamp_value "$bundle/README.md" '- Capture end:')
capture_start_epoch=$(capture_time_epoch "$capture_start")
capture_end_epoch=$(capture_time_epoch "$capture_end")
if [ "$capture_end_epoch" -lt "$capture_start_epoch" ]; then
  echo "capture end precedes capture start in $bundle/README.md" >&2
  exit 1
fi

require_topology_rows() {
  row_count=0
  topology_worker_ids=$(mktemp)
  topology_websocket_urls=$(mktemp)
  while IFS='|' read -r _ worker_id build_id websocket_url account_id auth_mode notes _; do
    case $worker_id in
      " Worker id " | " --- " | "" )
        continue
        ;;
    esac

    worker_id=$(printf '%s' "$worker_id" | sed 's/^ *//; s/ *$//')
    build_id=$(printf '%s' "$build_id" | sed 's/^ *//; s/ *$//')
    websocket_url=$(printf '%s' "$websocket_url" | sed 's/^ *//; s/ *$//')
    account_id=$(printf '%s' "$account_id" | sed 's/^ *//; s/ *$//')
    auth_mode=$(printf '%s' "$auth_mode" | sed 's/^ *//; s/ *$//')
    notes=$(printf '%s' "$notes" | sed 's/^ *//; s/ *$//')

    if [ -z "$worker_id" ] || [ -z "$build_id" ] || [ -z "$websocket_url" ] || [ -z "$auth_mode" ]; then
      echo "missing populated topology row in $bundle/README.md" >&2
      exit 1
    fi
    reject_placeholder_value "$bundle/README.md" 'topology worker id: ' "$worker_id"
    reject_placeholder_value "$bundle/README.md" 'topology build id: ' "$build_id"
    reject_placeholder_value "$bundle/README.md" 'topology websocket url: ' "$websocket_url"
    reject_placeholder_value "$bundle/README.md" 'topology account id: ' "$account_id"
    reject_placeholder_value "$bundle/README.md" 'topology auth mode: ' "$auth_mode"
    reject_placeholder_value "$bundle/README.md" 'topology notes: ' "$notes"

    case "$worker_id" in
      *[!A-Za-z0-9._-]*)
        echo "invalid worker id in $bundle/README.md: $worker_id" >&2
        exit 1
        ;;
    esac

    if grep -Fxq "$worker_id" "$topology_worker_ids"; then
      echo "duplicate worker id in $bundle/README.md: $worker_id" >&2
      exit 1
    fi
    if grep -Fxq "$websocket_url" "$topology_websocket_urls"; then
      echo "duplicate worker websocket url in $bundle/README.md: $websocket_url" >&2
      exit 1
    fi

    printf '%s\n' "$worker_id" >> "$topology_worker_ids"
    printf '%s\n' "$websocket_url" >> "$topology_websocket_urls"

    row_count=$((row_count + 1))
  done < <(awk '/^## Topology$/ { in_topology = 1; next } in_topology && /^## / { exit } in_topology && /^\|/ { print }' "$bundle/README.md")

  if [ "$row_count" -eq 0 ]; then
    echo "missing populated topology rows in $bundle/README.md" >&2
    exit 1
  fi

  rm -f "$topology_worker_ids" "$topology_websocket_urls"
}

require_runtime_config() {
  require_populated_field "$bundle/README.md" '- Northbound auth:'
  require_populated_field "$bundle/README.md" '- v2 initialize timeout:'
  require_populated_field "$bundle/README.md" '- v2 client send timeout:'
  require_populated_field "$bundle/README.md" '- v2 reconnect retry backoff:'
  require_populated_field "$bundle/README.md" '- v2 max pending server requests:'
  require_populated_field "$bundle/README.md" '- v2 max pending client requests:'
}

require_bundle_evidence() {
  path=$1
  pattern=$2
  label=$3

  if ! grep -R -Fq -- "$pattern" "$path"; then
    echo "missing required $label evidence in $path: $pattern" >&2
    exit 1
  fi
}

require_project_route_selection_evidence() {
  health=$1
  events=$2
  metrics=$3
  logs=$4

  require_bundle_evidence "$health" 'projectWorkerRoutes' 'project route selection health'
  require_bundle_evidence "$health" 'accountRoutingEligible' 'project route selection health'
  require_bundle_evidence "$events" 'gateway/projectWorkerRouteSelected' 'project route selection event'
  require_bundle_evidence "$metrics" 'gateway_project_worker_route_selections' 'project route selection metric'
  require_bundle_evidence "$logs" 'codex_gateway.audit' 'project route selection audit log'
  require_bundle_evidence "$logs" 'gateway project worker route selected' 'project route selection audit log'
}

require_topology_rows
require_runtime_config
require_bundle_evidence "$bundle/healthz" 'projectWorkerRoutes' 'project route health'
require_bundle_evidence "$bundle/events" 'gateway/projectWorkerRouteSelected' 'project route selection event'
require_bundle_evidence "$bundle/metrics" 'gateway_project_worker_route_selections' 'project route selection metric'
require_bundle_evidence "$bundle/logs" 'codex_gateway.audit' 'project route selection audit log'
require_bundle_evidence "$bundle/logs" 'gateway project worker route selected' 'project route selection audit log'

require_unique_scenario() {
  path=$1
  set_file=$2
  scenario=$3
  if grep -Fxq "$scenario" "$set_file"; then
    echo "duplicate scenario in $path: $scenario" >&2
    exit 1
  fi
  printf '%s\n' "$scenario" >> "$set_file"
}

require_section_body() {
  path=$1
  heading=$2
  body=$(awk -v heading="$heading" '
    $0 == heading { in_section = 1; next }
    in_section && /^## / { exit }
    in_section { print }
  ' "$path")
  body=$(printf '%s' "$body" | sed '/^$/d; s/^ *//; s/ *$//')
  if [ -z "$body" ]; then
    echo "missing populated section body in $path: $heading" >&2
    exit 1
  fi
}

capture_metadata_value() {
  path=$1
  prefix=$2
  awk -v prefix="$prefix" 'index($0, prefix) == 1 { sub("^" prefix, "", $0); print; exit }' "$path" | sed 's/^ *//; s/ *$//'
}

require_matching_capture_metadata() {
  scenario=$1
  transcript=$2
  health=$3
  events=$4
  metrics=$5
  logs=$6

  expected_gateway_build=$(capture_metadata_value "$transcript" 'gateway build:')
  expected_worker_build=$(capture_metadata_value "$transcript" 'worker build:')
  expected_tenant_id=$(capture_metadata_value "$transcript" 'tenant id:')
  expected_project_id=$(capture_metadata_value "$transcript" 'project id:')
  expected_worker_id=$(capture_metadata_value "$transcript" 'worker id:')
  expected_account_id=$(capture_metadata_value "$transcript" 'account id:')
  expected_normalized_account_id=$(normalize_account_id "$expected_account_id")
  expected_capture_time=$(capture_metadata_value "$transcript" 'capture time:')
  expected_capture_time_epoch=$(capture_time_epoch "$expected_capture_time")

  if [ "$expected_gateway_build" != "$readme_gateway_build" ]; then
    echo "capture gateway build in $transcript does not match bundle gateway build" >&2
    exit 1
  fi
  if ! comma_list_contains "$readme_topology_builds" "$expected_worker_build"; then
    echo "capture worker build in $transcript is not listed in bundle topology" >&2
    exit 1
  fi
  case "$readme_scope" in
    "tenant=$expected_tenant_id, projects="*)
      ;;
    *)
      echo "capture tenant id in $transcript does not match bundle tenant scope" >&2
      exit 1
      ;;
  esac
  bundle_projects=${readme_scope#tenant=$expected_tenant_id, projects=}
  if ! comma_list_contains "$bundle_projects" "$expected_project_id"; then
    echo "capture project id in $transcript is not listed in bundle project scope" >&2
    exit 1
  fi
  if [ -n "$expected_normalized_account_id" ] && [ -n "$readme_topology_accounts" ] && \
    ! comma_list_contains "$readme_topology_accounts" "$expected_normalized_account_id"; then
    echo "capture account id in $transcript is not listed in bundle topology" >&2
    exit 1
  fi
  if ! topology_has_worker_account_pair "$expected_worker_build" "$expected_account_id"; then
    echo "capture worker build and account id in $transcript do not match one bundle topology row" >&2
    exit 1
  fi

  if [ "$expected_capture_time_epoch" -lt "$capture_start_epoch" ] || [ "$expected_capture_time_epoch" -gt "$capture_end_epoch" ]; then
    echo "capture time in $transcript falls outside the README capture window" >&2
    exit 1
  fi

  if [ "$scenario" = "Project route selection" ]; then
    require_project_route_selection_evidence "$health" "$events" "$metrics" "$logs"
  fi

  for path in "$health" "$events" "$metrics" "$logs"; do
    gateway_build=$(capture_metadata_value "$path" 'gateway build:')
    worker_build=$(capture_metadata_value "$path" 'worker build:')
    tenant_id=$(capture_metadata_value "$path" 'tenant id:')
    project_id=$(capture_metadata_value "$path" 'project id:')
    worker_id=$(capture_metadata_value "$path" 'worker id:')
    account_id=$(capture_metadata_value "$path" 'account id:')
    normalized_account_id=$(normalize_account_id "$account_id")
    capture_time=$(capture_metadata_value "$path" 'capture time:')
    capture_time_epoch=$(capture_time_epoch "$capture_time")

    if [ "$gateway_build" != "$expected_gateway_build" ] || \
      [ "$worker_build" != "$expected_worker_build" ] || \
      [ "$tenant_id" != "$expected_tenant_id" ] || \
      [ "$project_id" != "$expected_project_id" ] || \
      [ "$worker_id" != "$expected_worker_id" ] || \
      [ "$normalized_account_id" != "$expected_normalized_account_id" ] || \
      [ "$capture_time" != "$expected_capture_time" ]; then
      echo "mismatched capture metadata in $path" >&2
      exit 1
    fi

    if [ "$capture_time_epoch" -lt "$capture_start_epoch" ] || [ "$capture_time_epoch" -gt "$capture_end_epoch" ]; then
      echo "capture time in $path falls outside the README capture window" >&2
      exit 1
    fi
  done

}

evidence_index_row_number=0
worksheet_row_number=0

require_section_body "$bundle/decision.md" '## Invalidation Rules'

require_expected_scenarios() {
  path=$1
  scenario_set=$2
  while IFS= read -r expected_scenario; do
    if ! grep -Fxq "$expected_scenario" "$scenario_set"; then
      echo "missing required promotion scenario in $path: $expected_scenario" >&2
      exit 1
    fi
  done <<'EOF'
Baseline before traffic
Steady-state bootstrap and thread/turn
Project route selection
Reconnect and recovery
Degraded-route fail-closed
Account-capacity transition
Bounded restoration
Live active-context no-handoff
Slow-client or backlog window
Cleanup or delivery failure
EOF
}

require_blocking_mismatch_row() {
  has_row=false
  while IFS= read -r row; do
    row=$(printf '%s' "$row" | sed 's/^ *//; s/ *$//')
    has_row=true
    if [ "$row" = '| --- | --- | --- | --- | --- |' ] || [ "$row" = '| | | | | |' ]; then
      echo "missing populated blocking-mismatches row in $bundle/decision.md" >&2
      exit 1
    fi

    row_cells=$(printf '%s' "$row" | awk -F'|' '{
      for (i = 2; i <= NF - 1; i++) {
        cell = $i
        gsub(/^ +| +$/, "", cell)
        print cell
      }
    }')

    while IFS= read -r cell; do
      if [ -z "$cell" ]; then
        echo "missing populated blocking-mismatches cell in $bundle/decision.md: $row" >&2
        exit 1
      fi
      reject_placeholder_value "$bundle/decision.md" 'blocking-mismatch cell: ' "$cell"
    done <<EOF
$row_cells
EOF
  done < <(awk '
    $0 == "## Blocking Mismatches" { in_section = 1; next }
    in_section && /^## / { exit }
    in_section && /^\| Scenario \|/ { next }
    in_section && /^\| --- \|/ { next }
    in_section && /^\|/ { print }
  ' "$bundle/decision.md")

  if [ "$has_row" != true ]; then
    echo "missing populated blocking-mismatches row in $bundle/decision.md" >&2
    exit 1
  fi
}

require_blocking_mismatch_row

require_capture_metadata() {
  path=$1
  for label in 'gateway build:' 'worker build:' 'tenant id:' 'project id:' 'worker id:' 'account id:' 'capture time:'; do
    if ! grep -Fqi "$label" "$path"; then
      echo "missing required capture metadata in $path: $label" >&2
      exit 1
    fi
    value=$(capture_metadata_value "$path" "$label")
    if [ -z "$value" ]; then
      echo "missing populated capture metadata in $path: $label" >&2
      exit 1
    fi
    reject_placeholder_value "$path" "$label" "$value"
  done
}

if grep -Fq '| | | | | | |' "$bundle/README.md"; then
  echo "README.md still contains placeholder topology or evidence-index rows" >&2
  exit 1
fi

if grep -Fq '| | | | | | |' "$bundle/worksheet.md"; then
  echo "worksheet.md still contains placeholder capture rows" >&2
  exit 1
fi

if grep -Fq '| | | | | |' "$bundle/decision.md"; then
  echo "decision.md still contains placeholder blocking-mismatch rows" >&2
  exit 1
fi

check_relative_path() {
  path=$1
  if [ -z "$path" ]; then
    return 0
  fi
  case "$path" in
    /*|../*|*/../*|*/..|..)
      echo "invalid referenced artifact path in $bundle: $path" >&2
      exit 1
      ;;
  esac
  if [ ! -e "$bundle/$path" ]; then
    echo "missing referenced artifact in $bundle: $path" >&2
    exit 1
  fi

  resolved_bundle_root=$(realpath "$bundle")
  resolved_path=$(realpath "$bundle/$path")
  case "$resolved_path" in
    "$resolved_bundle_root" | "$resolved_bundle_root"/*)
      ;;
    *)
      echo "referenced artifact escapes bundle root in $bundle: $path" >&2
      exit 1
      ;;
  esac
}

while IFS='|' read -r _ scenario transcript health events metrics logs worksheet _; do
  case $scenario in
    " Scenario " | " --- " | "" )
      continue
      ;;
  esac

  scenario=$(printf '%s' "$scenario" | sed 's/^ *//; s/ *$//')
  transcript=$(printf '%s' "$transcript" | sed 's/^ *//; s/ *$//')
  health=$(printf '%s' "$health" | sed 's/^ *//; s/ *$//')
  events=$(printf '%s' "$events" | sed 's/^ *//; s/ *$//')
  metrics=$(printf '%s' "$metrics" | sed 's/^ *//; s/ *$//')
  logs=$(printf '%s' "$logs" | sed 's/^ *//; s/ *$//')
  worksheet=$(printf '%s' "$worksheet" | sed 's/^ *//; s/ *$//')

  if [ -n "$scenario" ]; then
    evidence_index_row_number=$((evidence_index_row_number + 1))
    if [ -z "$transcript" ] || [ -z "$health" ] || [ -z "$events" ] || [ -z "$metrics" ] || [ -z "$logs" ] || [ -z "$worksheet" ]; then
      echo "missing populated evidence-index row for scenario: $scenario" >&2
      exit 1
    fi
    expected_worksheet_row="worksheet row $evidence_index_row_number"
    if [ "$worksheet" != "$expected_worksheet_row" ]; then
      echo "invalid worksheet row reference in $bundle/README.md for scenario: $scenario: expected $expected_worksheet_row, got $worksheet" >&2
      exit 1
    fi
    reject_placeholder_value "$bundle/README.md" 'evidence-index scenario: ' "$scenario"
    printf '%s\n' "$scenario" >> "$evidence_index_scenarios"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$scenario" "$transcript" "$health" "$events" "$metrics" "$logs" >> "$evidence_index_refs"
    require_unique_scenario "$bundle/README.md" "$evidence_index_scenario_set" "$scenario"
    check_relative_path "$transcript"
    check_relative_path "$health"
    check_relative_path "$events"
    check_relative_path "$metrics"
    check_relative_path "$logs"
    require_capture_metadata "$bundle/$transcript"
    require_capture_metadata "$bundle/$health"
    require_capture_metadata "$bundle/$events"
    require_capture_metadata "$bundle/$metrics"
    require_capture_metadata "$bundle/$logs"
    require_matching_capture_metadata "$scenario" "$bundle/$transcript" "$bundle/$health" "$bundle/$events" "$bundle/$metrics" "$bundle/$logs"
  fi
done < <(awk '/^## Evidence Index$/ { in_index = 1; next } in_index && /^## / { exit } in_index && /^\|/ { print }' "$bundle/README.md")

while IFS='|' read -r _ scenario transcript health events metrics logs result _; do
  case $scenario in
    " Scenario " | " --- " | "" )
      continue
      ;;
  esac

  scenario=$(printf '%s' "$scenario" | sed 's/^ *//; s/ *$//')
  transcript=$(printf '%s' "$transcript" | sed 's/^ *//; s/ *$//')
  health=$(printf '%s' "$health" | sed 's/^ *//; s/ *$//')
  events=$(printf '%s' "$events" | sed 's/^ *//; s/ *$//')
  metrics=$(printf '%s' "$metrics" | sed 's/^ *//; s/ *$//')
  logs=$(printf '%s' "$logs" | sed 's/^ *//; s/ *$//')
  result=$(printf '%s' "$result" | sed 's/^ *//; s/ *$//')

  if [ -n "$scenario" ]; then
    worksheet_row_number=$((worksheet_row_number + 1))
    if [ -z "$transcript" ] || [ -z "$health" ] || [ -z "$events" ] || [ -z "$metrics" ] || [ -z "$logs" ] || [ -z "$result" ]; then
      echo "missing populated worksheet row for scenario: $scenario" >&2
      exit 1
    fi
    reject_placeholder_value "$bundle/worksheet.md" 'worksheet scenario: ' "$scenario"
    reject_placeholder_value "$bundle/worksheet.md" "worksheet result for $scenario: " "$result"
    if [ "$decision_value" = "Release-quality multi-worker" ]; then
      reject_release_quality_gap "$bundle/worksheet.md" "worksheet result for $scenario: " "$result"
    fi
    printf '%s\n' "$scenario" >> "$worksheet_scenarios"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$scenario" "$transcript" "$health" "$events" "$metrics" "$logs" >> "$worksheet_refs"
    require_unique_scenario "$bundle/worksheet.md" "$worksheet_scenario_set" "$scenario"
    check_relative_path "$transcript"
    check_relative_path "$health"
    check_relative_path "$events"
    check_relative_path "$metrics"
    check_relative_path "$logs"
    require_capture_metadata "$bundle/$transcript"
    require_capture_metadata "$bundle/$health"
    require_capture_metadata "$bundle/$events"
    require_capture_metadata "$bundle/$metrics"
    require_capture_metadata "$bundle/$logs"
    require_matching_capture_metadata "$scenario" "$bundle/$transcript" "$bundle/$health" "$bundle/$events" "$bundle/$metrics" "$bundle/$logs"
  fi
done < <(awk '/^## Captures$/ { in_captures = 1; next } in_captures && /^## / { exit } in_captures && /^\|/ { print }' "$bundle/worksheet.md")

if [ "$evidence_index_row_number" -ne "$worksheet_row_number" ]; then
  echo "evidence index row count does not match worksheet capture row count" >&2
  exit 1
fi

if ! cmp -s "$evidence_index_scenarios" "$worksheet_scenarios"; then
  echo "evidence index scenarios do not match worksheet capture scenarios" >&2
  exit 1
fi

if ! cmp -s "$evidence_index_refs" "$worksheet_refs"; then
  echo "evidence index artifact references do not match worksheet capture references" >&2
  exit 1
fi

require_expected_scenarios "$bundle/README.md" "$evidence_index_scenario_set"
require_expected_scenarios "$bundle/worksheet.md" "$worksheet_scenario_set"

printf '%s\n' "$bundle"
