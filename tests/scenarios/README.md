# CLI scenario suite (real deployed backend)

End-to-end **scenario** tests that drive the `wms` CLI against a **real, deployed
backend** — not a mock, not local miniflare. Each suite resets the target database
to a clean baseline (`wms dev reset`), seeds a realistic 3PL volume from CSV, and
asserts operational behaviour through CLI output / exit codes.

Complements `../integration/run.sh` (local `wrangler dev`, single lifecycle). Here
the point is **volume + deterministic state against the live deployed worker**.

## Run

```bash
cli/tests/scenarios/all.sh          # every suite in sequence
cli/tests/scenarios/run.sh          # happy-path lifecycle only
cli/tests/scenarios/security.sh     # one theme
```

Requires `node` + `cargo`. Re-runnable (each suite resets first).

### Target & creds (env)

| Var | Default | Meaning |
|-----|---------|---------|
| `WMS_SCENARIO_ENDPOINT` | deployed **test** worker | backend origin |
| `WMS_BOOTSTRAP_EMAIL` / `WMS_BOOTSTRAP_PASSWORD` | from `WMS_SECRETS_FILE` | bootstrap developer |
| `WMS_SECRETS_FILE` | `backend/.test-env-secrets` | gitignored creds file |

To target **prod** (a wipe-able prototype here), point at the prod origin and use
`backend/.test-env-secrets.prod`:

```bash
export WMS_SCENARIO_ENDPOINT=https://erp-wms-backend.adamstudio.workers.dev
export WMS_SECRETS_FILE=backend/.test-env-secrets.prod
export WMS_BOOTSTRAP_EMAIL=$(grep ^BOOTSTRAP_EMAIL= "$WMS_SECRETS_FILE" | cut -d= -f2-)
export WMS_BOOTSTRAP_PASSWORD=$(grep ^BOOTSTRAP_PASSWORD= "$WMS_SECRETS_FILE" | cut -d= -f2-)
cli/tests/scenarios/all.sh
```

## Suites

| File | Covers |
|------|--------|
| `run.sh` | Happy path: onboarding, catalog, inbound→putaway, order lifecycle, backorder, reports/alerts, tenant isolation, authz. |
| `security.sh` | Role matrix (API keys), API-key lifecycle, disable-blocks-login, login timing oracle, brute-force throttle, cross-tenant isolation. |
| `integrity.sh` | Reservation conservation, idempotent imports, negative/guard tests, cursor pagination, atomic + dry-run imports, Idempotency-Key probe. |
| `warehouse.sh` | Move / count / adjust (+audit), ASN receive/putaway/over-putaway/cancel guards, backorder→restock→re-allocate→ship, tenant cascade delete. |
| `reporting.sh` | inventory/as_of/inbound/outbound/activity/billing-data reports, large bulk import (timed), concurrency (no oversell), login latency. |
| `all.sh` | Runs them all (`security.sh` last — its brute-force probe throttles our IP). |

`lib.sh` holds shared helpers; `gen-fixtures.mjs` generates CSV fixtures matching the
backend import grammars.

## Notes & findings (from running against the real worker)

- **Large imports auto-chunk (was a ceiling, now fixed).** A single `order import`
  is bounded by the Workers per-request subrequest budget (one createOrder per order,
  ~10–13 D1 ops each) and D1's ~100 bound-parameter limit. The backend now caps a
  single import at 50 orders and chunks its dedup `inArray`; the **CLI splits large
  files into ≤50-order batches** (dry-run-all → apply-all), so big imports work.
- **Idempotency-Key is implemented (atomic, exactly-once)** — a D1 idempotency table
  with a UNIQUE key claims the request atomically, so even *concurrent* same-key
  submissions create exactly one resource; the stored response (AES-256-GCM encrypted)
  is replayed on retries, an in-flight duplicate gets 409, and a key reused with a
  different body gets 422. (D1 is strongly consistent — no wait needed.)
- **Cron storage-snapshot** can't be triggered over HTTP on a deployed worker, so
  `billing-data` is exercised via its data path only (`storageUnitDays` is 0 until
  the daily cron runs).
- **Login latency ≈ 1.2–1.7 s** (Argon2id on the free tier) — surfaced as an
  operational metric in `reporting.sh`.
- **Brute-force / per-IP throttle** keys off the real client IP on the deployed
  worker (the `cf-connecting-ip` header can't be spoofed), so failed-login tests
  briefly throttle the runner's IP; they run last.
- **`wms dev tenant delete`** is intentionally interactive-only (typed confirmation),
  so the cascade-delete scenario calls the `DELETE /dev/tenants/{code}` API directly.
