# Optimistic swap versions

The router's existing `compute_route_fee` entrypoint records route accounting,
but callers previously had no compare-and-swap boundary. Issue #451 adds the
public `swap` operation and `get_swap_version` view. The new operation is
intended for clients that coordinate a route against a previously read pair
state.

## State model

Every `(source, destination)` pair has an independent monotonic version stored
under `DataKey::PairSwapVersion`. The absent value is version `0`. A successful
guarded swap changes exactly one pair's version from `v` to `v + 1`.

```text
initial pair state: version = 0
swap(expected = 0): version = 1
swap(expected = 1): version = 2
swap(expected = 0): VersionConflict(2)
```

The version is not a historical log, block number, timestamp, nonce for an
account, or global counter. It belongs to the pair state that the swap caller
read. Versions for two pairs may advance independently.

## Public API

```text
get_swap_version(source, destination) -> u64
swap(source, destination, amount, expected_version) -> SwapReceipt | SwapError
```

`SwapReceipt` contains:

| Field | Meaning |
| --- | --- |
| `fee` | Fee returned by the router's existing route accounting path |
| `version` | New pair version after the successful swap |

The caller should read the version immediately before constructing the
transaction. The expected version is part of the same invocation as route
accounting and the version write.

## Compare-and-swap sequence

The `swap` operation follows this order:

1. reject a paused router;
2. reject a non-positive amount;
3. reject an unregistered pair;
4. read the pair's current version;
5. compare it with `expected_version`;
6. return `VersionConflict(current_version)` on mismatch;
7. check that incrementing the version cannot overflow;
8. execute the existing fee, bound, liquidity, cooldown, and reentrancy path;
9. write the incremented pair version; and
10. emit the `swap_ver` transition event and return the receipt.

The route accounting path is invoked before the version write. If route
validation rejects the amount, pair, liquidity, or cooldown, the version write
and event are never reached. Soroban transaction atomicity rolls back route
accounting if the invocation fails.

## Version conflict

The conflict error is typed and carries the version observed by the contract:

```text
SwapError::VersionConflict(current_version)
```

It is not a string containing a debug representation, and it does not expose
storage keys or host internals. A client should use the payload to refresh its
state and decide whether the original swap is still valid. Blindly replacing
the expected value and resubmitting can apply stale business intent.

The version is returned only on the conflict path or in a successful receipt.
There is no partial version update on a failed operation. A conflict does not
increment a metric or emit a success event.

## Event

Successful swaps emit a `swap_ver` event with:

```text
(source, destination, previous_version, next_version)
```

The event is emitted after the persistent write. Indexers can use it to
reconcile the public read view and to detect missing or duplicated observations
without treating events as the source of truth. The source and destination
identify the pair; the two version values describe the transition.

There is no conflict event. A rejected stale write is visible in the invoking
transaction result and should be counted by the caller's observability layer.

## Pair isolation

The storage key includes both route symbols. A successful swap for `USDC/EURC`
does not change the version for `EURC/GBP`. A caller must not use one pair's
version as an optimistic token for another pair.

This isolation also applies to conflict handling. If `USDC/EURC` is at version
4 and `EURC/GBP` is at version 1, a stale `EURC/GBP` request reports 1, not 4.
The tests cover both the initial independent reads and a successful operation
on the second pair after the first pair has advanced.

## Existing accounting behavior

The router is an accounting-only placeholder and does not custody tokens or
coordinate cross-contract path payments. `swap` therefore delegates to
`compute_route_fee` for the established route checks and fee result. It does
not claim to move assets or to provide cross-contract atomicity.

The delegated path still enforces:

- pair registration;
- positive amount;
- pair minimum and maximum amounts;
- configured liquidity and its decrement;
- pair cooldown;
- fee basis-point and absolute cap/floor policy;
- the existing reentrancy lock; and
- the existing pause policy.

The new version is a coordination guard around that accounting operation. It
does not replace the existing route counters, last-route timestamp, liquidity
slot, or cooldown. Consumers that need the legacy accounting-only operation
may continue to call `compute_route_fee`.

## Pause behavior

`swap` checks the shared paused flag before its version read. While paused it
returns `SwapError::ContractPaused`, leaves the pair version unchanged, and
does not call route accounting. Read-only `get_swap_version` remains available
so an operator and clients can inspect state during an incident.

The pause mechanism remains admin-controlled and shared with the existing
router policy. `swap` does not introduce an unpause path or a second emergency
flag.

## Input failures

The new typed error surface is intentionally narrow:

| Error | Meaning | Version effect |
| --- | --- | --- |
| `ContractPaused` | Emergency stop is active | Unchanged |
| `AmountMustBePositive` | Amount is zero or negative | Unchanged |
| `PairNotRegistered` | Pair is not configured | Unchanged |
| `VersionConflict(current)` | Expected value is stale | Unchanged |
| `VersionOverflow` | Current version is `u64::MAX` | Unchanged |

After these typed preconditions pass, the existing route errors may reject
liquidity, amount bounds, cooldown, authorization, or reentrancy conditions.
Those failures also leave the version unchanged because the version write is
last and the invocation is atomic.

