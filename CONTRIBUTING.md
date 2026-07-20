# Contributing to stableroute-contracts

Thanks for contributing to the StableRoute Soroban contracts. This guide
documents the conventions the `StableRouteRouter` contract relies on. Every
rule below is enforced by the code in `src/lib.rs` and by CI, so following
them keeps reviews fast and avoids breaking on-chain compatibility.

Questions or want to coordinate? Join the StableRoute Discord:
<https://discord.gg/37aCpusvx>

## Error codes are append-only

`RouterError` is a `#[contracterror]` enum with an explicit `#[repr(u32)]`
discriminant on every variant. These codes are part of the contract's
on-chain ABI: off-chain clients and existing deployments depend on a given
number meaning a given error forever.

Rules:

- **Never reuse a code.** If a variant is removed, its number is retired,
  not recycled.
- **Never renumber a shipped variant.** Changing `ContractPaused = 9` to a
  different number silently breaks every caller that matches on `#9`.
- **New errors get the next free code.** The enum currently goes up to
  `MigrationVersionMismatch = 13`, so the next new variant must be `= 14`.

```rust
// in `enum RouterError`
SomethingNew = 14, // <- next free code
```

Add `///` doc on the new variant describing exactly when it is raised.

## Event topics must fit `symbol_short!` (<= 9 characters)

Events are published with a `symbol_short!` topic. `symbol_short!` only
accepts symbols of **9 characters or fewer**; a longer literal will not
compile. Choose a short, abbreviated topic name.

Existing topics follow this convention:

- `pair_reg` — `register_pair`
- `fee_set` — `set_pair_fee_bps`
- `adm_prop` — `propose_admin_transfer`
- `route` — `compute_route_fee`

Other live examples: `init`, `paused`, `adm_set`, `liq_set`, `unreg`. When
in doubt, abbreviate (`liq_set`, not `liquidity_set`).

## Admin-auth pattern

Every admin-gated entrypoint must start its body with:

```rust
Self::require_admin(&env);
```

`require_admin` loads `DataKey::Admin` from persistent storage, panics with
`RouterError::NotInitialized` (`#2`) if it is absent, then calls
`admin.require_auth()` and returns the admin address. Do **not** re-implement
the load-unwrap-require_auth block inline — always call the helper so auth
behaviour stays uniform and the helper never leaks into the generated client
ABI (it is private).

## Pause-gate pattern

State-changing entrypoints that should be blocked while the router is paused
must check the pause flag **before** doing any work and panic with
`RouterError::ContractPaused` (`#9`):

```rust
if env
    .storage()
    .persistent()
    .get(&DataKey::Paused)
    .unwrap_or(false)
{
    panic_with_error!(&env, RouterError::ContractPaused);
}
Self::require_admin(&env);
```

See `register_pair` and `set_pair_fee_bps` for the canonical placement
(pause check first, then `require_admin`).

## Storage tiers and TTL

- **Persistent storage** holds the admin address and all per-pair
  configuration (registration, fee bps, min/max amounts, liquidity, last
  route timestamp, schema version, fee recipient). These change rarely and
  must survive the contract's instance TTL window.
- **Instance storage** is reserved for hot configuration that every
  invocation is expected to touch. There is none today; do not move
  per-pair data into instance storage.
- **Bump the TTL on new slots when you write them.** Adding a new
  persistent slot means it can expire; extend its TTL on write so it lives
  as long as the rest of the contract state. Mirror the storage tier choice
  documented on the `DataKey` enum in `src/lib.rs`.

When adding a new storage key, add it to the `DataKey` enum with a `///`
comment explaining its tier rationale, matching the existing entries.

## WASM size budget

The deployable artifact (`cargo build --target wasm32v1-none --release`,
producing `target/wasm32v1-none/release/stableroute_contracts.wasm`) has a
hard size budget enforced by the `wasm-size` CI job. Artifact size is
ledger-relevant on Stellar — upload fees and ledger footprint scale with
the WASM byte size — which is why the release profile already tunes for
size (`lto`, `codegen-units = 1`, `strip = "symbols"`).

How it works:

- The budget lives in `.github/wasm-size-budget`: a single integer, the
  maximum allowed artifact size in **bytes**. The check is inclusive — an
  artifact exactly at the budget passes.
- On every push and PR, `scripts/check_wasm_size.sh` builds the
  `wasm32v1-none` release artifact, records its byte size, and fails the
  `wasm-size` job when the size exceeds the budget.
- On PRs the script also builds the base branch in a throwaway worktree
  and prints the size delta (bytes and percent). The delta is
  informational; only the budget gates the job.

Current baseline: **64,576 bytes** (measured at the commit that introduced
the check, 2026-07-19, rustc 1.97.1). The budget is set to **66,560
bytes** (64 KiB rounded up from the baseline, plus 1 KiB), giving roughly
3 percent headroom for ordinary changes.

Run the check locally before pushing:

```bash
rustup target add wasm32v1-none
bash scripts/check_wasm_size.sh              # budget check only
BASE_REF=main bash scripts/check_wasm_size.sh  # also print delta vs main
bash scripts/check_wasm_size_test.sh         # self-tests for the script
```

### Re-baselining procedure

Treat a budget bump like an error-code change: deliberate, visible, and
justified. When a change legitimately needs more room:

1. Build and measure locally:
   `cargo build --target wasm32v1-none --release`, then
   `wc -c < target/wasm32v1-none/release/stableroute_contracts.wasm`.
2. Confirm the growth is essential — check that the release profile is
   untouched and the new code cannot be expressed more compactly first.
3. Update `.github/wasm-size-budget` **in the same PR** as the change that
   needs it. Set the new budget to the measured size rounded up to the
   next KiB, plus one KiB of headroom.
4. Update the baseline figures in this section (size, date, rustc
   version).
5. Explain in the PR description why the growth is warranted; reviewers
   should reject drive-by budget bumps.

Never raise the budget in a separate "fix CI" PR — the bump must ride with
the code that consumes it so the justification is reviewable.

## Local workflow

Run these before opening a PR (they mirror CI):

```bash
cargo fmt --all -- --check
cargo build
cargo test
bash scripts/check_wasm_size.sh
```

`cargo fmt --all` will auto-fix formatting; the `--check` form only reports.
The full CI matrix (clippy, WASM build, size budget, coverage) is listed in
the README.

## PR checklist

Before requesting review, confirm:

- [ ] Tests added for new behaviour (happy path **and** error paths).
- [ ] NatSpec-style `///` doc comments on every new public entrypoint.
- [ ] No error codes renumbered or reused; new errors use the next free code.
- [ ] Events asserted in tests where an entrypoint publishes one.
- [ ] Docs updated (this file and/or the README) when conventions change.
- [ ] `cargo fmt --all -- --check`, `cargo build`, and `cargo test` all pass.
- [ ] `bash scripts/check_wasm_size.sh` passes; if the budget had to be
      raised, the re-baselining procedure above was followed in this PR.
