# Bounded all-or-nothing route batches

`compute_route_fees` is the bounded batch equivalent of the single-route
`compute_route_fee` entrypoint. It accepts a vector of `(source, destination,
amount)` entries, validates the complete vector, and returns one fee for each
entry in the original order.

## Contract

The batch has a hard maximum of `MAX_BATCH_SIZE` entries. An empty vector is
rejected with `EmptyBatch`; a vector above the limit is rejected with
`BatchTooLarge`. The limit protects CPU, memory, event, and storage budgets and
is exposed through `get_limits` for clients that need to size requests.

Each item uses exactly the single-route policy:

- the router must be initialized;
- the router must not be paused;
- the pair must be registered;
- the amount must be positive;
- the amount must satisfy the pair minimum and maximum;
- configured liquidity must cover the amount;
- fee basis points, absolute cap, and absolute floor determine the fee.

The result vector has the same length and order as the input. Callers should
associate a returned fee with its item by index, not by sorting or by pair.

## Two-phase execution

The implementation uses two phases. The preflight phase checks every item
without updating counters, liquidity, route timestamps, or emitting route
events. Only after the whole vector passes does the effect phase call the
canonical single-route function for each item.

The canonical call repeats the checks at the effect boundary. This intentional
defence in depth keeps the batch behavior aligned if the single-route policy
gains a new guard in a later release. If any item fails during the effect
phase, Soroban transaction atomicity rolls back all earlier effects in that
invocation.

## Atomicity guarantees

An accepted batch updates all affected state in one transaction. A rejected
batch leaves the following unchanged:

- every pair's reported liquidity;
- every pair's route count and cumulative volume;
- the global route counter;
- every pair's last-route timestamp;
- all route and liquidity-used events.

This guarantee covers failures from preflight validation and failures from the
canonical effect phase, including cooldown conflicts and future policy guards.
Applications must submit the batch as one transaction; splitting a vector
into independent transactions intentionally loses the all-or-nothing property.

## Duplicate pairs

The batch accepts repeated pairs as separate entries only when the normal
single-route policy permits the sequence. With a configured cooldown, a second
route for the same pair in the same ledger may fail. Because all entries are
inside one transaction, that failure rolls back the first route too. Clients
should either use distinct pairs or disable cooldown only through the normal
admin governance process.

Repeated pairs without a cooldown consume liquidity and increment metrics once
per entry. They are not silently coalesced because coalescing would change
event count, accounting, and fee semantics relative to individual calls.

## Error handling

Errors are typed router errors and do not expose internal storage details. A
missing pair returns `PairNotRegistered`; an invalid amount returns
`AmountMustBePositive`; an amount outside a configured bound returns the
corresponding boundary error; and insufficient liquidity returns
`InsufficientLiquidity`.

An error in any later item is not converted into a partial-success response.
The transaction fails and callers receive one failure. This makes retries
safe from the router's perspective: after a failed call, the caller can fix
the entire vector and resubmit without needing to compensate an unknown
subset of applied routes.

## Pause behavior

The batch is state changing and is rejected while the router is paused. The
pause check occurs before item validation. Read-only `quote_route` remains
available during a pause, so clients can continue preparing and displaying
candidate batches without causing route accounting effects.

Governance operations remain available while paused so an admin can recover
the router. A successful batch cannot slip through a pause because both the
batch entrypoint and every canonical single-route call enforce the guard.

## Event semantics

Successful entries emit the same per-item events as individual route calls.
There is no synthetic “batch succeeded” event to avoid creating two sources of
truth for accounting. Indexers can reconstruct the batch by transaction hash
and preserve each `route` and `liq_used` event in ledger order.

Failed transactions do not leave successful route events on the ledger. An
indexer should treat the transaction result as authoritative and should not
display locally simulated events as committed routes.

## Client guidance

Before submitting, a client should read `get_limits`, confirm each pair is
registered, and use `quote_route` for user-visible fee previews. Quotes are
advisory because liquidity, cooldown, pause state, and configuration can
change before the batch is included. The transaction remains the final
authority.

Clients should preserve the input order, record the transaction hash, and
wait for finality. On failure, reload all affected pair information before
retrying. Do not assume that a failure in item five means items one through
four were committed.

