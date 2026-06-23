#!/usr/bin/env bash
#
# CLI-driven E2E SCENARIO suite against a REAL DEPLOYED backend.
#
# Unlike tests/integration/run.sh (which boots a local wrangler dev), this drives
# the `wms` CLI against the live, isolated TEST environment
# (erp-wms-backend-test.bizcard.workers.dev), resets it to a clean baseline via
# `wms dev reset`, seeds a medium 3PL volume from CSV, and asserts realistic
# warehouse scenarios. It NEVER targets the real prod worker.
#
#   cli/tests/scenarios/run.sh
#
# Config (env overrides):
#   WMS_SCENARIO_ENDPOINT   backend origin (default: the deployed test worker)
#   WMS_BOOTSTRAP_EMAIL     bootstrap developer email   (default: from backend/.test-env-secrets)
#   WMS_BOOTSTRAP_PASSWORD  bootstrap developer password (default: from backend/.test-env-secrets)
#   N_LOC / N_ITEMS / N_STOCK / N_ORDERS / TENANTS  → see gen-fixtures.mjs
#
# Requires: node, cargo. Re-runnable (it resets first).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
CLI_DIR="$ROOT/cli"
BACKEND_DIR="$ROOT/backend"

ORIGIN="${WMS_SCENARIO_ENDPOINT:-https://erp-wms-backend-test.bizcard.workers.dev}"
SECRETS="$BACKEND_DIR/.test-env-secrets"
read_secret() { [ -f "$SECRETS" ] && grep -E "^$1=" "$SECRETS" | head -1 | cut -d= -f2- || true; }
BOOT_EMAIL="${WMS_BOOTSTRAP_EMAIL:-$(read_secret BOOTSTRAP_EMAIL)}"
BOOT_PW="${WMS_BOOTSTRAP_PASSWORD:-$(read_secret BOOTSTRAP_PASSWORD)}"
: "${BOOT_EMAIL:?need WMS_BOOTSTRAP_EMAIL or backend/.test-env-secrets}"
: "${BOOT_PW:?need WMS_BOOTSTRAP_PASSWORD or backend/.test-env-secrets}"

RUN_ID="$(date +%s)"
WORKDIR="$(mktemp -d)"
FIX="$WORKDIR/fixtures"
export XDG_CONFIG_HOME="$WORKDIR/config"   # isolate the CLI profile/credentials
trap 'rm -rf "$WORKDIR"' EXIT

PASS=0; FAIL=0
say()  { printf '\n\033[1m== %s\033[0m\n' "$*"; }
pass() { printf '\033[32mPASS\033[0m %s\n' "$*"; PASS=$((PASS+1)); }
fail() { printf '\033[31mFAIL\033[0m %s\n' "$*"; FAIL=$((FAIL+1)); }

# run_check <desc> <expected_exit> <needle|""> -- <cmd...>
run_check() {
  local desc="$1" exp="$2" needle="$3"; shift 3; [ "$1" = "--" ] && shift
  local out rc
  out="$("$@" </dev/null 2>&1)"; rc=$?
  if [ "$rc" -ne "$exp" ]; then fail "$desc (exit $rc, expected $exp)"; sed 's/^/      /' <<<"$out"; return; fi
  if [ -n "$needle" ] && ! grep -qF -- "$needle" <<<"$out"; then fail "$desc (missing \"$needle\")"; sed 's/^/      /' <<<"$out"; return; fi
  pass "$desc"
}
assert_eq() { # <desc> <expected> <actual>
  if [ "$2" = "$3" ]; then pass "$1 (=$3)"; else fail "$1 (expected $2, got $3)"; fi
}