## Client workflow

A safe client workflow is:

1. read pair configuration and `get_swap_version`;
2. calculate the intended amount and confirm it is still acceptable;
3. construct `swap` with that exact version;
4. submit the transaction;
5. on success, record the returned next version and fee;
6. on conflict, refresh pair state and re-evaluate the business intent; and
7. on pause or route error, surface a non-retryable result to the caller.

The client must not retry a non-idempotent external action merely because a
transaction submission timed out. The router operation itself is guarded, but
an application may have side effects around the submission that are outside
the contract.

If a transaction is known to have failed before inclusion, rereading the
version is safe. If inclusion is unknown, reconcile the pair version and route
event before deciding whether to submit again.

## Same-base race

Suppose two callers read version `7`:

```text
caller A reads 7
caller B reads 7
caller A submits expected 7 -> success, version 8
caller B submits expected 7 -> VersionConflict(8)
```

The second call does not run fee accounting, decrement liquidity, update route
counters, or emit a `swap_ver` success event. This is the important difference
from a read-only version check performed off-chain: the compare and write are
inside one invocation.

The sequence is still subject to normal ledger transaction ordering. The guard
does not predict which transaction will be included first; it ensures that the
later transaction cannot silently apply the stale version it was built from.

## Overflow boundary

The version increment uses checked addition. If a pair reaches `u64::MAX`, a
further guarded swap returns `VersionOverflow` and leaves the value at the
maximum. The contract does not wrap to zero, because wrapping would allow a
very old expected version to become valid again.

This is a theoretical long-lived boundary for ordinary ledger traffic, but it
is included in the typed API so the behavior is stable and testable. Operators
should treat a version near the boundary as a migration concern rather than
waiting for the next call to fail.

## Storage and TTL

`PairSwapVersion` is a persistent pair-scoped value. It follows the same pair
identity as route registration and accounting. The initial absent value reads
as zero, so existing pairs can adopt the guarded API without a migration write.

The version is written only after the existing route path succeeds. A future
storage/TTL change must preserve the property that a version read and version
write use the same key and invocation. Splitting them across transactions
would reintroduce the race the guard is intended to prevent.

The version is not included in `PairInfo` in this change so the established
aggregate read ABI remains stable. Callers that need both values can read
`get_pair_info` and `get_swap_version`; the latter is the authoritative version
for `swap`.

## Testing matrix

The unit and public-client integration tests cover:

| Case | Expected result |
| --- | --- |
| Initial read | Version is zero |
| Matching zero | Swap succeeds at version one |
| Matching non-zero | Swap succeeds and returns next version |
| Stale expected value | Typed conflict contains current version |
| Two calls from one base | First succeeds, second fails |
| Different pairs | Versions remain independent |
| Read after conflict | Version remains at successful value |
| Paused router | Typed pause error, no version write |
| Zero amount | Typed amount error, no version write |
| Unknown pair | Typed pair error, no version write |
| Fee and liquidity path | Existing accounting remains in receipt |
| Successful transition | `swap_ver` event emitted after write |
| Version maximum | Checked overflow error, no wrap |

The public integration tests are intentionally outside the library module.
They compile against the generated client and validate the ABI a downstream
consumer uses. The in-module tests validate the same transitions with direct
test setup and keep implementation regressions close to the contract code.

## Review checklist

Reviewers should confirm:

- the version key includes both pair symbols;
- absent storage reads as zero;
- the compare occurs before route accounting;
- a mismatch returns the observed current value;
- the version write occurs only after successful accounting;
- checked addition prevents wraparound;
- the event is emitted after the write;
- pause, amount, and pair failures do not mutate state;
- existing `compute_route_fee` behavior is unchanged;
- read-only version inspection remains available while paused;
- tests use the generated public client; and
- the PR description links `Closes #451`.

## Non-goals

This guard does not provide:

- cross-contract coordination;
- a historical version log;
- a global transaction ordering guarantee;
- automatic retries;
- token custody or path payment execution;
- protection against a caller deliberately using stale business inputs; or
- a replacement for application-level idempotency keys.

Those concerns require separate protocol and product decisions. The value of
this change is narrower: it turns a pair-state race into an explicit typed
conflict at the state boundary where the caller can respond safely.

## Upgrade guidance

The new key is additive. Existing `DataKey` values are not renamed, and a pair
does not need an initialization transaction for version zero. A deployment
that upgrades to this code can expose `swap` immediately for registered pairs.

Before enabling clients, operators should verify that the pair registration,
liquidity, amount bounds, fee policy, and pause policy reflect the intended
route. The version guard does not make an unconfigured pair routable.

Indexers should begin storing the pair version from successful receipts or
`swap_ver` events. They should not infer it from route count because those are
separate counters and legacy calls can advance route accounting without
advancing the guarded swap version.

## Summary

`swap` is a compare-and-swap boundary for pair-scoped route accounting. Read a
version, submit that expected version, and accept only the transition that
matches current state. A concurrent stale operation receives a structured
current version and does not apply route side effects. Successful operations
advance one pair version, preserve existing fee/liquidity behavior, and emit a
reconciliation event.
