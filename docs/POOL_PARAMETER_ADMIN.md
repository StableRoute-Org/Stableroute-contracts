# Bounded pool-parameter administration

StableRoute stores routing configuration per directional pair. Before this
change, an operator who wanted to change a pair's fee and amount limits had to
issue several independent calls. That made a configuration rollout harder to
audit and allowed an intermediate snapshot with a new fee but old limits, or
new limits but an old fee.

The `set_pair_parameters` entrypoint provides one admin-guarded transaction for
the fee and amount-limit set. `get_pair_parameters` provides the matching
read-only snapshot. Existing individual setters and the existing `PairInfo`
and `PairInfoExt` reads remain available for ABI compatibility.

## API

```rust
pub struct PairParameters {
    pub fee_bps: u32,
    pub min_amount: i128,
    pub max_amount: i128,
}

pub fn get_pair_parameters(
    env: Env,
    source: Symbol,
    destination: Symbol,
) -> PairParameters;

pub fn set_pair_parameters(
    env: Env,
    source: Symbol,
    destination: Symbol,
    parameters: PairParameters,
);
```

`PairParameters` is a Soroban contract type. Field order is part of its wire
encoding and must remain stable. New fields should be appended only after a
versioning review. The aggregate is a view over the existing storage keys; it
does not introduce a second source of truth.

## Validation

The combined setter validates the complete candidate before it writes any
slot:

| Field | Accepted values | Failure |
| --- | --- | --- |
| `fee_bps` | `0..=MAX_FEE_BPS` | `InvalidParameterRange` |
| `min_amount` | `0..=max_amount` | `InvalidParameterRange` |
| `max_amount` | `1..=i128::MAX` | `InvalidParameterRange` |
| pair | already registered | `PairNotRegistered` |
| caller | current admin | host authorization failure |
| router | not paused | `ContractPaused` |

The minimum is allowed to be zero, which preserves the existing “no lower
bound” sentinel. The maximum must be positive, which is consistent with the
individual maximum setter. A minimum greater than the maximum is rejected as a
single typed range error rather than leaving the pair in an impossible state.

The fee is bounded by the same public `MAX_FEE_BPS` constant used by the
individual fee setter. There is no separate aggregate limit that could drift
from the single-field API.

Validation order is deliberate:

1. pause guard;
2. admin authorization;
3. value bounds and cross-field range;
4. pair-registration guard;
5. old snapshot read;
6. three writes and TTL renewal;
7. one event.

No storage mutation or success event occurs before all input and state checks
pass. Soroban transaction atomicity additionally rolls back the whole call if
the host rejects a later operation.

## Authorization and pause policy

The operation uses the same `require_admin` guard as the existing fee setter.
It does not accept the liquidity oracle, because the oracle is scoped to
liquidity updates only. A read of the aggregate is auth-free, like
`get_pair_info` and the individual getters.

The combined operation uses `require_not_paused` because it changes the
relative fee used by route accounting. This matches `set_pair_fee_bps` and
prevents an operator from sneaking a fee change through a paused operational
state. Existing amount-bound setters retain their existing pause behavior; the
new aggregate has the stricter fee-compatible guard for the whole snapshot.

The pair must already be registered. Registration is the lifecycle boundary
for live routes. Configuration for an unknown pair is rejected instead of
creating dormant storage that looks configured but cannot route.

## Storage behavior

The aggregate writes these existing keys:

| Parameter | Storage key |
| --- | --- |
| fee | `DataKey::PairFeeBps(source, destination)` |
| minimum | `DataKey::PairMinAmount(source, destination)` |
| maximum | `DataKey::PairMaxAmount(source, destination)` |

Each key receives the same TTL renewal used by its individual setter. The
aggregate therefore follows the current persistent-storage retention policy
without adding an aggregate key that could expire independently from its
fields.

Unregister behavior is unchanged. The three amount/configuration fields are
cleared by the existing pair cleanup policy, while historical route metrics
remain governed by the separate metrics policy. Re-registering a pair returns
the normal defaults for these live parameters.

## Read semantics

`get_pair_parameters` returns a snapshot using the established sentinels:

```text
fee_bps    = 0
min_amount = 0
max_amount = i128::MAX
```

These values mean “free relative fee”, “no minimum”, and “no effective
maximum”. The getter is intentionally not restricted to registered pairs so
dashboards can inspect a candidate pair without first making a configuration
transaction. The setter remains registration-first.

The getter does not include liquidity, cooldown, or routing metrics. Those
values have different writers and lifecycles and remain available from
`PairInfoExt`. Keeping the aggregate focused means fee/amount configuration can
be updated without implying ownership of operational telemetry.

## Event contract

Each successful update emits exactly one event:

```text
topic: param_set
data:  (source, destination, old_parameters, new_parameters)
```