If a client wants independent retry behavior, it may submit separate
single-route calls, but it must clearly communicate that this is a different
atomicity mode. The bounded batch should be used when all routes belong to one
business operation and must settle together.

## Security review

The primary risks are unbounded resource use, partial accounting, policy
drift, and inconsistent event interpretation. The size limit addresses
resource growth; preflight plus transaction atomicity addresses partial state;
delegation to the canonical single path addresses policy drift; and per-item
events with transaction ordering address indexer consistency.

No caller-supplied value bypasses admin authorization, pair registration,
pause checks, amount bounds, liquidity checks, cooldowns, or fee policy. The
batch endpoint does not introduce a privileged route mode.

The batch is not a settlement transfer API. It computes and accounts for
router route fees according to the existing contract model. It does not move
tokens, call an external token contract, or provide partial-success semantics.

## Test matrix

The focused tests cover:

1. A valid multi-pair batch returns ordered fees and updates both metrics.
2. An invalid later pair fails before the first route is committed.
3. An empty vector returns `EmptyBatch`.
4. An oversized vector returns `BatchTooLarge`.
5. A non-positive amount leaves counters unchanged.
6. Pair minimum and maximum rules match single-route behavior.
7. The result count equals the input count on success.

The broader router suite continues to cover registration, fee caps, fee
floors, liquidity, cooldown, pause, reentrancy, and governance. Integration
checks should additionally inspect the event stream and transaction-level
rollback after a late-item failure.

## Compatibility

Existing single-route callers and their ABI are unchanged. The new endpoint is
additive and uses existing error codes and fee calculations. Existing clients
that do not know the endpoint continue to operate exactly as before.

The batch size is a protocol limit. Raising it is a resource-governance change
and requires fresh gas measurements, indexer review, and a migration note. A
client must not assume that a value accepted on one network is accepted on all
networks without reading `get_limits`.

## Operational checklist

- [ ] Read `get_limits` before constructing the batch.
- [ ] Confirm every pair is registered.
- [ ] Preview fees with `quote_route`.
- [ ] Keep the vector at or below the advertised limit.
- [ ] Submit the vector in business-operation order.
- [ ] Wait for finality before recording success.
- [ ] Reconcile every per-item event by transaction hash.
- [ ] On failure, reload all pair state before retrying.
- [ ] Never treat local simulation as committed accounting.
- [ ] Re-run gas checks after changing the batch bound.

This design gives callers one clear atomic operation while preserving the
router's established authorization, validation, accounting, and event rules.

## Failure recovery playbook

When a batch transaction fails, first retain the transaction result, the exact
serialized vector, and the ledger timestamp used for the attempt. Do not
reconstruct the vector from a UI table after the fact; a changed symbol,
amount, or ordering can represent a different business operation.

Read the global route counter and every affected pair's route count, volume,
liquidity, and last-route timestamp. These values should match the snapshot
from before submission. If an indexer reports a route event from a failed
transaction, mark it pending investigation and reconcile against finalized
ledger state rather than applying a compensating route.

If the failure was a missing pair, register the pair through the admin path
and submit a newly reviewed batch. If it was an amount boundary or liquidity
failure, update the business request instead of bypassing the limit. If it was
a cooldown failure, wait for the pair's cooldown and re-read the timestamp.

A caller should not blindly retry an entire batch while the router's
configuration may have changed. Quote again, compare the new result to the
user's approved operation, and record the new transaction hash. The batch
endpoint is deterministic for a fixed state and input, but state can change
between ledger closes.

## Resource accounting

Batch capacity is bounded by count, but resource use also depends on whether
each item has configured liquidity, cooldown metadata, and fee configuration.
Gas benchmarks should measure representative worst cases, including the
maximum number of distinct pairs and the maximum number of persistent writes.

The implementation avoids storing a temporary batch record. Preflight reads
the existing pair slots, and the effect phase updates only the same slots the
single route path would update. This keeps storage growth proportional to
routes, not to the size of an historical batch request.

The returned fee vector is bounded by the input vector. It does not include
duplicate metadata, raw serialized input, or arbitrary caller strings. Event
payloads likewise contain pair symbols and accounting values only. These
limits reduce both return-size and event-indexing risk.

