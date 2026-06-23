# CLI ↔ backend integration test

`run.sh` is an end-to-end test that boots the **real backend** locally and drives
the `wms` CLI through a full warehouse lifecycle, asserting exit codes and output.

## What it does

1. Builds the CLI (`cargo build`).
2. Ensures `backend/.dev.vars` exists (writes test-only local secrets if missing;
   both `.dev.vars` and `.wrangler/` are gitignored).
3. Applies local D1 migrations and starts `wrangler dev` on `:8787` (local
   miniflare only — never touches remote Cloudflare).
4. Seeds a bootstrap **developer** user directly into local D1 (there is
   intentionally no seed endpoint). `seed-user.mjs` computes the Argon2id PHC and
   the HMAC blind index exactly as the backend does, so login works.
5. Runs the CLI through: auth · tenant/user · item/location · inbound blind
   receive + putaway · order create→allocate→pick→pack→ship · reports · guarded
   `inventory adjust` · ledger · `dev` debug/query/key · authz boundary.
6. Tears the backend down and prints a `PASS/FAIL` summary (non-zero exit on any
   failure).

## Run

```sh
cli/tests/integration/run.sh
```

Requires `node`, the backend's `wrangler` (its devDependency), and `cargo`.
Re-runnable and non-destructive: created entities use a per-run id; the bootstrap
user is inserted with `INSERT OR IGNORE`.

## Overrides (env)

`WMS_IT_PORT` (default 8787) · `WMS_IT_PII_KEY` · `WMS_IT_BIDX_KEY` ·
`WMS_IT_SESSION_SECRET`. If `backend/.dev.vars` already exists it is reused (the
seed reads its `BLIND_INDEX_KEY` so the HMAC matches the running worker).

## Note

This is a shell harness, not `cargo test` — it needs the backend toolchain and a
live worker, so it is kept out of the unit-test path. Run it before releases and
after changes to the client/transport layer.