BIN="$CLI_DIR/target/debug/wms"
dq_num() { # <sql> → first value of the first row (CLI prints the rows array directly)
  "$BIN" --output json dev query "$1" 2>/dev/null | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{try{let j=JSON.parse(s);if(j&&j.rows)j=j.rows;const r=Array.isArray(j)?(j[0]||{}):(j||{});process.stdout.write(String(Object.values(r)[0]))}catch{process.stdout.write('ERR')}})"
}
json_first_id() { node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{try{const j=JSON.parse(s);const a=j.data||j;const o=Array.isArray(a)?a[0]:a;process.stdout.write((o&&o.id)||'')}catch{process.stdout.write('')}})"; }

# ------------------------------------------------------------------ build
say "Building the CLI"
( cd "$CLI_DIR" && cargo build --quiet ) || { echo "cargo build failed"; exit 1; }

say "Target: $ORIGIN"
"$BIN" config set endpoint "$ORIGIN" >/dev/null

# ------------------------------------------------------------------ reset baseline
say "Reset to a clean baseline (wms dev reset)"
run_check "login (bootstrap dev)"     0 "developer" -- "$BIN" auth login --email "$BOOT_EMAIL" --password "$BOOT_PW"
run_check "dev reset (wipe + reseed)" 0 ""          -- "$BIN" dev reset --seed-email "$BOOT_EMAIL" --seed-password "$BOOT_PW" --confirm RESET
run_check "re-login after reset"      0 "developer" -- "$BIN" auth login --email "$BOOT_EMAIL" --password "$BOOT_PW"
assert_eq "baseline: 0 tenants" 0 "$(dq_num 'SELECT count(*) AS c FROM tenants')"
assert_eq "baseline: 1 user"    1 "$(dq_num 'SELECT count(*) AS c FROM users')"

# ------------------------------------------------------------------ fixtures
say "Generating medium-volume fixtures"
node "$SCRIPT_DIR/gen-fixtures.mjs" "$FIX" "$RUN_ID"
mapfile -t TENANTS < "$FIX/tenants.txt"

# ------------------------------------------------------------------ onboarding + catalog
say "Onboarding tenants + catalog (CSV import)"
for T in "${TENANTS[@]}"; do
  run_check "tenant create $T" 0 "$T" -- "$BIN" tenant create --code "$T" --name "$T Logistics"
done
run_check "import locations" 0 "created" -- "$BIN" location import "$FIX/locations.csv"
for T in "${TENANTS[@]}"; do
  run_check "import items ($T)" 0 "created" -- "$BIN" --tenant "$T" item import "$FIX/items-$T.csv"
done
assert_eq "locations seeded"  "${N_LOC:-50}"                 "$(dq_num 'SELECT count(*) AS c FROM locations')"
assert_eq "items seeded"      "$(( ${N_ITEMS:-50} * ${#TENANTS[@]} ))" "$(dq_num 'SELECT count(*) AS c FROM items')"

# ------------------------------------------------------------------ inbound → stock ("물류 확보")
say "Inbound receiving + putaway (building on-hand stock)"
stocked=0
while IFS=$'\t' read -r T sku loc qty; do
  [ -z "$T" ] && continue
  if "$BIN" --tenant "$T" inbound receive --line "$sku:$qty" --putaway-to "$loc" </dev/null >/dev/null 2>&1; then
    stocked=$((stocked+1))
  fi
done < "$FIX/stock.tsv"
total_stock_lines=$(wc -l < "$FIX/stock.tsv")
assert_eq "all stock putaways succeeded" "$total_stock_lines" "$stocked"
# Ledger conservation invariant: every on-hand change wrote a ledger row.
assert_eq "ledger Σdelta == Σon_hand" \
  "$(dq_num 'SELECT COALESCE(SUM(delta),0) AS c FROM inventory_ledger')" \
  "$(dq_num 'SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory')"

# ------------------------------------------------------------------ bulk orders (auto-allocate)
say "Bulk order import (auto-allocation)"
for T in "${TENANTS[@]}"; do
  run_check "import orders ($T)" 0 "created" -- "$BIN" --tenant "$T" order import "$FIX/orders-$T.csv"
