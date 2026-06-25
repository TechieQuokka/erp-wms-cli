#!/usr/bin/env bash
# Data-integrity & edge-case scenarios: reservation conservation, idempotent
# imports, negative/guard tests, cursor pagination, atomic + dry-run imports,
# and an Idempotency-Key probe (documents that dedup is not implemented).
set -uo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
SUITE="integrity"

build_cli
reset_baseline

T="INTG"
res_of() { dq_num "SELECT COALESCE(SUM(qty_reserved),0) AS c FROM inventory iv JOIN items it ON iv.item_id=it.id WHERE it.sku='$1'"; }
oh_of()  { dq_num "SELECT COALESCE(SUM(qty_on_hand),0)  AS c FROM inventory iv JOIN items it ON iv.item_id=it.id WHERE it.sku='$1'"; }
oid_of() { "$BIN" --tenant "$T" --output json order create --ref "$1" --ship-to "Buyer" --line "$2" 2>/dev/null | json_field id; }
ord_count() { dq_num "SELECT count(*) AS c FROM orders o JOIN tenants t ON o.tenant_id=t.id WHERE t.code='$T'"; }

say "Seed (tenant + bulk volume via CSV)"
FIX="$WORKDIR/fix"
TENANTS="$T" N_LOC=12 N_ITEMS=15 N_STOCK=10 N_ORDERS=60 node "$SCEN_DIR/gen-fixtures.mjs" "$FIX" "$RUN_ID"
run_check "tenant create"    0 "$T"      -- "$BIN" tenant create --code "$T" --name "Integrity Co"
run_check "import locations" 0 "created" -- "$BIN" location import "$FIX/locations.csv"
run_check "import items (15)" 0 "created" -- "$BIN" --tenant "$T" item import "$FIX/items-$T.csv"
while IFS=$'\t' read -r tn sku loc qty; do [ -z "$tn" ] && continue; "$BIN" --tenant "$T" inbound receive --line "$sku:$qty" --putaway-to "$loc" </dev/null >/dev/null 2>&1; done < "$FIX/stock.tsv"
# dedicated SKU + location for reservation/negative tests (separate from bulk)
RSKU="RES-SKU-$RUN_ID"; RLOC="RES-LOC-$RUN_ID"
run_check "reservation item"  0 "$RSKU" -- "$BIN" --tenant "$T" item create --sku "$RSKU" --name "Res Widget"
run_check "reservation loc"   0 "$RLOC" -- "$BIN" location create --code "$RLOC" --type storage
run_check "reservation stock" 0 ""       -- "$BIN" --tenant "$T" inbound receive --line "$RSKU:100" --putaway-to "$RLOC"

say "Bulk orders + cursor pagination + idempotent imports (exactly 60 orders here)"
run_check "import orders (60)"  0 "created" -- "$BIN" --tenant "$T" order import "$FIX/orders-$T.csv"
norders="$("$BIN" --tenant "$T" --output json order list 2>/dev/null | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{try{const j=JSON.parse(s);const a=j.data||j;process.stdout.write(String(new Set(a.map(o=>o.id)).size))}catch{process.stdout.write('ERR')}})")"
assert_eq "order list paginates all 60 (no dup/missing)" 60 "$norders"
run_check "re-import items: 0 created"  0 "0 created" -- "$BIN" --tenant "$T" item import "$FIX/items-$T.csv"
run_check "re-import orders (idempotent)" 0 ""        -- "$BIN" --tenant "$T" order import "$FIX/orders-$T.csv"
assert_eq "orders still 60 after re-import" 60 "$(ord_count)"

say "Reservation conservation"
assert_eq "initial reserved 0" 0 "$(res_of "$RSKU")"
OA="$(oid_of "RES-A-$RUN_ID" "$RSKU:30")"
assert_eq "after allocate reserved 30" 30 "$(res_of "$RSKU")"
run_check "cancel releases" 0 "" -- "$BIN" --tenant "$T" order cancel "$OA" --reason "test release"
assert_eq "after cancel reserved 0" 0 "$(res_of "$RSKU")"
OB="$(oid_of "RES-B-$RUN_ID" "$RSKU:40")"
run_check "pick" 0 "" -- "$BIN" --tenant "$T" order pick "$OB" --line "$RSKU@$RLOC:40"
run_check "pack" 0 "" -- "$BIN" --tenant "$T" order pack "$OB"
run_check "ship" 0 "" -- "$BIN" --tenant "$T" order ship "$OB"
assert_eq "after ship on_hand 60" 60 "$(oh_of "$RSKU")"
assert_eq "after ship reserved 0"  0 "$(res_of "$RSKU")"

