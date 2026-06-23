#!/usr/bin/env bash
# Security & access-control scenarios (role matrix, API keys, session/login,
# brute-force, cross-tenant). Uses API keys for the role matrix (fast SHA-256
# verify) to avoid the Argon2 login cost; the few real logins are ordered BEFORE
# any failed login, because the per-IP brute-force throttle keys off our single
# real IP on the deployed worker.
set -uo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
SUITE="security"

build_cli
reset_baseline   # two legit logins while the per-IP fail counter is 0

# --- seed a tenant + catalog + stock (developer acts as worker+/admin) -------
say "Seed"
TC="SEC${RANDOM}"; TC2="SEX${RANDOM}"
SKU="SEC-SKU-$RUN_ID"; LOC="SEC-LOC-$RUN_ID"
run_check "tenant create"        0 "$TC"  -- "$BIN" tenant create --code "$TC" --name "Sec Co"
run_check "tenant2 create"       0 "$TC2" -- "$BIN" tenant create --code "$TC2" --name "Sec Co 2"
run_check "item create"          0 "$SKU" -- "$BIN" --tenant "$TC" item create --sku "$SKU" --name "Widget"
run_check "location create"      0 "$LOC" -- "$BIN" location create --code "$LOC" --type storage
run_check "stock"                0 ""      -- "$BIN" --tenant "$TC" inbound receive --line "$SKU:50" --putaway-to "$LOC"

# --- a worker session login (legit, IP fail counter still 0) -----------------
say "Session / login lifecycle"
SUSER="sess-$RUN_ID@wmsprod.test"; SPW="session-pass-123"
SUID="$("$BIN" --output json user create --email "$SUSER" --name "Sess" --role worker --password "$SPW" 2>/dev/null | json_field id)"
STOK="$(raw_login "$SUSER" "$SPW")"
assert_eq "worker session issued" "yes" "$([ -n "$STOK" ] && echo yes || echo no)"
run_check "worker session works"  0 "$SKU" -- "$BIN" --token "$STOK" --tenant "$TC" item list

# --- timing-oracle: absent vs wrong-password both run Argon2 (early, IP=0) ----
say "Login timing oracle (absent vs wrong password ~ both hash)"
flt_gt() { node -e "process.exit((parseFloat(process.argv[1])>parseFloat(process.argv[2]))?0:1)" "$1" "$2"; }
T_ABSENT=$(curl -s -o /dev/null -w '%{time_total}' -X POST -H 'content-type: application/json' --data '{"email":"nobody-'"$RUN_ID"'@wmsprod.test","password":"whatever-123"}' "$ORIGIN/api/v1/auth/login")
T_WRONG=$(curl -s -o /dev/null -w '%{time_total}' -X POST -H 'content-type: application/json' --data "$(printf '{"email":"%s","password":"definitely-wrong"}' "$SUSER")" "$ORIGIN/api/v1/auth/login")
if flt_gt "$T_ABSENT" 0.3 && flt_gt "$T_WRONG" 0.3; then pass "absent & wrong both hash (no timing oracle): absent=${T_ABSENT}s wrong=${T_WRONG}s"; else fail "timing oracle suspected: absent=${T_ABSENT}s wrong=${T_WRONG}s"; fi

# --- role matrix via API keys (no Argon2) ------------------------------------
say "Role matrix (API keys)"
WK="$(create_key worker)"; AK="$(create_key admin)"; SK="$(create_key shipper "$TC")"
assert_eq "keys minted" "yes" "$([ -n "$WK" ] && [ -n "$AK" ] && [ -n "$SK" ] && echo yes || echo no)"
# worker: warehouse ops yes, operator surfaces no
run_check "worker reads items"            0 "$SKU" -- "$BIN" --api-key "$WK" --tenant "$TC" item list
run_check "worker cannot create user (3)" 3 ""     -- "$BIN" --api-key "$WK" user create --email "x$RANDOM@wmsprod.test" --name X --role worker --password password-123
run_check "worker cannot create tenant (3)" 3 "" -- "$BIN" --api-key "$WK" tenant create --code "Z$RANDOM" --name Z
run_check "worker cannot dev debug (3)"     3 "" -- "$BIN" --api-key "$WK" dev debug
# admin: manage worker/shipper yes, developer namespace no
run_check "admin creates worker user"       0 "" -- "$BIN" --api-key "$AK" user create --email "au$RANDOM@wmsprod.test" --name AU --role worker --password password-123
run_check "admin cannot dev key create (3)" 3 "" -- "$BIN" --api-key "$AK" dev key create --name nope --role worker
# shipper: read own tenant yes, writes no (CLI requires --tenant; server pins shipper anyway)
run_check "shipper reads inventory"         0 "" -- "$BIN" --api-key "$SK" --tenant "$TC" inventory list
run_check "shipper cannot create order (3)" 3 "" -- "$BIN" --api-key "$SK" --tenant "$TC" order create --ref "O$RANDOM" --line "$SKU:1"
run_check "shipper cannot create item (3)"  3 "" -- "$BIN" --api-key "$SK" --tenant "$TC" item create --sku "X$RANDOM" --name X

# --- API key lifecycle: create → use → revoke → denied -----------------------
say "API key lifecycle"
KJSON="$("$BIN" --output json dev key create --name "lc-$RUN_ID" --role worker 2>/dev/null)"
LK="$(echo "$KJSON" | json_field key)"; KID="$(echo "$KJSON" | json_field id)"
run_check "new key authenticates"  0 "$SKU" -- "$BIN" --api-key "$LK" --tenant "$TC" item list
run_check "revoke key"             0 ""      -- "$BIN" dev key revoke "$KID"
run_check "revoked key denied (3)" 3 ""      -- "$BIN" --api-key "$LK" --tenant "$TC" item list

# --- cross-tenant isolation --------------------------------------------------
say "Cross-tenant isolation"
run_check "item visible in its tenant"      0 "$SKU" -- "$BIN" --tenant "$TC"  item get "$SKU"
run_check "item NOT visible cross-tenant (4)" 4 "" -- "$BIN" --tenant "$TC2" item get "$SKU"

# --- session: a disabled user can no longer authenticate ---------------------
say "Disable blocks new logins"
run_check "disable the worker user" 0 "" -- "$BIN" user disable "$SUID"
CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' --data "$(printf '{"email":"%s","password":"%s"}' "$SUSER" "$SPW")" "$ORIGIN/api/v1/auth/login")
if [ "$CODE" = "401" ] || [ "$CODE" = "429" ]; then pass "disabled user login blocked (http $CODE)"; else fail "disabled user login not blocked (http $CODE)"; fi

# --- brute-force throttle (LAST: this throttles our IP) ----------------------
say "Brute-force throttle (runs last)"
GOT429=0
for i in 1 2 3 4 5 6 7 8; do
  c=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' --data "$(printf '{"email":"bf-%s@wmsprod.test","password":"bad-%s"}' "$RUN_ID" "$i")" "$ORIGIN/api/v1/auth/login")
  [ "$c" = "429" ] && GOT429=1
done
assert_eq "rapid bad logins trigger 429" 1 "$GOT429"

summary
