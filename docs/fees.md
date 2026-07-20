# StableRoute — Fee Model & Computation

Authoritative reference for the router's fee arithmetic
([`src/lib.rs`](../src/lib.rs)). Covers basis-point semantics, the
relative/absolute cap composition, integer truncation, and representative
worked examples.

## Fee formula

A single route is charged a fee computed from the routed `amount` (in source
units) and the stored per-pair `fee_bps`:

```
proportional_fee = amount * fee_bps / BPS_DENOMINATOR
final_fee        = min(proportional_fee, max_fee_absolute)   // if cap is set
```

| Constant | Value | Location | Meaning |
|----------|-------|----------|---------|
| `MAX_FEE_BPS` | `1_000` (10 %) | `src/lib.rs:123` | Hard upper bound on a per-pair `fee_bps` value |
| `BPS_DENOMINATOR` | `10_000` | `src/lib.rs:125` | Basis-point divisor: 1 bps = 1 / 10 000 |

## Relative cap: `MAX_FEE_BPS`

Every per-pair `fee_bps` is bounded by `MAX_FEE_BPS` when set. The admin
entrypoint `set_pair_fee_bps` rejects any value above `MAX_FEE_BPS` with
error `FeeBpsTooHigh` (#4). An unset pair defaults to `0` bps (free).

This relative cap alone would let large amounts produce arbitrarily large
fees, which is why the absolute cap exists.

## Absolute cap: `MaxFeeAbsolute`

The admin can configure a singleton absolute ceiling (`set_max_fee_absolute`,
stored under `DataKey::MaxFeeAbsolute`). When set, `apply_fee_cap`
(`src/lib.rs:708`) clamps every fee to this value:

```rust
fn apply_fee_cap(env: &Env, fee: i128) -> i128 {
    match env.storage().persistent().get::<_, i128>(&DataKey::MaxFeeAbsolute) {
        Some(cap) => fee.min(cap),
        None => fee,
    }
}
```

- **Unset** — the proportional fee is used unchanged.
- **Set to `C`** — every fee is `min(proportional_fee, C)`.
- **Set to `0`** — every route is free (cap of zero).

Setting a negative cap is rejected with `AmountMustBePositive` (#6).

### Precedence

Both caps compose. A route is charged:

```
final_fee = min(amount * fee_bps / 10_000, max_fee_absolute)
```

The _tighter_ of the two wins. This means a low `max_fee_absolute` can
override a high `fee_bps`, and a low `fee_bps` already bound the fee below
the absolute cap.

## Arithmetic & truncation

Fee computation uses checked integer arithmetic:

```rust
let fee = amount
    .checked_mul(fee_bps as i128)
    .map(|n| n / BPS_DENOMINATOR)
    .unwrap_or(0);
```

1. **`checked_mul`** — returns `None` on overflow, which is mapped to `0`.
   This is a last-resort safety net; in practice amounts stay well below
   `i128::MAX / MAX_FEE_BPS`.
2. **Truncating division** — Rust's integer division rounds toward zero.
   For the fee domain (non-negative values) this is a **floor** truncation.
3. **Zero-bps shortcut** — when `fee_bps = 0`, the computation short-circuits
   to `0` naturally.

### Small-amount truncation

Because `fee_bps` is at most `1_000` and the divisor is `10_000`, any
`amount < 10` yields a fee of `0` for all non-zero fee rates. Concretely:

| amount | fee_bps | `amount * fee_bps / 10_000` | truncates to |
|-------|---------|-----------------------------|-------------|
| 9 | 1 000 | `9 * 1 000 / 10 000 = 0.9` | **0** |
| 1 | 500 | `1 * 500 / 10 000 = 0.05` | **0** |

This means very small amounts always route fee-free. Integrators expecting a
minimum per-route fee should use `set_pair_min_amount` to enforce a floor on
routed amounts.

## Worked examples

The table below shows `final_fee` for various amounts and fee rates, both
with and without an absolute cap.

Assumptions: `BPS_DENOMINATOR = 10_000`, `MAX_FEE_BPS = 1_000`.

| amount | fee_bps | proportional fee | absolute cap | final fee | notes |
|--------|---------|-----------------|--------------|-----------|-------|
| 1 000 000 | 50 | 5 000 | unset | 5 000 | baseline fee |
| 1 000 000 | 50 | 5 000 | 1 000 | 1 000 | cap bites |
| 1 000 000 | 50 | 5 000 | 10 000 | 5 000 | proportional is tighter |
| 1 000 000 | 0 | 0 | unset | **0** | zero bps → free |
| 100 000 | 1 000 | 10 000 | unset | 10 000 | at max rate |
| 100 000 | 1 000 | 10 000 | 5 000 | 5 000 | cap under max rate |
| 42 | 250 | 1 | unset | 1 | smallest non-zero fee |
| 9 | 1 000 | 0 | unset | **0** | truncation to zero |
| 1 | 500 | 0 | unset | **0** | truncation to zero |
| 0 | 50 | — | — | — | rejected (#6) |
| i128::MAX | 1 | overflow → 0 | unset | **0** | overflow safety net |

## Key entrypoints

| Entrypoint | Signature | Returns | Doc reference |
|-----------|-----------|---------|--------------|
| `quote_route` | `(source, destination, amount)` → `(fee, net)` | Read-only quote | `docs/abi.md` |
| `compute_route_fee` | `(source, destination, amount)` → `fee` | Mutating route + fee | `docs/abi.md` |
| `set_pair_fee_bps` | `(source, destination, fee_bps)` | — | `docs/abi.md` |
| `set_max_fee_absolute` | `(max_fee)` | — | `docs/abi.md` |
| `get_max_fee_absolute` | — | `Option<i128>` | `docs/abi.md` |

## Test coverage

Fee arithmetic is covered by:

- **Property tests** (`proptest`) — invariant checks across randomised
  inputs in `src/lib.rs`:
  - `prop_fee_within_amount` — fee never exceeds amount, never negative
  - `prop_zero_fee_bps_is_free` — zero fee_bps ⇒ zero fee
  - `prop_quote_matches_compute` — `quote_route` and `compute_route_fee`
    agree on the fee
- **Unit tests** (`mod test`, `mod test_i16_fee_arithmetic`,
  `mod test_i41_fee_cap`) — explicit edge cases for overflow, truncation,
  cap clamping, zero cap, negative-cap rejection, and quote/compute parity.

Run all fee-related tests with:

```shell
cargo test fee
cargo test i16_fee_arithmetic
cargo test i41_fee_cap
```

## See also

- [`docs/storage.md`](storage.md) — fee-related `DataKey` entries
  (`PairFeeBps`, `MaxFeeAbsolute`, `FeeRecipient`, counters)
- [`docs/abi.md`](abi.md) — full entrypoint signatures and event payloads
- [`README.md`](../README.md) — error code reference, liquidity consumption
  model