say "Negative / guard tests"
OC="$(oid_of "RES-C-$RUN_ID" "$RSKU:10")"
run_check "over-pick rejected (conflict 6)" 6 "" -- "$BIN" --tenant "$T" order pick "$OC" --line "$RSKU@$RLOC:20"
run_check "adjust below zero rejected (5)"  5 "" -- "$BIN" --tenant "$T" inventory adjust --sku "$RSKU" --location "$RLOC" --qty -99999 --reason "underflow" --yes
run_check "unknown sku order (not_found 4)" 4 "" -- "$BIN" --tenant "$T" order create --ref "BAD-$RUN_ID" --line "NO-SUCH-SKU:1"

say "Atomic + dry-run imports"
BAD="$WORKDIR/bad-items.csv"; printf 'sku,name,min_level\nOKSKU-%s,Fine,0\nBADSKU-%s,Broken,notanumber\n' "$RUN_ID" "$RUN_ID" > "$BAD"
before="$(dq_num "SELECT count(*) AS c FROM items")"
run_check "atomic import rejects bad row (5)" 5 "" -- "$BIN" --tenant "$T" item import "$BAD"
assert_eq "no rows written on bad import" "$before" "$(dq_num "SELECT count(*) AS c FROM items")"
run_check "dry-run writes nothing" 0 "dry-run" -- "$BIN" --tenant "$T" item import "$FIX/items-$T.csv" --dry-run
assert_eq "dry-run left counts unchanged" "$before" "$(dq_num "SELECT count(*) AS c FROM items")"

say "Idempotency-Key (atomic exactly-once, D1)"
DTOK="$(raw_login "$BOOT_EMAIL" "$BOOT_PW")"
cnt_ref() { dq_num "SELECT count(*) AS c FROM orders o JOIN tenants t ON o.tenant_id=t.id WHERE t.code='$T' AND o.ref='$1'"; }
# (1) retry replays the same order — strongly consistent, no wait needed
IREF="IDEM-$RUN_ID"; IKEY="idem-$RUN_ID"
mk_idem() { curl -s -X POST -H "authorization: Bearer $DTOK" -H "x-tenant: $T" -H 'content-type: application/json' -H "idempotency-key: $IKEY" --data "{\"ref\":\"$IREF\",\"lines\":[{\"sku\":\"$RSKU\",\"qty\":1}]}" "$ORIGIN/api/v1/orders"; }
ID1="$(mk_idem | json_field id)"
ID2="$(mk_idem | json_field id)"
assert_eq "immediate retry replays same order" "$ID1" "$ID2"
assert_eq "retry → exactly one order" 1 "$(cnt_ref "$IREF")"
# (2) two CONCURRENT same-key requests still create exactly one
CREF="IDEMC-$RUN_ID"; CKEY="idemc-$RUN_ID"
mk_c() { curl -s -o /dev/null -X POST -H "authorization: Bearer $DTOK" -H "x-tenant: $T" -H 'content-type: application/json' -H "idempotency-key: $CKEY" --data "{\"ref\":\"$CREF\",\"lines\":[{\"sku\":\"$RSKU\",\"qty\":1}]}" "$ORIGIN/api/v1/orders"; }
mk_c & mk_c & wait
assert_eq "concurrent same-key → exactly one order" 1 "$(cnt_ref "$CREF")"
# (3) same key, different payload → 422
DREF="IDEMD-$RUN_ID"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H "authorization: Bearer $DTOK" -H "x-tenant: $T" -H 'content-type: application/json' -H "idempotency-key: $IKEY" --data "{\"ref\":\"$DREF\",\"lines\":[{\"sku\":\"$RSKU\",\"qty\":2}]}" "$ORIGIN/api/v1/orders")
assert_eq "reused key + different payload → 422" 422 "$code"

summary
