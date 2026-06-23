#!/usr/bin/env bash
# Master runner — every scenario suite in sequence against the configured target.
# Each suite resets to its own clean baseline (wms dev reset), so they are
# independent. `security.sh` runs LAST because its brute-force probe throttles
# our single real IP for a few minutes (per-IP rate limit on the deployed worker).
#
#   cli/tests/scenarios/all.sh
# Target/creds via the same env vars as lib.sh (default: deployed test worker).
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SUITES=(run.sh integrity.sh warehouse.sh reporting.sh security.sh)
FAILED=()
for s in "${SUITES[@]}"; do
  printf '\n\033[1;44m  ==== RUNNING %s ====  \033[0m\n' "$s"
  if bash "$DIR/$s"; then :; else FAILED+=("$s"); fi
done

printf '\n\033[1m======== ALL SUITES ========\033[0m\n'
if [ "${#FAILED[@]}" -eq 0 ]; then
  echo "✅ ALL SCENARIO SUITES GREEN"
else
  printf '❌ FAILED: %s\n' "${FAILED[*]}"; exit 1
fi
