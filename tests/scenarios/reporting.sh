#!/usr/bin/env bash
# Reporting / billing + scale / concurrency: inbound/outbound/activity/as_of
# reports, billing-data accrual inputs, a large bulk import (timed), concurrent
# order creation (no oversell), and login latency.
#
# Note: the daily storage-snapshot cron cannot be triggered over HTTP on a
# deployed worker, so billing-data's storageUnitDays is exercised via its data
# path only (it is 0 until the cron runs); this is documented, not a failure.
set -uo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
SUITE="reporting"

build_cli
reset_baseline

T="RPT"
oh_sku() { dq_num "SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory iv JOIN items it ON iv.item_id=it.id WHERE it.sku='$1'"; }
res_sku() { dq_num "SELECT COALESCE(SUM(qty_reserved),0) AS c FROM inventory iv JOIN items it ON iv.item_id=it.id WHERE it.sku='$1'"; }
# Report date params are YYYY-MM-DD (as_of/since/until); billing period is YYYY-MM.
NOW="$(date -u +%Y-%m-%d)"; SINCE="$(date -u +%Y-%m-%d)"; PERIOD="$(date -u +%Y-%m)"

say "Seed (inbound + one shipped order for movement history)"
SKU="RPT-SKU-$RUN_ID"; LOC="RPT-LOC-$RUN_ID"
run_check "tenant" 0 "$T"   -- "$BIN" tenant create --code "$T" --name "Reporting Co"
run_check "item"   0 "$SKU" -- "$BIN" --tenant "$T" item create --sku "$SKU" --name "Reported"
run_check "loc"    0 "$LOC" -- "$BIN" location create --code "$LOC" --type storage
run_check "inbound 100" 0 "" -- "$BIN" --tenant "$T" inbound receive --line "$SKU:100" --putaway-to "$LOC"
OID="$("$BIN" --tenant "$T" --output json order create --ref "RPT-O-$RUN_ID" --ship-to "Buyer" --line "$SKU:25" 2>/dev/null | json_field id)"
run_check "pick" 0 "" -- "$BIN" --tenant "$T" order pick "$OID" --line "$SKU@$LOC:25"
run_check "pack" 0 "" -- "$BIN" --tenant "$T" order pack "$OID"
run_check "ship" 0 "" -- "$BIN" --tenant "$T" order ship "$OID"

say "Reports"
run_check "report inventory (current)"  0 "$SKU" -- "$BIN" --tenant "$T" --output json report inventory
run_check "report inventory --as-of"    0 "$SKU" -- "$BIN" --tenant "$T" --output json report inventory --as-of "$NOW"
run_check "report inbound (window)"     0 "$SKU" -- "$BIN" --tenant "$T" --output json report inbound --since "$SINCE"
run_check "report outbound (window)"    0 "$SKU" -- "$BIN" --tenant "$T" --output json report outbound --since "$SINCE"
run_check "report activity"             0 ""     -- "$BIN" --tenant "$T" --output json report activity
run_check "billing-data (admin)"        0 ""     -- "$BIN" --tenant "$T" --output json report billing-data --period "$PERIOD"

say "Scale: large bulk import (CLI auto-chunks into <=50-order batches, timed)"
# The backend caps a single order import at 50 (Workers subrequest budget) and
# chunks its dedup inArray (D1 bound-param limit); the CLI splits large files into
# <=50-order batches automatically (dry-run-all -> apply-all), so big imports work.
SCALE_ORDERS="${SCALE_ORDERS:-120}"
FIX="$WORKDIR/fix"
TENANTS="$T" N_LOC=15 N_ITEMS=25 N_STOCK=20 N_ORDERS="$SCALE_ORDERS" node "$SCEN_DIR/gen-fixtures.mjs" "$FIX" "$RUN_ID"
run_check "import 25 items"  0 "created" -- "$BIN" --tenant "$T" item import "$FIX/items-$T.csv"
while IFS=$'\t' read -r tn sku loc qty; do [ -z "$tn" ] && continue; "$BIN" --tenant "$T" inbound receive --line "$sku:$qty" --putaway-to "$loc" </dev/null >/dev/null 2>&1; done < "$FIX/stock.tsv"
t0=$(date +%s)
run_check "import $SCALE_ORDERS orders" 0 "created" -- "$BIN" --tenant "$T" order import "$FIX/orders-$T.csv"
t1=$(date +%s)
echo "   ($SCALE_ORDERS-order import took $((t1 - t0))s)"
assert_eq "$SCALE_ORDERS orders imported" "$SCALE_ORDERS" "$(dq_num "SELECT count(*) AS c FROM orders o JOIN tenants t ON o.tenant_id=t.id WHERE t.code='$T' AND o.ref LIKE '$T-ORD-%'")"

say "Concurrency: 20 parallel orders for one SKU (demand 100 > stock 50, no oversell)"
CSKU="CC-SKU-$RUN_ID"; CLOC="CC-LOC-$RUN_ID"
run_check "concurrency item"  0 "$CSKU" -- "$BIN" --tenant "$T" item create --sku "$CSKU" --name "Hot Item"
run_check "concurrency loc"   0 "$CLOC" -- "$BIN" location create --code "$CLOC" --type storage
run_check "concurrency stock 50" 0 ""   -- "$BIN" --tenant "$T" inbound receive --line "$CSKU:50" --putaway-to "$CLOC"
for i in $(seq 1 20); do "$BIN" --tenant "$T" order create --ref "CC-$i-$RUN_ID" --line "$CSKU:5" </dev/null >/dev/null 2>&1 & done
wait
CRES="$(res_sku "$CSKU")"; COH="$(oh_sku "$CSKU")"
echo "   (after 20×5 concurrent demand: reserved=$CRES on_hand=$COH)"
if [ "$CRES" -le "$COH" ] && [ "$COH" = "50" ]; then pass "no oversell under concurrency (reserved $CRES ≤ on_hand 50)"; else fail "oversell/inconsistency: reserved=$CRES on_hand=$COH"; fi

say "Login latency (operational metric)"
LAT="$(curl -s -o /dev/null -w '%{time_total}' -X POST -H 'content-type: application/json' --data "$(printf '{"email":"%s","password":"%s"}' "$BOOT_EMAIL" "$BOOT_PW")" "$ORIGIN/api/v1/auth/login")"
pass "auth login wall-time: ${LAT}s (Argon2id)"

summary
