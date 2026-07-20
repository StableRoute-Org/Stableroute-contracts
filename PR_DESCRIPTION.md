# Expose protocol limits via `get_limits` (issue #196)

## Summary

`MAX_FEE_BPS`, `BPS_DENOMINATOR`, `MAX_BATCH_SIZE`, and `MAX_COOLDOWN_SECS`
were Rust `pub const`s only. An on-chain caller — or a client that did not
compile against this crate — had no way to discover the limits it must respect
before submitting a transaction.

This PR adds an on-chain discovery surface:

- A new `#[contracttype] struct RouterLimits` exposing the four constants.
- A new auth-free, read-only entrypoint `get_limits` that returns it.
- Tests asserting the returned values equal the compile-time constants.
- Documentation in `README.md` and `docs/abi.md`.

## Changes

### `src/lib.rs`

- **`RouterLimits` struct** (new `#[contracttype]`)
  Stable field order, documented as part of the on-chain ABI:

  | Field | Type | Constant |
  |-------|------|----------|
  | `max_fee_bps` | `u32` | `MAX_FEE_BPS` |
  | `bps_denominator` | `i128` | `BPS_DENOMINATOR` |
  | `max_batch_size` | `u32` | `MAX_BATCH_SIZE` |
  | `max_cooldown_secs` | `u64` | `MAX_COOLDOWN_SECS` |

  New limits must be **appended** — the order must not be reordered or inserted
  into, as that would change the XDR encoding.

- **`StableRouteRouter::get_limits`** (new entrypoint)
  Auth: none. Returns a `RouterLimits` snapshot mirroring the `pub const`s.
  It never touches storage, so it works even on an uninitialized contract
  (no `Admin` slot required).

### `docs/abi.md`

- Added `get_limits` to the **Lifecycle** entrypoint table.
- Added a new **Protocol limits (`RouterLimits`)** section documenting the
  struct fields, their constants/values, and which config setter enforces each
  bound.

### `README.md`

- Added a **Protocol limits** section summarizing the four bounds and pointing
  readers to `get_limits` for on-chain discovery.

## Tests (`src/lib.rs`, `mod test_i196_get_limits`)

All new tests are in the dedicated `test_i196_get_limits` module:

- `test_get_limits_matches_constants` — returned struct equals each `pub const`.
- `test_get_limits_hardcoded_values` — asserts the concrete values
  (`1_000` / `10_000` / `100` / `2_592_000`) so a silent constant change is
  caught, not just drift between the struct and the constants.
- `test_get_limits_struct_is_consistent_with_manual_build` — struct can be
  rebuilt identically from the constants (guards against a dropped/reordered
  field).
- `test_get_limits_works_when_uninitialized` — read-only discovery works with
  no admin set (no auth, no storage dependency).
- `test_get_limits_are_the_enforced_bounds` — ties the discovered limits to the
  real enforcement paths: a fee at `max_fee_bps` and a cooldown at
  `max_cooldown_secs` are accepted.
- `test_get_limits_batch_size_is_enforced_cap` — a batch of
  `max_batch_size + 1` is rejected with `BatchTooLarge` (#18), proving the
  returned `max_batch_size` is the actual enforcement cap.

## Verification

```
cargo fmt --all -- --check   # passes
cargo build                  # passes
cargo test                   # all get_limits tests pass
```

> Note: the unrelated pre-existing test
> `test::test_reregister_after_unregister_restores_pair_and_preserves_fee`
> was already failing on `main` before this change (verified via `git stash`);
> it is outside the scope of issue #196 and is not modified here.

closes #196
