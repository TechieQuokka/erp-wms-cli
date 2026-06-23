#!/usr/bin/env bash
#
# End-to-end integration test: boots the REAL backend locally (wrangler dev +
# local D1/KV), seeds a bootstrap operator, and drives the `wms` CLI through a
# full warehouse lifecycle, asserting exit codes and output.
#
#   cli/tests/integration/run.sh
#
# Requirements: node, npx/wrangler (backend devDependency), cargo. Uses only the
# LOCAL miniflare state — it does not touch any remote Cloudflare resources.
# Re-runnable and non-destructive: all created entities use a per-run id.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
BACKEND_DIR="$ROOT/backend"
CLI_DIR="$ROOT/cli"

PORT="${WMS_IT_PORT:-8787}"
EP="http://127.0.0.1:$PORT"
RUN_ID="$(date +%s)"
WORKDIR="$(mktemp -d)"
export XDG_CONFIG_HOME="$WORKDIR/config"

# Test-only local secrets (overridable). PII key must decode to >= 32 bytes.
PII_MASTER_KEY="${WMS_IT_PII_KEY:-dGVzdC1waWktbWFzdGVyLWtleS0zMmJ5dGVzLWxvbmchIQ==}"
BLIND_INDEX_KEY="${WMS_IT_BIDX_KEY:-it-blind-index-key}"
SESSION_SECRET="${WMS_IT_SESSION_SECRET:-it-session-secret}"

BOOT_EMAIL="wms-it-dev@local.test"
BOOT_PW="it-password-123"

PASS=0
FAIL=0
WRANGLER_PID=""

