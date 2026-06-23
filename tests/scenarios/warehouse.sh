#!/usr/bin/env bash
# Deeper warehouse ops + lifecycle: move / count / adjust (+ audit), ASN
# receive/putaway/over-putaway/cancel guards, backorder → restock → re-allocate
# → ship, and tenant offboarding (cascade delete via the DELETE API — the CLI
# `dev tenant delete` is intentionally interactive-only).
set -uo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
SUITE="warehouse"

build_cli
reset_baseline

T="WHSE"
oh_loc() { dq_num "SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory iv JOIN items it ON iv.item_id=it.id JOIN locations l ON iv.location_id=l.id WHERE it.sku='$1' AND l.code='$2'"; }
oh_sku() { dq_num "SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory iv JOIN items it ON iv.item_id=it.id WHERE it.sku='$1'"; }
 stat_of() { "$BIN" --tenant "$T" --output json order get "$1" 2>/dev/null | json_field status; }

say "Seed"
WSKU="WH-SKU-$RUN_ID"; LA="WH-A-$RUN_ID"; LB="WH-B-$RUN_ID"
run_check "tenant"   0 "$T"    -- "$BIN" tenant create --code "$T" --name "Warehouse Co"
run_check "item"     0 "$WSKU" -- "$BIN" --tenant "$T" item create --sku "$WSKU" --name "Widget"
run_check "loc A"    0 "$LA"   -- "$BIN" location create --code "$LA" --type storage
run_check "loc B"    0 "$LB"   -- "$BIN" location create --code "$LB" --type storage
run_check "stock 100@A" 0 ""   -- "$BIN" --tenant "$T" inbound receive --line "$WSKU:100" --putaway-to "$LA"

say "Inventory move (A → B)"
run_check "move 40 A→B" 0 "" -- "$BIN" --tenant "$T" inventory move --sku "$WSKU" --from "$LA" --to "$LB" --qty 40
assert_eq "A now 60" 60 "$(oh_loc "$WSKU" "$LA")"
assert_eq "B now 40" 40 "$(oh_loc "$WSKU" "$LB")"
assert_eq "total still 100 (move conserves)" 100 "$(oh_sku "$WSKU")"
assert_eq "ledger has move_out+move_in (2)" 2 "$(dq_num "SELECT count(*) AS c FROM inventory_ledger WHERE type IN ('move_out','move_in')")"

say "Cycle count (discrepancy logged, on-hand unchanged)"
run_check "count B = 35 (actual 40)" 0 "" -- "$BIN" --tenant "$T" inventory count --location "$LB" --line "$WSKU:35"
assert_eq "B on-hand unchanged (count does not apply)" 40 "$(oh_loc "$WSKU" "$LB")"

say "Adjust (audited)"
run_check "adjust -10 @A (damage)" 0 "" -- "$BIN" --tenant "$T" inventory adjust --sku "$WSKU" --location "$LA" --qty -10 --reason "damage" --yes
assert_eq "A now 50" 50 "$(oh_loc "$WSKU" "$LA")"
assert_eq "adjust wrote an audit row" 1 "$(dq_num "SELECT count(*) AS c FROM audit_log WHERE action='inventory.adjust'")"

say "ASN lifecycle (receive → putaway → guards)"
A1="$("$BIN" --tenant "$T" --output json inbound create --ref "ASN1-$RUN_ID" --line "$WSKU:50" 2>/dev/null | json_field id)"
run_check "receive 50"           0 "" -- "$BIN" --tenant "$T" inbound receive "$A1" --line "$WSKU:50"
run_check "putaway 50 @B"        0 "" -- "$BIN" --tenant "$T" inbound putaway "$A1" --line "$WSKU@$LB:50"
assert_eq "B grew to 90 after putaway" 90 "$(oh_loc "$WSKU" "$LB")"
run_check "cancel after putaway → 409 (6)" 6 "" -- "$BIN" --tenant "$T" inbound cancel "$A1"
# short receive then over-putaway guard
A2="$("$BIN" --tenant "$T" --output json inbound create --ref "ASN2-$RUN_ID" --line "$WSKU:30" 2>/dev/null | json_field id)"
run_check "receive short (25 of 30)" 0 "" -- "$BIN" --tenant "$T" inbound receive "$A2" --line "$WSKU:25"
run_check "putaway 25"               0 "" -- "$BIN" --tenant "$T" inbound putaway "$A2" --line "$WSKU@$LB:25"
run_check "over-putaway → 409 (6)"   6 "" -- "$BIN" --tenant "$T" inbound putaway "$A2" --line "$WSKU@$LB:10"

say "Backorder → restock → re-allocate → ship"
BSKU="BO-SKU-$RUN_ID"; BLOC="BO-LOC-$RUN_ID"
run_check "backorder item"  0 "$BSKU" -- "$BIN" --tenant "$T" item create --sku "$BSKU" --name "Backordered"
run_check "backorder loc"   0 "$BLOC" -- "$BIN" location create --code "$BLOC" --type storage
BO="$("$BIN" --tenant "$T" --output json order create --ref "BO-$RUN_ID" --ship-to "Waiting Buyer" --line "$BSKU:20" 2>/dev/null | json_field id)"
assert_eq "order is backordered (no stock)" "backorder" "$(stat_of "$BO")"
run_check "restock 50"      0 "" -- "$BIN" --tenant "$T" inbound receive --line "$BSKU:50" --putaway-to "$BLOC"
run_check "re-allocate"     0 "" -- "$BIN" --tenant "$T" order allocate "$BO"
assert_eq "now allocated" "allocated" "$(stat_of "$BO")"
run_check "pick" 0 "" -- "$BIN" --tenant "$T" order pick "$BO" --line "$BSKU@$BLOC:20"
run_check "pack" 0 "" -- "$BIN" --tenant "$T" order pack "$BO"
run_check "ship" 0 "" -- "$BIN" --tenant "$T" order ship "$BO"
assert_eq "backordered SKU on-hand 30 after ship" 30 "$(oh_sku "$BSKU")"

say "Tenant offboarding (cascade delete via DELETE API)"
DEL="DELME${RANDOM}"
run_check "create disposable tenant" 0 "$DEL" -- "$BIN" tenant create --code "$DEL" --name "Disposable"
run_check "item under it"            0 ""     -- "$BIN" --tenant "$DEL" item create --sku "DEL-SKU-$RUN_ID" --name "Doomed"
DTOK="$(raw_login "$BOOT_EMAIL" "$BOOT_PW")"
code=$(curl -s -o /dev/null -w '%{http_code}' -X DELETE -H "authorization: Bearer $DTOK" "$ORIGIN/api/v1/dev/tenants/$DEL")
assert_eq "cascade delete returns 200" 200 "$code"
assert_eq "tenant gone" 0 "$(dq_num "SELECT count(*) AS c FROM tenants WHERE code='$DEL'")"
assert_eq "its items gone" 0 "$(dq_num "SELECT count(*) AS c FROM items it JOIN tenants t ON it.tenant_id=t.id WHERE t.code='$DEL'")"
# global conservation across everything still holds
assert_eq "global Σdelta == Σon_hand" \
  "$(dq_num 'SELECT COALESCE(SUM(delta),0) AS c FROM inventory_ledger')" \
  "$(dq_num 'SELECT COALESCE(SUM(qty_on_hand),0) AS c FROM inventory')"

summary
