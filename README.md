# `wms` — 3PL WMS CLI

Official command-line client for the [3PL WMS](https://github.com/TechieQuokka/erp-wms-backend)
headless API. Written in Rust (edition 2024). The CLI is a thin client of the
HTTP API — every capability it exposes is an API call (see the backend's
`docs/api-contract.md` / `docs/cli-spec.md`).

## Install

```sh
cargo install --path .          # from this repo
# or, once published:
cargo install wms
# or download a prebuilt binary from GitHub Releases
```

## Quick start

```sh
wms config set endpoint https://your-wms.example.com
wms auth login --email you@example.com          # prompts for password, stores a session token
wms --tenant ACME item list
wms --tenant ACME inventory adjust --sku WIDGET --location A-01 --qty -3 --reason "damage" --yes
```

## Configuration & precedence

Settings resolve highest-priority first:

```
flag  >  WMS_* env  >  profile config  >  built-in default
```

Environment: `WMS_ENDPOINT`, `WMS_TOKEN`, `WMS_API_KEY`, `WMS_TENANT`.

Files (under `~/.config/wms/`):
- `config.toml` — endpoint, default output, default tenant, per profile;
- `credentials.toml` — session token / API key, written mode `0600`.

```sh
wms config set endpoint https://wms.example.com
wms config set default-tenant ACME
wms config set default-output json
wms --profile staging config set endpoint https://staging.example.com
wms config use staging          # switch the default profile
wms config profiles
```

## Authentication

- **Humans**: `wms auth login` exchanges email+password for a session token (stored per profile).
- **Machines/CI**: pass `--api-key` or set `WMS_API_KEY` (issued via `wms dev key create`).

## Command groups

`auth` · `config` · `tenant` · `user` · `item` · `location` · `inbound` ·
`inventory` · `order` · `report` · `alert` · `dev`

Run `wms <group> --help` for actions and flags. Available commands depend on the
role of your token; out-of-scope calls return `forbidden`.

Line specs:
- quantity lines: `--line <sku>:<qty>` (e.g. `--line WIDGET:10`);
- located lines: `--line <sku>@<location>:<qty>` (putaway / pick).

## Output

`--output table` (default, human) · `--output json` (scripting) · `--output csv`.
List commands follow cursor pagination automatically and print every page.

## Safety

- Guarded actions (`inventory adjust`, `dev user grant`) require `--yes` (or an
  interactive `y`). Destructive ones (`dev tenant delete`) also require typing a
  confirmation phrase. Non-interactive runs must pass `--yes` explicitly.
- Mutating creates/imports send an `Idempotency-Key` so retries are safe.
- `--dry-run` (imports) validates without writing.

## Exit codes

`0` success · `1` generic error · `2` usage · `3` auth/forbidden · `4` not found ·
`5` validation · `6` conflict · `7` rate-limited.

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets
cargo fmt
```

End-to-end test against a real local backend:
`tests/integration/run.sh` (see `tests/integration/README.md`).

## CI & releases

- **CI** (`.github/workflows/ci.yml`): fmt + clippy on Linux, and build + test on
  Linux/macOS/Windows for every push and PR.
- **Releases** (`.github/workflows/release.yml`): pushing a `vX.Y.Z` tag builds
  binaries for Linux (gnu/musl x86_64, aarch64), macOS (x86_64, aarch64) and
  Windows (x86_64), with `.sha256` checksums, and attaches them to the GitHub
  Release.

```sh
# cut a release
cargo test && git tag v0.1.0 && git push origin v0.1.0
```

Dual-licensed under MIT or Apache-2.0.
