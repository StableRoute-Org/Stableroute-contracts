# Route event contract

This document defines the event interface for the router's state-changing
route operation. It is written for indexers, analytics services, relayers,
and reviewers who need to distinguish a successful route from a quote or a
failed transaction.

The contract currently exposes `compute_route_fee` rather than an entrypoint
named `swap`. A successful `compute_route_fee` call is the router's
swap-like state transition, and the existing `route` event is its canonical
success event. The event name is therefore intentionally preserved for ABI
compatibility.

## Goals

The event contract has five goals:

1. Every successful route is observable.
2. Every successful route has one and only one canonical route event.
3. A bounded liquidity debit is observable with its resulting balance.
4. Read-only calls and reverted calls do not look successful.
5. Payloads remain small, typed, directional, and stable for indexers.

The implementation and tests use these goals as invariants. They do not rely
on a consumer inferring state changes from storage diffs alone.

## Event inventory

### `route`

The `route` event is emitted once after a successful route's accounting state
has been written.

| Field | Type | Meaning |
| --- | --- | --- |
| topic | `Symbol` | `route` |
| source | `Symbol` | Asset leaving the source side of the route |
| destination | `Symbol` | Asset entering the destination side |
| amount | `i128` | Exact requested source amount |

The data payload is the tuple:

```text
(source: Symbol, destination: Symbol, amount: i128)
```

The amount is not the fee, the net amount, or a rounded display amount. It is
the exact amount supplied to `compute_route_fee`. A consumer that needs the
fee should use the call result or reproduce the documented fee calculation;
it must not reinterpret the event amount as a fee.

### `liq_used`

The `liq_used` event is emitted only when a pair has an explicitly stored,
finite liquidity value. It is emitted once for the successful debit and
contains the balance after the debit.

| Field | Type | Meaning |
| --- | --- | --- |
| topic | `Symbol` | `liq_used` |
| source | `Symbol` | Debited pair source |
| destination | `Symbol` | Debited pair destination |
| remaining_liquidity | `i128` | Pair liquidity after consuming `amount` |

The data payload is:

```text
(source: Symbol, destination: Symbol, remaining_liquidity: i128)
```

An unset liquidity slot means the pair uses the router's unbounded sentinel.
That path has no stored balance to debit, so it emits no `liq_used` event.
This is intentional and is different from an explicitly stored zero, which
is rejected as insufficient liquidity.

## Event ordering

For a bounded successful route, events are emitted in this order:

```text
liq_used(source, destination, remaining_liquidity)
route(source, destination, amount)
```

The liquidity event describes the intermediate accounting result. The route
event is the final success marker. Consumers that process a transaction in
order may update a pair balance from `liq_used` and then record the route from
`route`.

For an unbounded successful route, the sequence is:

```text
route(source, destination, amount)
```

There is no synthetic liquidity event with `i128::MAX`. Emitting such an
event would make an unbounded pair look like it had a real finite balance.

For a batch containing multiple entries, each entry goes through the same
single-route path. Events retain input order:

```text
liq_used(entry_1)     # only if entry_1 is bounded
route(entry_1)
liq_used(entry_2)     # only if entry_2 is bounded
route(entry_2)
...
```

There is no batch summary event. The per-entry route event is the complete
success record for that entry.

## State and event atomicity

Soroban transaction atomicity applies to storage and events together. A
transaction that fails after an earlier batch item has been processed rolls
back the earlier item and its events.

Indexers should therefore commit event-derived state only after accepting the
transaction outcome. They should not treat an event observed during a failed
simulation or an incomplete local execution as a confirmed route.

The router performs all route guards before accounting effects. These guards
include:

- positive amount;
- registered source/destination pair;
- configured minimum amount;
- configured maximum amount;
- sufficient liquidity when a finite balance is configured; and
- pair cooldown requirements.

No `route` or `liq_used` event is emitted when one of those checks fails. A
failed bounded route also leaves the stored liquidity unchanged.

## Read-only behavior

`quote_route` calculates fee and net amount without updating route counters,
timestamps, volumes, or liquidity. It emits no `route` event and no
`liq_used` event.

The following methods are also read-only from the event contract's point of
view:

- `get_pair_info`;
- `get_pair_info_ext`;
- `get_pair_liquidity`;
- `get_pair_last_route_at`;
- `get_total_routes_all_time`;
- `quote_route`;
- `route_tag`; and
- `get_limits`.

Do not build a route activity feed by watching reads. Only successful
`compute_route_fee` effects represent route activity.

## Direction is part of the identity

`source` and `destination` are ordered fields. A route from `USDC` to `EURC`
is not equivalent to a route from `EURC` to `USDC`.

Consumers must preserve both fields exactly as emitted. They should not sort
the pair, alphabetize symbols, or collapse both directions into one event
without retaining the original direction. The existing `route_tag` helper is
also direction-sensitive for this reason.

## Topic and payload constraints

Each event uses one short topic symbol. Topic symbols are limited to the
Soroban short-symbol length supported by this contract. The canonical topics
are:

| Topic | Length | Purpose |
| --- | ---: | --- |
| `route` | 5 | Successful route accounting |
| `liq_used` | 8 | Finite liquidity debit |

The payload is a tuple rather than a map. Tuple order is part of the ABI and
must not be changed casually. A consumer should decode by the documented
types and positions, not by a display string.

Adding a second topic such as `swap` for the same transition would create two
success records for one state change and force every indexer to deduplicate
them. The router therefore keeps `route` as the single canonical topic.

## Indexer processing guide

An indexer can process the events with the following rules:

