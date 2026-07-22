# Contributing to stableroute-contracts

Thanks for contributing to the StableRoute Soroban contracts. This guide
documents every convention the `StableRouteRouter` contract relies on. All
rules are enforced by the code in `src/lib.rs` and by CI, so following them
keeps reviews fast and avoids breaking on-chain compatibility.

Questions or want to coordinate? Join the StableRoute Discord:
<https://discord.gg/37aCpusvx>

---

- [Error codes are append-only](#error-codes-are-append-only)
- [Event topics must fit `symbol_short!` (≤ 9 characters)](#event-topics-must-fit-symbol_short--9-characters)
- [Admin-auth pattern](#admin-auth-pattern)
- [Pause-gate pattern](#pause-gate-pattern)
- [Non-reentrancy pattern](#non-reentrancy-pattern)
- [Oracle dual-auth pattern](#oracle-dual-auth-pattern)
- [Registration-first invariant](#registration-first-invariant)
- [Batch-operation conventions](#batch-operation-conventions)
- [Checks/effects ordering](#checkseffects-ordering)
- [Saturating math](#saturating-math)
- [Storage tiers and TTL](#storage-tiers-and-ttl)
- [Sentinel conventions](#sentinel-conventions)
- [Route side-effects summary](#route-side-effects-summary)
- [`quote_route` vs `compute_route_fee`](#quote_route-vs-compute_route_fee)
- [Local workflow](#local-workflow)
- [PR checklist](#pr-checklist)

---

## Error codes are append-only

`RouterError` is a `#[contracterror]` enum with an explicit `#[repr(u32)]`
discriminant on every variant. These codes are part of the contract's on-chain
ABI: off-chain clients and existing deployments depend on a given number
meaning a given error forever.

### Rules

- **Never reuse a code.** If a variant is removed, its number is retired, not
  recycled.
- **Never renumber a shipped variant.** Changing `ContractPaused = 9` to a
  different number silently breaks every caller that matches on `#9`.
- **New errors get the next free code.** The enum currently goes up to
  `CooldownTooLarge = 20`, so the next new variant must be `= 21`:

```rust
// in `enum RouterError`
SomethingNew = 21, // <- next free code
```

Add a `///` doc comment on the new variant describing exactly when it is
raised, matching the existing entries.

### Current error table

| Code | Variant | Description |
|------|---------|-------------|
| 1 | `AlreadyInitialized` | `init` called but admin already stored |
| 2 | `NotInitialized` | Admin slot absent |
| 3 | `SourceEqualsDestination` | `register_pair` with `source == destination` |
| 4 | `FeeBpsTooHigh` | Fee above `MAX_FEE_BPS` (1000) |
| 5 | `PairNotRegistered` | Unregistered pair used |
| 6 | `AmountMustBePositive` | Non-positive amount |
| 7 | `NoPendingAdminTransfer` | No pending admin to accept |
| 8 | `NotPendingAdmin` | Caller does not match pending admin |
| 9 | `ContractPaused` | State-changing call while paused |
| 10 | `AmountBelowMin` | Amount below `PairMinAmount` |
| 11 | `AmountAboveMax` | Amount above `PairMaxAmount` |
| 12 | `InsufficientLiquidity` | Amount exceeds reported liquidity |
| 13 | `MigrationVersionMismatch` | Non-v1 schema on migrate |
| 14 | `TimelockNotElapsed` | Admin handover before timelock expiry |
| 15 | `ReentrantCall` | Re-entrant call detected |
| 16 | `NotAuthorized` | Neither admin nor oracle |
| 17 | `RouteCooldownActive` | Per-pair cooldown not elapsed |
| 18 | `BatchTooLarge` | Batch exceeds `MAX_BATCH_SIZE` |
| 19 | `EmptyBatch` | Batch with zero entries |
| 20 | `CooldownTooLarge` | Cooldown exceeds `MAX_COOLDOWN_SECS` |

## Event topics must fit `symbol_short!` (≤ 9 characters)

Events are published with a `symbol_short!` topic. `symbol_short!` only accepts
symbols of **9 characters or fewer**; a longer literal will not compile. Choose
a short, abbreviated topic name.

### All event topics

| Topic | Entrypoint(s) | Payload |
|-------|---------------|---------|
| `init` | `__constructor` | `admin: Address` |
| `paused` | `pause`, `unpause` | `is_paused: bool` |
| `pair_reg` | `register_pair`, `register_pairs` | `(source, destination): (Symbol, Symbol)` |
| `fee_set` | `set_pair_fee_bps`, `set_pair_fees_bps` | `(source, destination, fee_bps): (Symbol, Symbol, u32)` |
| `liq_set` | `set_pair_liquidity` | `(source, destination, liquidity): (Symbol, Symbol, i128)` |
| `liq_used` | `compute_route_fee` | `(source, destination, remaining): (Symbol, Symbol, i128)` |
| `route` | `compute_route_fee` | `(source, destination, amount): (Symbol, Symbol, i128)` |
| `unreg` | `unregister_pair` | `(source, destination): (Symbol, Symbol)` |
| `cfg_clr` | `unregister_pair` | `(source, destination): (Symbol, Symbol)` |
| `queued` | `propose_admin_transfer` | `(new_admin, eta): (Address, u64)` |
| `executed` | `accept_admin_transfer`, `force_admin_transfer` | `admin: Address` |
| `cd_set` | `set_pair_cooldown` | `(source, destination, cooldown_secs): (Symbol, Symbol, u64)` |
| `maxfee` | `set_max_fee_absolute` | `max_fee: i128` |
| `orac_set` | `set_oracle` | `oracle: Address` |
| `orac_rm` | `remove_oracle` | `removed: Option<Address>` |
| `pair_mrst` | `purge_pair_metrics` | `(source, destination): (Symbol, Symbol)` |
| `upgraded` | `upgrade` | `new_wasm_hash: BytesN<32>` |

When adding a new topic, keep it short and match the existing style
(`liq_used`, not `liquidity_debited`).

## Admin-auth pattern

Every admin-gated entrypoint must call `Self::require_admin(&env)` before
doing any state-changing work:

```rust
Self::require_admin(&env);
// ... state changes ...
```

`require_admin` loads `DataKey::Admin` from persistent storage, panics with
`RouterError::NotInitialized` (`#2`) if absent, then calls
`admin.require_auth()` and returns the admin address.

**Do not** re-implement the load-unwrap-require_auth block inline — always
call the helper so auth behaviour stays uniform and the helper never leaks
into the generated client ABI (it is private).

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

The pause check comes first, then `require_admin`. See `register_pair` and
`set_pair_fee_bps` for the canonical ordering.

Entrypoints that are **not** pause-gated:
- `upgrade` — the admin must be able to deploy a fix even while paused
- Read-only entrypoints (`quote_route`, getters, `version`)
- `__constructor` — runs at deploy time before any pause state exists

## Non-reentrancy pattern

`compute_route_fee` uses a reentrancy lock (`DataKey::ReentrancyLock`) to
prevent a malicious nested call from operating on partially-applied effects.

The lock is acquired at the very start of the guarded entrypoint and released
on **every** exit path:

```rust
Self::enter_nonreentrant(&env);
// ... checks (each releasing the lock before panicking) ...
// ... effects ...
Self::exit_nonreentrant(&env);
```

**Rules:**
- Acquire the lock with `Self::enter_nonreentrant(&env)` immediately on entry.
- Release it with `Self::exit_nonreentrant(&env)` before **every** panic and
  on the success return.
- `enter_nonreentrant` itself panics with `ReentrantCall` (`#15`) if the lock
  is already held — no release is needed because this call never acquired it.
- The lock is stored in persistent storage as a `bool` (defaults to `false`).

## Oracle dual-auth pattern

`set_pair_liquidity` is dual-authorized: the caller must be **either** the
admin **or** the configured oracle:

```rust
caller.require_auth();
let admin = env.storage().persistent().get(&DataKey::Admin)
    .unwrap_or_else(|| panic_with_error!(...));
let oracle: Option<Address> = env.storage().persistent().get(&DataKey::Oracle);
if caller != admin && Some(caller.clone()) != oracle {
    panic_with_error!(... RouterError::NotAuthorized);
}
```

**Rules:**
- The oracle role is strictly scoped: it can only call `set_pair_liquidity`.
- When no oracle is configured, `Some(caller) != None` is always true, so
  only the admin is accepted — the slot degrades cleanly to admin-only.
- Use `remove_oracle` to revoke the oracle key (the recovery path).
- The oracle can be set or rotated via `set_oracle` (admin-gated).

## Registration-first invariant

Every per-pair config setter (`set_pair_fee_bps`, `set_pair_min_amount`,
`set_pair_max_amount`, `set_pair_liquidity`) validates that the pair was
previously registered via `register_pair`. Use the shared helper:

```rust
Self::require_pair_registered(&env, &source, &destination);
```

This prevents creating orphan storage slots for corridors that were never
(or no longer) registered. The helper panics with `PairNotRegistered` (`#5`).

The check happens **after** admin/oracle auth validation so that the auth
error (if any) is raised first, and the `PairNotRegistered` error only
appears when an authorized caller provides an unregistered pair.

## Batch-operation conventions

`register_pairs` and `set_pair_fees_bps` accept multiple entries in a single
call. Both follow the same rules:

```rust
if pairs.is_empty() {
    panic_with_error!(&env, RouterError::EmptyBatch);
}
if pairs.len() > MAX_BATCH_SIZE {
    panic_with_error!(&env, RouterError::BatchTooLarge);
}
```

**Rules:**
- Reject an empty batch with `EmptyBatch` (`#19`).
- Reject a batch exceeding `MAX_BATCH_SIZE` (100) entries with
  `BatchTooLarge` (`#18`).
- Validate every entry before writing any — Soroban transactions are atomic,
  so a single invalid entry rolls back the entire batch.
- Each valid entry gets its own event (e.g. `pair_reg` or `fee_set`).

## Checks/effects ordering

`compute_route_fee` demonstrates the canonical ordering:

1. **Checks** — all validation (paused, amount, registration, bounds,
   liquidity, cooldown) runs before any storage write or event emission.
2. **Lock** — the reentrancy lock is acquired immediately on entry.
3. **Effects** — liquidity debit, counter increments, timestamp stamp, events.

This ensures that a rejected route leaves no state trail behind. New
state-changing entrypoints should follow the same pattern: validate first,
mutate second.

## Saturating math

All counters and accumulators use saturating arithmetic so that overflow can
never panic the contract:

```rust
env.storage().persistent().set(
    &DataKey::TotalRoutesAllTime,
    &total.saturating_add(1),
);
```

**Slots using saturating math:**
- `TotalRoutesAllTime` — `saturating_add(1)`
- `PairRouteCount` — `saturating_add(1)`
- `PairVolume` — `saturating_add(amount)`
- `PairLiquidity` — `saturating_sub(amount)`

Fee computation uses `checked_mul` with a fallback:

```rust
let fee = amount
    .checked_mul(fee_bps as i128)
    .map(|n| n / BPS_DENOMINATOR)
    .unwrap_or(0);
```

This avoids panics on extreme amounts near `i128::MAX`.

## Storage tiers and TTL

All `DataKey` slots live in **persistent** storage. See
[`docs/storage.md`](docs/storage.md) for the authoritative reference
covering every slot's key shape, value type, default-when-absent,
reader/writer entrypoints, and TTL classification.

### TTL classes

| Class | Description | Write frequency |
|-------|-------------|-----------------|
| **Static** | Written once at construction or migration | Once |
| **Config** | Admin-gated governance/config writes | Rare |
| **Hot** | Written on every `compute_route_fee` call | Every route |

**Rules:**
- Add every new slot to the `DataKey` enum with a `///` comment explaining
  its tier rationale.
- Bump the TTL on new slots when you write them so they cannot expire before
  the contract state they accompany.
- Update `docs/storage.md` when adding, removing, or changing a slot.

## Sentinel conventions

The contract uses consistent sentinel values for absent storage slots:

| Sentinel | Applies to |
|----------|------------|
| `false` | Absent `bool` (pair registration, paused, reentrancy lock) |
| `i128::MAX` | "Unbounded" for `PairMaxAmount` and liquidity inside `compute_route_fee` |
| `0` | Counters, fees, timestamps (`u64`), `PairMinAmount`, cooldowns |
| `None` | Absent `Option` (admin, pending admin, fee recipient, last-route timestamp, max fee absolute, oracle) |
| `1` | `SchemaVersion` when absent (implicit pre-migration default) |

When adding a new storage slot, document its sentinel in the `DataKey` enum
comment and (if applicable) update the list in `docs/storage.md`.

## Route side-effects summary

On every successful `compute_route_fee`, these persistent slots are written:

| Slot | Operation |
|------|-----------|
| `ReentrancyLock` | `true` at entry → `false` on exit |
| `TotalRoutesAllTime` | `saturating_add(1)` |
| `PairRouteCount` | `saturating_add(1)` |
| `PairVolume` | `saturating_add(amount)` |
| `PairLastRouteAt` | `env.ledger().timestamp()` |
| `PairLiquidity` | `saturating_sub(amount)` (only when set, i.e. ≠ `i128::MAX`) |

Additionally, `ReentrancyLock` is always written and cleared per-call, and
`apply_fee_cap` may clamp the returned fee but does not write state.

## `quote_route` vs `compute_route_fee`

| Aspect | `quote_route` | `compute_route_fee` |
|--------|---------------|---------------------|
| **State mutation** | None (read-only) | Writes counters, timestamps, liquidity |
| **Reentrancy lock** | No | Yes |
| **Events** | None | Emits `route` (and `liq_used` if liquidity is set) |
| **Pause gate** | No (available while paused) | Yes (rejects while paused) |
| **Fee cap** | Yes (`apply_fee_cap`) | Yes (`apply_fee_cap`) |
| **Return** | `(fee, net_amount)` | `fee` |

`quote_route` is the planner hook for off-chain integrators — it mirrors the
fee computation without the side-effects.

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

Current baseline: **68,477 bytes** (re-measured 2026-07-22, rustc 1.91.1,
after the per-pair route-counter/volume tracking, `get_limits`, and
absolute min-fee-floor features landed). The budget is set to **69,632
bytes** (67 KiB rounded up from the baseline, plus 1 KiB), giving roughly
2 percent headroom for ordinary changes.

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
cargo clippy --all-targets -- -D warnings
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
