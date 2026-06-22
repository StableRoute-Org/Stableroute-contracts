# stableroute-contracts

Soroban smart contracts for [StableRoute](https://github.com/your-org/stableroute) — Stellar liquidity routing protocol.

## What this repo contains

- **StableRouteRouter** — Soroban contract placeholder for routing metadata and route integrity (version, route tags). Production logic will integrate with path payments and liquidity data.

## Prerequisites

- [Rust](https://rustup.rs/) (stable, with `rustfmt`)
- Optional: [Soroban CLI](https://soroban.stellar.org/docs/tools/cli) for deployment

## Setup (contributors)

1. Clone the repo and enter the directory:
   ```bash
   git clone <repo-url> && cd stableroute-contracts
   ```
2. Install Rust (if needed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup component add rustfmt clippy
   rustup target add wasm32-unknown-unknown
   cargo install cargo-llvm-cov
   ```
3. Build and test:
   ```bash
   cargo build
   cargo clippy --all-targets -- -D warnings
   cargo test
   cargo build --target wasm32-unknown-unknown --release
   cargo llvm-cov --all-targets --fail-under-lines 95
   ```
4. Check formatting:
   ```bash
   cargo fmt --all -- --check
   ```

## Commands

| Command | Description |
|--------|-------------|
| `cargo build` | Build the contracts |
| `cargo test` | Run unit tests |
| `cargo clippy --all-targets -- -D warnings` | Treat Rust lints and warnings as CI failures |
| `cargo build --target wasm32-unknown-unknown --release` | Build the deployable Soroban WASM artifact |
| `cargo llvm-cov --all-targets --fail-under-lines 95` | Report coverage and fail below 95 percent line coverage |
| `cargo fmt --all` | Format code |
| `cargo fmt --all -- --check` | CI: verify formatting |

## CI/CD

On every push/PR to `main`, GitHub Actions runs:

- `cargo fmt --all -- --check`
- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `cargo build --target wasm32-unknown-unknown --release`
- `cargo llvm-cov --all-targets --fail-under-lines 95`

Ensure these pass locally before pushing.

## Contributing

1. Fork the repo and create a branch from `main`.
2. Make changes; keep formatting, linting, tests, WASM build, and coverage passing.
3. Open a PR; CI must be green.
4. Follow the project’s code style (enforced by `rustfmt`).

### Internal helper conventions

**`require_admin`** — every admin-gated entrypoint in `StableRouteRouter` calls the private `fn require_admin(env: &Env) -> Address` helper instead of repeating the load-unwrap-require_auth block inline. When adding a new admin-gated entrypoint, start the body with `Self::require_admin(&env);`. Do not duplicate the pattern manually.

## RouterError reference

`src/lib.rs` defines `RouterError` as an append-only Soroban contract error enum. Off-chain SDKs, frontends, indexers, and runbooks should treat `Error(Contract, #N)` as a stable integration contract: once a variant ships, its numeric code must not be reused or renumbered. New errors should be appended with the next available code and this table should be updated in the same change.

| Code | Variant | Raised by | Operator-facing meaning and remedy |
|------|---------|-----------|------------------------------------|
| 1 | `AlreadyInitialized` | `init` | The router already has an admin address. Do not call `init` again on the same deployment; use the admin-transfer flow for handover or deploy a fresh instance if this is a setup mistake. |
| 2 | `NotInitialized` | `require_admin`, reached by every admin-gated entrypoint before `init` | The admin address has not been stored yet. Initialize the router with `init(admin)` before calling admin operations such as `pause`, `register_pair`, or fee/liquidity setters. |
| 3 | `SourceEqualsDestination` | `register_pair` | The source and destination symbols are identical. Register directional pairs with distinct assets, for example `USDC -> EURC`, and handle reverse routing as a separate pair. |
| 4 | `FeeBpsTooHigh` | `set_pair_fee_bps` | The requested fee is above `MAX_FEE_BPS` (1,000 bps / 10%). Lower the configured fee or make a deliberate governance/code change before raising the cap. |
| 5 | `PairNotRegistered` | `quote_route`, `compute_route_fee` | A caller requested a quote or fee for an unregistered pair. Register the pair first with `register_pair`, or route the request through a supported pair. |
| 6 | `AmountMustBePositive` | `quote_route`, `compute_route_fee`, `set_pair_liquidity`, `set_pair_max_amount`, `set_pair_min_amount` | The supplied amount or limit is zero or negative where a positive/non-negative value is required. Validate UI and API inputs before submitting the transaction. |
| 7 | `NoPendingAdminTransfer` | `accept_admin_transfer` | No pending admin handover exists. The current admin must call `propose_admin_transfer(new_admin)` before the new admin accepts. |
| 8 | `NotPendingAdmin` | `accept_admin_transfer` | The caller is not the address stored in `PendingAdmin`. Have the proposed address accept the transfer, or cancel and propose the intended address again. |
| 9 | `ContractPaused` | `register_pair`, `set_pair_fee_bps` | A state-changing router operation was attempted while the router is paused. Investigate why the pause is active, then call `unpause` from the admin account before resuming operations. |
| 10 | `AmountBelowMin` | `compute_route_fee` | The route amount is below the pair's configured minimum. Raise the requested amount or lower `PairMinAmount` through the admin flow. |
| 11 | `AmountAboveMax` | `compute_route_fee` | The route amount is above the pair's configured maximum. Split the route, lower the requested amount, or raise `PairMaxAmount` through the admin flow. |
| 12 | `InsufficientLiquidity` | `compute_route_fee` | The reported pair liquidity is below the requested amount. Wait for more liquidity, refresh the off-chain liquidity report, or route through a different pair. |
| 13 | `MigrationVersionMismatch` | `migrate_v1_to_v2` | The stored schema version is not the expected v1 starting point. Confirm the deployment's current schema version before retrying or writing a new migration. |

### Error-code stability guarantees

- Codes are stable and append-only. Never rename, remove, reuse, or renumber an existing `RouterError` variant after release.
- Client integrations should branch on the numeric code and may display the variant name as a friendly label.
- Adding a new error requires three synchronized updates: append the enum variant in `src/lib.rs`, add or update tests for the new failure path, and add exactly one row to the table above.
- Remedies should remain operational and non-sensitive. Do not document private keys, internal credentials, or liquidity-source secrets in error guidance.

## License

MIT