1. Filter events to the router contract address.
2. Decode the single topic as a short `Symbol`.
3. Decode `route` as `(Symbol, Symbol, i128)`.
4. Decode `liq_used` as `(Symbol, Symbol, i128)`.
5. Preserve transaction and event ordering.
6. Treat `route` as the successful route record.
7. Apply `liq_used` as the post-debit finite balance for that pair.
8. Ignore unknown topics for forward compatibility, but retain them for
   diagnostics.
9. Do not synthesize a `liq_used` event for unbounded pairs.
10. Reconcile route counters and pair balances periodically from read calls.

The event payload does not include the transaction fee, ledger timestamp, or
caller address. Those values belong to the enclosing transaction and ledger
metadata. Keeping them outside the payload avoids duplicating transport-level
fields and preserves the compact event ABI.

### Idempotency

Use the enclosing transaction hash plus event index as the event identity.
Do not use only `(source, destination, amount)`, because identical routes can
occur in different transactions.

For a bounded route, keep both event identities. A `liq_used` event is not a
replacement for the corresponding `route` event; it represents a different
observable field of the same state transition.

### Reorganizations and retries

Follow the network's normal ledger finality and reorganization policy. If a
transaction is rolled back by the network, remove its derived event state.
If a client retries a route in a later transaction, the later transaction is
a new route record even when its arguments are identical.

## Batch processing

`compute_route_fees` accepts a bounded list and invokes the canonical route
path for each item. A batch does not change the payload shape.

For example, a batch with one unbounded and one bounded pair produces:

```text
route(USDC, EURC, 100)
liq_used(XLM, GBP, 1_250)
route(XLM, GBP, 750)
```

The presence of two route events means two entries succeeded. The presence of
one liquidity event means only the second pair had a finite stored balance.

If any preflight or route guard fails, transaction atomicity removes all
events from the failed batch. A consumer must not retain the first event from
a batch when the enclosing transaction did not succeed.

## Compatibility rules

The following changes are backward-compatible additions or clarifications:

- documenting `liq_used` in the ABI catalog;
- adding tests that enforce existing topic and payload behavior;
- adding indexer guidance;
- adding a new event topic for a genuinely new state transition; and
- appending fields only when the serialized ABI strategy explicitly permits
  it.

The following changes require an ABI review:

- renaming `route` to `swap`;
- changing tuple field order;
- changing a field type;
- emitting both `route` and `swap` for one route;
- changing whether unbounded routes emit liquidity events; or
- emitting events from `quote_route`.

The current issue is addressed without any of those breaking changes. The
existing canonical event remains the compatibility boundary.

## Test coverage

The event contract tests cover these cases:

- unbounded success emits one route event;
- unbounded success emits no liquidity event;
- bounded success emits one liquidity debit;
- bounded success reports the post-debit balance;
- bounded success emits the route record exactly once;
- repeated calls each produce one fresh route payload;
- batch entries preserve route order;
- bounded batch entries produce one debit per bounded entry;
- quote calls emit no route activity;
- failed routes emit no success events;
- failed routes do not debit liquidity;
- source and destination remain directional; and
- the maximum supported positive amount is preserved exactly.

These tests intentionally inspect topic counts as well as decoded payloads.
Checking only that an event exists would not detect a duplicate event or a
payload that silently swapped source and destination.

## Review checklist

When changing route accounting, reviewers should confirm:

- [ ] The state-changing operation has one canonical success topic.
- [ ] Every successful route emits that topic exactly once.
- [ ] Every finite liquidity debit emits one `liq_used` event.
- [ ] The event is emitted after the corresponding write is valid.
- [ ] Failed transactions leave no retained success event.
- [ ] Read-only methods remain event-free.
- [ ] Source and destination are not reordered.
- [ ] Amounts are not narrowed, rounded, or converted to fee values.
- [ ] Topic symbols remain within the short-symbol limit.
- [ ] Batch ordering remains deterministic.
- [ ] Tests assert both counts and payload values.
- [ ] The ABI catalog is updated with any new event.
- [ ] Indexer migration notes explain any intentional behavior change.

## Operational examples

### Unbounded pair

Suppose `USDC → EURC` has no explicit liquidity slot and the caller routes
`1_000` units. The transaction emits:

```text
route(USDC, EURC, 1_000)
```

The route counter and volume increase, but the getter for stored liquidity
continues to report the absent-slot default. No consumer should infer a
finite remaining balance from this event.

### Bounded pair

Suppose `USDC → EURC` has `1_000` units and the caller routes `250`. The
transaction emits:

```text
liq_used(USDC, EURC, 750)
route(USDC, EURC, 250)
```

The event consumer can set the pair's finite balance to `750` and record a
successful route of `250`.

### Rejected pair

Suppose the same pair has `500` units and the caller requests `501`. The
transaction fails with `InsufficientLiquidity`. It emits neither canonical
route event nor liquidity debit event, and the stored balance remains `500`.

### Quote

Suppose a planner calls `quote_route(USDC, EURC, 1_000)`. The call returns a
fee/net tuple but emits no route activity. A planner can safely poll quotes
without polluting an execution feed.

## Maintenance note

Keep this document and `docs/abi.md` synchronized with the `symbol_short!`
calls in `src/lib.rs`. Any event addition should include:

1. the implementation topic;
2. the ABI catalog row;
3. payload decoding tests;
4. count and duplicate tests;
5. failure/rollback behavior tests; and
6. an indexer migration note when behavior changes.

The event contract is deliberately explicit because events are a public API.
Once indexers depend on a topic and tuple shape, an apparently small logging
change can become a protocol compatibility change.