done
assert_eq "orders seeded" "$(( ${N_ORDERS:-50} * ${#TENANTS[@]} ))" "$(dq_num 'SELECT count(*) AS c FROM orders')"

# ------------------------------------------------------------------ order lifecycle (dedicated, deterministic)
say "Order lifecycle (allocate → pick → pack → ship)"
T0="${TENANTS[0]}"
LSKU="LIFE-SKU-$RUN_ID"; LLOC="LIFE-LOC-$RUN_ID"
run_check "create lifecycle item"  0 "$LSKU" -- "$BIN" --tenant "$T0" item create --sku "$LSKU" --name "Lifecycle Widget"
run_check "create lifecycle loc"   0 "$LLOC" -- "$BIN" location create --code "$LLOC" --type storage
run_check "stock 100"              0 ""       -- "$BIN" --tenant "$T0" inbound receive --line "$LSKU:100" --putaway-to "$LLOC"
run_check "inventory shows 100"    0 "100"    -- "$BIN" --tenant "$T0" inventory list --sku "$LSKU"
OID="$("$BIN" --tenant "$T0" --output json order create --ref "LIFECYCLE-$RUN_ID" --ship-to "Acme Receiving" --line "$LSKU:10" 2>/dev/null | json_first_id)"
run_check "allocate" 0 "" -- "$BIN" --tenant "$T0" order allocate "$OID"
run_check "pick"     0 "" -- "$BIN" --tenant "$T0" order pick "$OID" --line "$LSKU:10"
run_check "pack"     0 "" -- "$BIN" --tenant "$T0" order pack "$OID"
run_check "ship"     0 "" -- "$BIN" --tenant "$T0" order ship "$OID" --tracking "TRK-$RUN_ID"
run_check "inventory dropped to 90" 0 "90" -- "$BIN" --tenant "$T0" inventory list --sku "$LSKU"
assert_eq "conservation still holds after ship" \
  "$(dq_num 'SELECT COALESCE(SUM(delta),0) AS c FROM inventory_ledger')" \
  "$(dq_num 'SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory')"

# ------------------------------------------------------------------ backorder
say "Backorder (demand exceeds stock)"
BO="$("$BIN" --tenant "$T0" --output json order create --ref "BACKORDER-$RUN_ID" --ship-to "Big Buyer" --line "$LSKU:99999" 2>/dev/null | json_first_id)"
run_check "short order → backorder" 0 "backorder" -- "$BIN" --tenant "$T0" order get "$BO"

# ------------------------------------------------------------------ reports & alerts
say "Reports & alerts reflect the seeded volume"
run_check "report inventory" 0 "$T0-SKU-0001" -- "$BIN" --tenant "$T0" --output json report inventory
run_check "low-stock alert"  0 "$T0-SKU-00$(( ${N_STOCK:-20} + 1 ))" -- "$BIN" --tenant "$T0" --output json alert list

# ------------------------------------------------------------------ multi-tenant isolation
say "Multi-tenant isolation"
ISKU="$T0-SKU-0001"
run_check "item visible in its tenant"  0 "$ISKU" -- "$BIN" --tenant "$T0" item get "$ISKU"
if [ "${#TENANTS[@]}" -ge 2 ]; then
  run_check "item NOT visible cross-tenant (exit 4)" 4 "" -- "$BIN" --tenant "${TENANTS[1]}" item get "$ISKU"
fi

# ------------------------------------------------------------------ authz boundary
say "Authz boundary"
run_check "unauth write rejected (exit 3)" 3 "" -- "$BIN" --token bogus --endpoint "$ORIGIN" tenant create --code "X$RUN_ID" --name x

# ------------------------------------------------------------------ summary
say "Summary"
printf 'PASS=%d  FAIL=%d  (target: %s)\n' "$PASS" "$FAIL" "$ORIGIN"
[ "$FAIL" -eq 0 ] || exit 1
echo "scenarios: all green"
