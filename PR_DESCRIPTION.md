## Summary

Fixes documentation drift in `src/lib.rs` where `DataKey` doc comments no longer reflected the actual contract implementation. Closes #159.

## Changes

### `src/lib.rs` — `DataKey` enum documentation

Three categories of doc-comment correction, all **comment-only** (no logic, events, or errors changed):

#### 1. Enum-level doc block (lines 84–99)

The enum-level `///` comment previously claimed "no instance storage variants (none yet)". Updated to accurately describe the current layout:

- **Twenty-one variants** total (was stale at an older count).
- **Three hot-global singletons** now live in **instance storage**: `Admin`, `PendingAdmin`, `Paused`. Bundling them with the contract instance avoids a separate persistent-storage read on every admin-gated or pause-gated call.
- Sentinel conventions now list `min fee absolute` alongside the existing `max fee absolute`.

#### 2. `DataKey::Admin` (line 117)

Was documented as "set once at init". Updated to:

> Set once by `__constructor`; only changed by a two-step handover (`propose_admin_transfer` → `accept_admin_transfer`).

This reflects the migration from a standalone `init()` entrypoint (which now unconditionally panics with `AlreadyInitialized`) to the Soroban constructor pattern (`__constructor`), which atomically sets the admin at deploy time and closes the init front-running window.

#### 3. `DataKey::Paused` (lines 127–141)

The comment previously claimed that "No write entrypoint accepts calls until an unpause," which was inaccurate — numerous admin/config entrypoints intentionally remain available so the admin can recover during a pause.

Updated to enumerate the exact pause-gating boundaries:

- **Rejected while paused** (call `require_not_paused`): `compute_route_fee`, `register_pair`, `register_pairs`, `set_pair_fee_bps`, `set_pair_fees_bps`.
- **Available while paused** (no pause check): all admin/config setters (`set_pair_liquidity`, `set_pair_cooldown`, `set_pair_min_amount`, `set_pair_max_amount`, `set_fee_recipient`, `set_max_fee_absolute`, `set_min_fee_absolute`, `set_oracle`, `remove_oracle`), lifecycle entrypoints (`unregister_pair`, `purge_pair_metrics`), governance (`pause`, `unpause`, `set_timelock`, `propose_admin_transfer`, `cancel_admin_transfer`, `force_admin_transfer`, `accept_admin_transfer`), migration (`migrate_v1_to_v2`), and read-only queries (`quote_route`, getters, `version`).

### `docs/storage.md` — Storage tier documentation

The storage reference now correctly documents the two-tier layout:

- **Instance storage**: `Admin`, `PendingAdmin`, `Paused` — hot globals read on every admin/pause call.
- **Persistent storage**: every other `DataKey` slot.

Added the `MinFeeAbsolute` row to the DataKey table, and updated `Admin`, `PendingAdmin`, and `Paused` tier columns from "persistent" to "**instance**".

## Validation

All existing tests pass, confirming no behavioural changes:

- ✅ `cargo fmt --all -- --check` — no formatting drift
- ✅ `cargo build` — compiles cleanly
- ✅ `cargo clippy --all-targets -- -D warnings` — no new lints
- ✅ `cargo test` — all tests green
- ✅ `cargo build --target wasm32v1-none --release` — deployable WASM artifact
- ✅ `bash scripts/check_wasm_size.sh` — within size budget
- ✅ `cargo llvm-cov --all-targets --fail-under-lines 95` — ≥95% line coverage

## Related

Closes #159 — Replace the constructor-only init path: documentation drift in the `DataKey` and `Paused` comments.
