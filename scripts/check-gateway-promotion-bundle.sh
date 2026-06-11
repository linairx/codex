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

require_file "$bundle/README.md"
require_file "$bundle/worksheet.md"
require_file "$bundle/decision.md"
require_dir "$bundle/transcripts"
require_dir "$bundle/healthz"
require_dir "$bundle/events"
require_dir "$bundle/metrics"
require_dir "$bundle/logs"

grep -Fq '# Gateway Promotion Evidence Bundle' "$bundle/README.md"
grep -Fq '# Gateway Multi-Worker Promotion Evidence' "$bundle/worksheet.md"
grep -Fq '# Gateway Multi-Worker Promotion Decision' "$bundle/decision.md"

printf '%s\n' "$bundle"
