#!/usr/bin/env bash
# Shared helpers for the CLI scenario suite. `source` this from a theme script.
#
# Target + bootstrap creds come from env (or a secrets file):
#   WMS_SCENARIO_ENDPOINT   backend origin (default: deployed test worker)
#   WMS_BOOTSTRAP_EMAIL / WMS_BOOTSTRAP_PASSWORD  (default: from WMS_SECRETS_FILE)
#   WMS_SECRETS_FILE        default backend/.test-env-secrets
#
# CLI exit codes (error.rs): 2 usage · 3 unauth/forbidden · 4 not_found ·
#   5 validation · 6 conflict · 7 rate_limited · 1 other.

SCEN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCEN_DIR/../../.." && pwd)"
CLI_DIR="$ROOT/cli"
BACKEND_DIR="$ROOT/backend"
BIN="$CLI_DIR/target/debug/wms"

ORIGIN="${WMS_SCENARIO_ENDPOINT:-https://erp-wms-backend-test.adamstudio.workers.dev}"
SECRETS_FILE="${WMS_SECRETS_FILE:-$BACKEND_DIR/.test-env-secrets}"
_rs() { [ -f "$SECRETS_FILE" ] && grep -E "^$1=" "$SECRETS_FILE" | head -1 | cut -d= -f2- || true; }
BOOT_EMAIL="${WMS_BOOTSTRAP_EMAIL:-$(_rs BOOTSTRAP_EMAIL)}"
BOOT_PW="${WMS_BOOTSTRAP_PASSWORD:-$(_rs BOOTSTRAP_PASSWORD)}"

RUN_ID="$(date +%s)$RANDOM"
WORKDIR="$(mktemp -d)"
export XDG_CONFIG_HOME="$WORKDIR/config"   # isolate the CLI profile
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
assert_eq() { if [ "$2" = "$3" ]; then pass "$1 (=$3)"; else fail "$1 (expected $2, got $3)"; fi; }

# Developer read-only SQL (uses the logged-in developer profile). Prints first value.
dq_num() {
  "$BIN" --output json dev query "$1" 2>/dev/null | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{try{let j=JSON.parse(s);if(j&&j.rows)j=j.rows;const r=Array.isArray(j)?(j[0]||{}):(j||{});process.stdout.write(String(Object.values(r)[0]))}catch{process.stdout.write('ERR')}})"
}
# Extract a field from JSON stdin: object, {data:[..]}, or array → first.id by default.
json_field() { node -e "let s='';const k=process.argv[1]||'id';process.stdin.on('data',d=>s+=d).on('end',()=>{try{const j=JSON.parse(s);let o=j.data||j;o=Array.isArray(o)?o[0]:o;o=o&&o.user?o.user:o;process.stdout.write(String((o&&o[k])??''))}catch{process.stdout.write('')}})" "$1"; }

build_cli() { say "Building the CLI"; ( cd "$CLI_DIR" && cargo build --quiet ) || { echo "cargo build failed"; exit 1; }; }

# Raw curl login → echoes the session token (empty on failure). Does not touch the profile.
raw_login() {
  curl -s -X POST -H 'content-type: application/json' --data "$(printf '{"email":"%s","password":"%s"}' "$1" "$2")" "$ORIGIN/api/v1/auth/login" \
    | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>{try{process.stdout.write(JSON.parse(s).token||'')}catch{process.stdout.write('')}})"
}

# Log in as the bootstrap developer, wipe to a clean baseline, and re-login.
reset_baseline() {
  say "Reset to clean baseline ($ORIGIN)"
  "$BIN" config set endpoint "$ORIGIN" >/dev/null
  run_check "login (bootstrap dev)"     0 "developer" -- "$BIN" auth login --email "$BOOT_EMAIL" --password "$BOOT_PW"
  run_check "dev reset"                 0 ""          -- "$BIN" dev reset --seed-email "$BOOT_EMAIL" --seed-password "$BOOT_PW" --confirm RESET
  run_check "re-login after reset"      0 "developer" -- "$BIN" auth login --email "$BOOT_EMAIL" --password "$BOOT_PW"
}

# create_key <role> [tenant] → echoes the raw API key (developer profile must be active).
create_key() {
  local role="$1" tenant="${2:-}"
  if [ -n "$tenant" ]; then
    "$BIN" --output json dev key create --name "k-$role-$RUN_ID" --role "$role" --tenant "$tenant" 2>/dev/null | json_field key
  else
    "$BIN" --output json dev key create --name "k-$role-$RUN_ID" --role "$role" 2>/dev/null | json_field key
  fi
}

summary() {
  say "Summary"; printf 'PASS=%d  FAIL=%d  (%s, target %s)\n' "$PASS" "$FAIL" "${SUITE:-suite}" "$ORIGIN"
  [ "$FAIL" -eq 0 ] || exit 1
}