If the protocol raises `MAX_BATCH_SIZE`, the change must include updated CPU
and memory measurements, a review of worst-case event count, and an update to
the client limit discovery documentation. A larger limit is not merely a
convenience change because it changes the maximum cost of a single operation.

## Observability

Metrics should count submitted batches, accepted batches, rejected batches,
and rejection categories. They should also count total entries in accepted
batches and the maximum observed batch size. These metrics are operational
telemetry and must not be used as a second accounting source.

Logs should include the transaction hash, batch size, result category, and
router address. Avoid logging raw user payloads if route symbols or amounts
are considered sensitive in the deployment environment. A failed batch should
be identifiable without leaking credentials or authorization material.

Indexers should associate each per-item event with its transaction hash and
ledger sequence. If a future implementation adds a batch summary event, the
summary must be treated as a convenience projection; per-item route events
remain the canonical accounting record.

## Review questions

Reviewers should verify that the batch does not call an external contract
between preflight and effects in a way that can invalidate the atomicity
assumption. If a future external call is introduced, the reentrancy guard and
rollback tests must be extended before merging.

Reviewers should verify that every new validation is applied consistently to
both the batch and single-route entrypoints. If a rule is intentionally
different, it must have a named error, documentation, and a dedicated test.
The safest default is to reuse the single-route path as this implementation
does.

Reviewers should verify that a later-item failure cannot return a partially
filled fee vector. The contract returns only after all effect calls finish, so
callers receive either a complete result or a failed transaction.

Reviewers should verify that empty input behavior is explicit. Treating an
empty batch as success can be useful for generic pipelines, but this contract
rejects it to surface accidental empty settlement work and to keep a success
event impossible without a route entry.

## Release notes

The endpoint is additive and does not alter the encoding of existing
`PairInfo`, `PairInfoExt`, or route events. Clients may detect support by
reading the generated contract specification or by attempting the new method
against a known test deployment. Production clients should use an explicit
contract version check where available.

The feature does not guarantee that external settlement systems process all
returned fees. It guarantees only that this router's route accounting is
performed atomically. Settlement integrations must retain their own
idempotency keys and reconcile the router transaction before paying out.

The release owner should archive the source revision, wasm hash, ABI snapshot,
gas report, test report, and a sample successful and failed transaction. This
evidence makes it possible to distinguish a client misuse from a protocol
regression during incident response.

## Quick reference

| Question | Contract answer |
| --- | --- |
| Is the batch bounded? | Yes, by `MAX_BATCH_SIZE`. |
| Is empty input accepted? | No, `EmptyBatch`. |
| Are all items validated first? | Yes, preflight precedes effects. |
| Are effects partially committed? | No, the transaction is atomic. |
| Are fees returned in input order? | Yes. |
| Are per-item events emitted? | Yes, the existing route event. |
| Can a batch bypass single-route checks? | No. |
| Does pause block the batch? | Yes. |
| Does a failed batch change counters? | No. |
| Can callers inspect limits? | Yes, via `get_limits`. |

This reference is intentionally short enough for operators while the
sections above define the detailed security, compatibility, and recovery
contract.

## Example lifecycle

An integration first obtains the limit snapshot and prepares two registered
routes. It previews each fee, obtains user approval for the complete vector,
and submits one `compute_route_fees` transaction. After finality it records
the returned fees and reconciles the two route events.

If the second route is rejected, the integration displays one failed batch and
does not mark the first route as complete. It reloads both pairs, explains the
specific typed failure, and asks the user whether to revise the complete
operation. This preserves the user's mental model of one atomic action.

The same lifecycle applies to a batch containing one item. Clients may use
the single endpoint for a single route, but the policy and accounting result
are identical when the batch endpoint is used.

The release checklist must include a successful two-item transaction, a late
invalid-item rollback, the exact-limit boundary, and an oversized rejection.
These cases verify both user-visible behavior and the resource guard.

The same evidence should be attached to the PR and deployment record so
maintainers can review atomicity, boundary handling, and event behavior
without reproducing an unfinalized transaction.

All test evidence must come from finalized transactions or deterministic unit
fixtures, never from a manually edited report.

This preserves a reviewable chain from input vector to finalized ledger result.

The operation is therefore suitable for settlement planning that requires a
single auditable success or failure boundary.