The topic is nine characters or fewer to remain valid for Soroban's short
symbol encoding. The old snapshot is captured immediately before the writes;
the new snapshot is the validated call input. Indexers can reconstruct the
configuration transition without issuing a follow-up RPC read or diffing
several independent events.

The event is not emitted for invalid input, an unregistered pair, an
unauthorized caller, or a paused-router rejection. A failed transaction's
diagnostic host events are not part of the public configuration event stream.

The individual setters continue to emit their existing `fee_set`, `min_set`,
and `max_set` events. Clients that use the combined setter should consume
`param_set` as the authoritative transition event for that call and should not
expect three duplicate single-field events.

## Error taxonomy

`RouterError::InvalidParameterRange` is appended as code `23`. Existing error
codes are not renumbered. The error catalog includes the new code, symbolic
name, category, retry recommendation, and configuration-fix flag.

The error is an input/configuration error:

- it is not retryable without changing the values;
- it is normally resolvable by an administrator correcting the range;
- it does not reveal storage or host internals;
- it remains distinguishable from `AmountMustBePositive`, which is used by
  route amounts and individual setters.

The catalog mapping is exhaustive and round-trips code `23` back to the typed
enum. This keeps SDKs that consume `get_error_catalog` synchronized with the
contract's new failure path.

## Why an aggregate instead of changing existing setters?

Changing the signatures of the existing setters would break generated clients
and deployed callers. Adding a new entrypoint preserves those APIs while
offering operators an atomic path for coordinated changes.

The aggregate also makes intent explicit. A call containing all three values
can be reviewed, signed, indexed, and reproduced as one policy change. The
existing setters remain useful when an operator intentionally changes one
field and wants the established single-field event.

## Atomicity examples

### Valid update

```text
old: { fee_bps: 0, min_amount: 0, max_amount: i128::MAX }
new: { fee_bps: 125, min_amount: 100, max_amount: 50_000 }
result: three slots updated, one param_set event
```

### Invalid cross-field range

```text
request: { fee_bps: 60, min_amount: 501, max_amount: 500 }
result: InvalidParameterRange, old snapshot remains unchanged
```

### Invalid fee bound

```text
request: { fee_bps: MAX_FEE_BPS + 1, min_amount: 0, max_amount: 100 }
result: InvalidParameterRange, no writes, no param_set event
```

### Unregistered pair

```text
request: valid values for an unknown pair
result: PairNotRegistered, no configuration storage is created
```

## Test coverage

`src/pool_parameters_tests.rs` exercises the contract boundary with the
Soroban test host:

- an admin can update all three values;
- the read view reflects the update;
- default values are returned before configuration;
- old and new snapshots are present in the event payload;
- fee-overflow, negative minimum, zero maximum, and inverted ranges fail;
- a failed update leaves a prior valid snapshot intact;
- an unregistered pair fails;
- a caller without admin authorization fails;
- a paused router rejects the update.

The event test decodes the payload into `(Symbol, Symbol, PairParameters,
PairParameters)` rather than only checking that some event exists. This guards
the ABI shape and the old/new ordering that indexers rely on.

The error-taxonomy tests additionally verify contiguous append-only codes,
catalog length, descriptor metadata, operation mapping, and code round trips.
The existing 331-test suite remains green with the new module included.

## Review checklist

- Confirm the caller is the operational admin, not the liquidity oracle.
- Confirm the pair is registered before signing the transaction.
- Confirm `min_amount <= max_amount` and both values fit the intended asset
  precision.
- Confirm the fee is expressed in basis points, not a percentage integer.
- Confirm the `param_set` event is captured and decoded by the indexer.
- Confirm the emitted old snapshot matches the dashboard's last known value.
- Confirm a failed submission did not leave a partially updated local cache.
- Confirm pause state is considered before sending a configuration transaction.

## Client migration guidance

Clients can adopt the aggregate view without adopting the setter immediately:

1. call `get_pair_parameters` to populate one configuration card;
2. display the three fields with the established units and sentinels;
3. when saving all fields, call `set_pair_parameters` once;
4. listen for `param_set` and replace the local snapshot from its new payload;
5. fall back to the individual getters for older deployments that do not yet
   expose the new method.

Do not infer `max_amount = 0` as “unbounded”; zero is invalid for the setter.
Use `i128::MAX` as the established unbounded read sentinel.

## Operational notes

The setter does not alter liquidity, cooldown, route counters, or cumulative
volume. Changing a fee or amount bound does not retroactively rewrite route
history. A route already committed before the update remains part of history;
subsequent route validation reads the new live snapshot.

The contract does not custody funds in this operation. The setter only changes
configuration used by fee computation and amount checks. Operators should
still coordinate fee changes with the off-chain quoting service so a quote
cache does not lag the emitted event.

## Verification commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The focused development command is:

```bash
cargo test pool_parameters
```

The complete test command is the merge gate because `RouterError` and the
generated contract client are shared by every router test module.
