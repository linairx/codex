#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/check-gateway-promotion-bundle.sh BUNDLE_DIR
EOF
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ] || [ "$#" -ne 1 ]; then
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
  if [ ! -e "$bundle/$path" ]; then
    echo "missing referenced artifact in $bundle: $path" >&2
    exit 1
  fi
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
    check_relative_path "$transcript"
    check_relative_path "$health"
    check_relative_path "$events"
    check_relative_path "$metrics"
    check_relative_path "$logs"
    if [ -z "$worksheet" ]; then
      echo "missing worksheet row reference for scenario: $scenario" >&2
      exit 1
    fi
  fi
done < <(awk '/^## Evidence Index$/ { in_index = 1; next } in_index && /^## / { exit } in_index && /^\|/ { print }' "$bundle/README.md")

printf '%s\n' "$bundle"