cleanup() {
  # wrangler dev spawns workerd in the same process group (we start it via
  # setsid), so kill the whole group to avoid leaking the runtime child.
  if [ -n "$WRANGLER_PID" ]; then
    kill -TERM -"$WRANGLER_PID" 2>/dev/null || kill "$WRANGLER_PID" 2>/dev/null
  fi
  rm -f "$BACKEND_DIR/.wms-it-seed.mjs"
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

# run_check <desc> <expected_exit> <needle|""> -- <cmd...>
run_check() {
  local desc="$1" exp="$2" needle="$3"; shift 3
  [ "$1" = "--" ] && shift
  local out rc
  out="$("$@" </dev/null 2>&1)"; rc=$?
  if [ "$rc" -ne "$exp" ]; then
    printf '\033[31mFAIL\033[0m %s (exit %s, expected %s)\n' "$desc" "$rc" "$exp"
    sed 's/^/      /' <<<"$out"
    FAIL=$((FAIL + 1)); return
  fi
  if [ -n "$needle" ] && ! grep -qF -- "$needle" <<<"$out"; then
    printf '\033[31mFAIL\033[0m %s (output missing "%s")\n' "$desc" "$needle"
    sed 's/^/      /' <<<"$out"
    FAIL=$((FAIL + 1)); return
  fi
  printf '\033[32mPASS\033[0m %s\n' "$desc"
  PASS=$((PASS + 1))
}

# ---------------------------------------------------------------- build CLI
say "Building the CLI"
( cd "$CLI_DIR" && cargo build --quiet ) || { echo "cargo build failed"; exit 1; }
BIN="$CLI_DIR/target/debug/wms"

# ---------------------------------------------------------------- .dev.vars
say "Preparing backend local secrets (.dev.vars)"
DEV_VARS="$BACKEND_DIR/.dev.vars"
if [ -f "$DEV_VARS" ]; then
  echo "using existing $DEV_VARS"
  # Reuse its blind-index key so the seed's HMAC matches the worker's.
  existing_bidx="$(grep -E '^BLIND_INDEX_KEY=' "$DEV_VARS" | head -1 | cut -d= -f2- | tr -d '"'"'"'')"
  [ -n "$existing_bidx" ] && BLIND_INDEX_KEY="$existing_bidx"
else
  cat >"$DEV_VARS" <<EOF
PII_MASTER_KEY=$PII_MASTER_KEY
BLIND_INDEX_KEY=$BLIND_INDEX_KEY
SESSION_SECRET=$SESSION_SECRET
TURNSTILE_SECRET=
EOF
  echo "wrote $DEV_VARS (test-only local secrets)"
fi

export WRANGLER_SEND_METRICS=false CI=1

# ---------------------------------------------------------------- migrations
say "Applying local D1 migrations"
( cd "$BACKEND_DIR" && npx wrangler d1 migrations apply wms --local ) \
  || { echo "migrations failed"; exit 1; }

# ---------------------------------------------------------------- boot worker
say "Starting wrangler dev on :$PORT"
# setsid → its own process group so cleanup can kill wrangler + workerd together.
setsid bash -c "cd '$BACKEND_DIR' && exec npx wrangler dev --port '$PORT'" \
  >"$WORKDIR/wrangler.log" 2>&1 &
WRANGLER_PID=$!

printf 'waiting for %s/api/v1/health ' "$EP"
ready=""
for _ in $(seq 1 90); do
  if curl -fsS "$EP/api/v1/health" >/dev/null 2>&1; then ready=1; break; fi
  printf '.'; sleep 1
done
echo
if [ -z "$ready" ]; then
  echo "backend did not become ready; last log lines:"; tail -30 "$WORKDIR/wrangler.log"; exit 1
fi

# ---------------------------------------------------------------- seed user
say "Seeding bootstrap developer ($BOOT_EMAIL)"
SEED_SQL="$WORKDIR/seed.sql"
# ESM resolves bare imports (`@noble/hashes`) from the script's own directory, so
# run a copy placed inside the backend (where node_modules lives), not from cli/.
SEED_TMP="$BACKEND_DIR/.wms-it-seed.mjs"
cp "$SCRIPT_DIR/seed-user.mjs" "$SEED_TMP"
( cd "$BACKEND_DIR" && BLIND_INDEX_KEY="$BLIND_INDEX_KEY" \
  node "$SEED_TMP" "$BOOT_EMAIL" "$BOOT_PW" developer >"$SEED_SQL" ) \
  || { rm -f "$SEED_TMP"; echo "seed script failed"; exit 1; }
rm -f "$SEED_TMP"
( cd "$BACKEND_DIR" && npx wrangler d1 execute wms --local --file "$SEED_SQL" ) \
  || { echo "seed insert failed"; exit 1; }

# ---------------------------------------------------------------- CLI config
say "Configuring CLI profile"
"$BIN" config set endpoint "$EP" >/dev/null

# ---------------------------------------------------------------- lifecycle
TCODE="IT${RUN_ID}"
SKU="SKU-${RUN_ID}"
LOC="L-${RUN_ID}"
WORKER_EMAIL="worker-${RUN_ID}@local.test"

say "Auth"
run_check "login as developer"        0 "role: developer"  -- "$BIN" auth login --email "$BOOT_EMAIL" --password "$BOOT_PW"
run_check "whoami → developer"        0 "developer"        -- "$BIN" auth whoami
run_check "bad login → exit 3"        3 ""                 -- "$BIN" auth login --email "$BOOT_EMAIL" --password wrong

say "Tenants & users"
run_check "tenant create"             0 "$TCODE"           -- "$BIN" tenant create --name "IT $RUN_ID" --code "$TCODE"
run_check "tenant get"                0 "$TCODE"           -- "$BIN" tenant get "$TCODE"
run_check "tenant list"               0 "$TCODE"           -- "$BIN" tenant list
run_check "tenant get missing → 4"    4 ""                 -- "$BIN" tenant get "NOPE${RUN_ID}"
run_check "user create (worker)"      0 "$WORKER_EMAIL"    -- "$BIN" user create --email "$WORKER_EMAIL" --name "W $RUN_ID" --role worker
"$BIN" config set default-tenant "$TCODE" >/dev/null

say "Catalog"
run_check "item create"               0 "$SKU"             -- "$BIN" item create --sku "$SKU" --name "Widget" --min-level 5
run_check "item list"                 0 "$SKU"             -- "$BIN" item list
run_check "location create"           0 "$LOC"             -- "$BIN" location create --code "$LOC" --type storage

say "Inbound → stock"
run_check "blind receive + putaway"   0 ""                 -- "$BIN" inbound receive --line "$SKU:100" --putaway-to "$LOC"
run_check "inventory shows 100"       0 "100"              -- "$BIN" inventory list --sku "$SKU"

say "Outbound lifecycle"
ORD_JSON="$("$BIN" --output json order create --ref "O-$RUN_ID" --ship-to "Acme Inc" --line "$SKU:10" 2>/dev/null)"
ORD_ID="$(sed -n 's/.*"id": *"\([^"]*\)".*/\1/p' <<<"$ORD_JSON" | head -1)"
echo "order id: $ORD_ID"
run_check "order get"                 0 "$ORD_ID"          -- "$BIN" order get "$ORD_ID"
run_check "order allocate"            0 ""                 -- "$BIN" order allocate "$ORD_ID"
run_check "order pick"                0 ""                 -- "$BIN" order pick "$ORD_ID" --line "$SKU:10"
run_check "order pack"                0 ""                 -- "$BIN" order pack "$ORD_ID"
run_check "order ship"                0 ""                 -- "$BIN" order ship "$ORD_ID" --tracking "TRACK-$RUN_ID"
run_check "inventory drops to 90"     0 "90"               -- "$BIN" inventory list --sku "$SKU"

say "Reports & guarded ops"
run_check "report inventory"          0 ""                 -- "$BIN" report inventory
run_check "inventory adjust (--yes)"  0 ""                 -- "$BIN" inventory adjust --sku "$SKU" --location "$LOC" --qty -5 --reason "IT shrink" --yes
run_check "ledger lists movements"    0 "$SKU"             -- "$BIN" inventory ledger --sku "$SKU"

say "Dev namespace"
run_check "dev debug"                 0 ""                 -- "$BIN" dev debug
run_check "dev query (read-only)"     0 ""                 -- "$BIN" dev query "SELECT count(*) AS n FROM tenants"
run_check "dev query rejects write"   3 ""                 -- "$BIN" dev query "DELETE FROM tenants"
KEY_JSON="$("$BIN" --output json dev key create --name "it-$RUN_ID" --role worker 2>/dev/null)"
KEY_ID="$(sed -n 's/.*"id": *"\([^"]*\)".*/\1/p' <<<"$KEY_JSON" | head -1)"
run_check "dev key revoke"            0 ""                 -- "$BIN" dev key revoke "$KEY_ID"

say "Authz boundary"
# An unauthenticated request to an operator-only surface must be rejected.
run_check "unauth tenant create → 3"  3 ""                 -- "$BIN" --token "bogus" --endpoint "$EP" tenant create --name x --code "X$RUN_ID"

# ---------------------------------------------------------------- summary
say "Summary"
printf 'PASS=%d  FAIL=%d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
echo "integration: all green"
