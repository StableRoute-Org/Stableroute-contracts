## Summary

Adds missing configuration events to `set_pair_min_amount`, `set_pair_max_amount`, and `set_fee_recipient`, bringing all state-changing setters to parity. Off-chain indexers can now track historical changes to per-pair bounds and the fee recipient address. Closes #160.

## Changes

### `src/lib.rs` — Event emission (3 setters)

Three entrypoints that previously mutated persistent state silently now emit a `symbol_short!` event:

#### `set_pair_min_amount` → `min_set`
- Topic: `min_set` (7 chars — within the ≤9 limit)
- Payload: `(source, destination, min_amount): (Symbol, Symbol, i128)`
- Matches the existing `fee_set`/`liq_set`/`cd_set` per-pair tuple convention

#### `set_pair_max_amount` → `max_set`
- Topic: `max_set` (7 chars — within the ≤9 limit)
- Payload: `(source, destination, max_amount): (Symbol, Symbol, i128)`
- Matches the same per-pair tuple convention

#### `set_fee_recipient` → `recip_set`
- Topic: `recip_set` (9 chars — within the ≤9 limit)
- Payload: `recipient: Address`
- Follows the singleton-event pattern (cf. `orac_set(address)`, `maxfee(i128)`, `minfee(i128)`)

### Doc comments

Each function's `///` doc comment now mentions the emitted event by name:
- *"Emits a `min_set` event carrying the pair and the new floor."*
- *"Emits a `max_set` event carrying the pair and the new ceiling."*
- *"Emits a `recip_set` event carrying the new recipient address."*

### Tests (`src/lib.rs`)

Extended `test_pair_lifecycle_events_have_exact_payloads_and_counts` with three new assertion blocks, one per new event:

| New event | Assertions |
|-----------|-----------|
| `min_set` | Count = 1, payload decodes to `(Symbol, Symbol, i128)`, matches the input `(USDC, EURC, 50)` |
| `max_set` | Count = 1, payload decodes to `(Symbol, Symbol, i128)`, matches the input `(USDC, EURC, 10_000)` |
| `recip_set` | Count = 1, payload decodes to `Address`, matches the `admin` address passed as recipient |

### `docs/abi.md` — Event documentation

- **Fees table:** `set_fee_recipient` Event column changed from `—` to `recip_set(recipient)`
- **Bounds & liquidity table:** `set_pair_min_amount` Event changed from `—` to `min_set(source, destination, min_amount)`; `set_pair_max_amount` Event changed from `—` to `max_set(source, destination, max_amount)`
- **Event catalog:** Added three new rows — `min_set`, `max_set`, `recip_set` — with their payload types and emitter entrypoints

## Validation

All CI checks pass locally:

| Check | Result |
|-------|--------|
| `cargo fmt --all -- --check` | ✅ Pass |
| `cargo build` | ✅ Pass |
| `cargo clippy --all-targets -- -D warnings` | ✅ Pass |
| `cargo test` (234 tests) | ✅ All pass |
| `cargo llvm-cov --all-targets --fail-under-lines 95` | ✅ ≥95% |

No validation logic, return values, error codes, or storage slots were changed — only event emissions and their documentation.

## Related

Closes #160 — Emit a config event from `set_pair_min_amount`, `set_pair_max_amount`, and `set_fee_recipient`.
