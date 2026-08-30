# StableRoute emergency-stop policy

The router has one instance-storage `Paused` flag and one shared
`require_not_paused` guard. The flag is intended to stop route mutations while
leaving enough read and governance access for incident response. This document
records the boundary implemented by `StableRouteRouter` and the public-client
tests in `tests/pause_emergency.rs`.

## State transition

The state is a boolean:

```text
false --pause--> true --unpause--> false
```

The absent storage value is interpreted as `false`. Initialization does not
need a separate migration for the flag. `pause` and `unpause` are admin-gated,
and both operations are idempotent so an incident automation job can safely
repeat a transition after an uncertain submission.

## Public operations

| Operation | While unpaused | While paused | Authorization |
| --- | --- | --- | --- |
| `pause` | Sets `true` | Remains `true` | Admin |
| `unpause` | Remains `false` | Sets `false` | Admin |
| `is_paused` | Returns `false` | Returns `true` | Public read |
| `quote_route` | Returns quote | Returns quote | Public read |
| `get_pair_info` | Returns state | Returns state | Public read |
| `compute_route_fee` | Performs accounting | Typed pause failure | Public route call |
| `register_pair` | Registers pair | Typed pause failure | Admin |
| `register_pairs` | Registers batch | Typed pause failure | Admin |
| `set_pair_fee_bps` | Updates fee | Typed pause failure | Admin |

The full implementation contains additional governance, configuration, and
lifecycle operations. Their pause behavior follows the shared policy stated in
the source comments: recovery/governance and read-only planning remain
available, while route-accounting and explicitly pause-gated mutations stop.
The authoritative list is the `DataKey::Paused` documentation and each
entrypoint's guard placement in `src/lib.rs`.

## Why reads remain available

During an incident, operators need to inspect pair registration, fee settings,
liquidity, counters, and the pause state. Integrators also need to determine
whether a proposed route would have been acceptable once the pause is lifted.
Keeping `quote_route` and getters available provides that visibility without
recording a route or decrementing liquidity.

Read availability does not mean a quote is an authorization to submit a route.
Clients must check `is_paused` and handle a typed pause failure from any
mutation. A quote can become stale between the read and a later unpause.

## Why the guard is shared

The pause check belongs in one private helper so every pause-gated mutation
uses the same storage key and error. Duplicating the condition in each public
method creates drift: a new entrypoint can accidentally bypass the emergency
stop, or two methods can return different errors for the same state.

The helper reads instance storage, where the hot singleton is colocated with
the contract instance. It does not create a second persistent flag and does
not use a caller-provided pause value. The on-chain value is the authority.

## Admin boundary

The current admin is loaded through the existing admin helper and must
authorize `pause` and `unpause`. A non-admin cannot pause the router by
supplying a different address in an argument because the pause entrypoints do
not accept an arbitrary caller identity; Soroban authorization is required
from the configured admin account.

The pause flag does not rotate the admin, modify fee configuration, or change
pair registration. It is an operational circuit breaker only. Admin handover
and upgrade behavior remain governed by their existing timelock and
authorization rules.

## Failure semantics

While paused, a blocked mutation fails with the stable `RouterError::ContractPaused`
code. It does not:

- increment `TotalRoutesAllTime`;
- increment a pair route count;
- update `PairLastRouteAt`;
- decrement `PairLiquidity`;
- update pair volume;
- register a new pair;
- change a pause-gated fee; or
- emit a successful route event.

The read-only methods do not write a recovery marker or silently clear the
flag. A rejected call leaves the router paused and can be retried only after
an authorized unpause.

## Event behavior

Each transition publishes the existing `paused` event with a boolean payload:

```text
pause()   -> topic: paused, data: true
unpause() -> topic: paused, data: false
```

Repeated idempotent calls may publish the same state event again. Consumers
should treat the latest event as an observation and confirm the state with
`is_paused` when reconciling after a network timeout. Events are useful for
monitoring but the storage read is authoritative.

## Incident runbook

When suspicious behavior is detected:

1. Submit `pause` from the configured admin account.
2. Confirm `is_paused() == true` in a subsequent read.
3. Confirm the `paused` event and transaction result.
4. Review pair state, route counters, liquidity, and recent events.
5. Keep route mutations stopped while determining scope and remediation.
6. Use read-only quotes to evaluate the post-incident state.
7. Apply any approved governance/configuration recovery steps.
8. Submit `unpause` only after the maintainer decision is recorded.
9. Confirm `is_paused() == false` and monitor the first resumed route.

If the pause transaction's inclusion is unknown, repeat the admin call or read
the flag first. Idempotency makes a repeated pause safe. Do not assume a local
RPC timeout means that no transaction was included.

## Recovery and governance

The flag is not time-boxed and does not auto-unpause. This is deliberate: an
emergency stop should not expire while an incident is still active. The admin
must explicitly resume operations after review.

Governance and upgrade entrypoints remain available according to the source
policy so maintainers can recover from or patch a problem. This is a documented
trade-off. Keeping upgrade access does not make the upgrade safe by itself;
admin key custody and deployment review remain outside this contract.

## Client guidance

Clients should:

- read `is_paused` before presenting a route action;
- handle `ContractPaused` as a normal operational state, not a malformed
  request;
- avoid unbounded automatic retries while paused;
- keep the quote and route submission in separate state-aware steps;
- refresh pair information after unpause; and
- reconcile uncertain submissions using transaction status, events, and reads.

The pause check does not replace client idempotency. A client may receive a
network error after an unpaused route was included; it must reconcile before
submitting the same business action again.

## Testing strategy

The unit tests near the contract implementation cover the shared helper,
gated mutations, idempotency, and recovery. The public integration suite
additionally verifies:

- the default read state;
- pause/unpause through the generated client;
- quotes and pair reads while paused;
- route rejection while paused;
- route resumption after unpause;
- both transition events;
- repeated transition calls;
- batch mutation rejection;
- state preservation after a rejected mutation; and
- no route counter side effect from a blocked route.

This separation matters. An in-module test can accidentally call a private
helper or bypass the generated ABI. The integration tests use the contract as
an external consumer and therefore protect the public argument and return
shapes.

## Review checklist

Reviewers should verify:

- `Paused` remains a single instance-storage source of truth;
- `pause` and `unpause` both use the existing admin authorization;
- the shared guard is used by each intended mutation;
- reads remain usable during the pause;
- blocked route calls do not change accounting state;
- transition events carry the new boolean state;
- repeated transitions do not make recovery impossible;
- no automatic unpause was introduced;
- tests exercise public generated-client behavior; and
- the PR body links `Closes #452`.

## Scope and non-goals

This policy does not add granular per-function pause reasons, timed pauses,
cross-contract circuit breaking, or automatic incident remediation. It does
not guarantee that an already-included transaction can be cancelled, and it
does not protect the admin key. It provides a simple shared stop for the
router's state-changing route surface with explicit read and recovery rules.

## Verification commands

Run the focused public integration suite with:

```sh
cargo test --test pause_emergency
```

Run the full repository suite with:

```sh
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
```

The release build remains:

```sh
cargo build --target wasm32-unknown-unknown --release
```

The local test commands depend on the available Rust toolchain, disk space,
and installed wasm target. CI should execute the same commands from a clean
checkout.

## Summary

Pause is an admin-controlled, instance-stored boolean with a shared guard.
Mutating route operations fail closed with a stable typed error, read-only
inspection remains available, and both transitions emit observable events.
The policy is intentionally simple so incident responders can understand and
verify it quickly while the existing router accounting remains unchanged.

## Transition invariants

The following invariants should hold for every invocation:

1. `is_paused` returns the value stored in `DataKey::Paused`.
2. Only the configured admin can change that value.
3. `pause` never changes the value to `false`.
4. `unpause` never changes the value to `true`.
5. A blocked route cannot increment a counter.
6. A blocked route cannot decrement liquidity.
7. A blocked route cannot publish a route-success event.
8. A read does not clear or rewrite the flag.
9. A repeated transition remains a valid administrative operation.
10. A successful unpause is the only transition back to normal route service.

These invariants are deliberately expressed in terms of externally observable
state. They make it possible for a monitoring job to detect a partial or
unexpected transition without relying on private implementation details.

## Monitoring signals

An operator dashboard should expose:

| Signal | Source | Interpretation |
| --- | --- | --- |
| Current pause state | `is_paused` | Authoritative operational state |
| Pause transition count | `paused` events | Number of observed transition calls |
| Last pause transaction | Event ledger metadata | Incident start reference |
| Last unpause transaction | Event ledger metadata | Recovery reference |
| Blocked route errors | Transaction results | Demand during incident |
| Route counter | `get_total_routes_all_time` | Must not rise from blocked calls |
| Pair liquidity | `get_pair_info` | Must not fall from blocked calls |

Alerts should distinguish a paused state from an RPC outage. If reads fail,
the dashboard should report the state as unknown rather than assuming that the
router is either safe or paused. If an unpause event is observed but the read
still reports true after finality, the operator should investigate the
transaction and network before allowing traffic.

## Reconciliation after an incident

After recovery, compare the route counter and per-pair accounting with the
last known good snapshot. A rejected route should not appear as a successful
counter increment. If an operation's inclusion was uncertain during the pause,
use the transaction status and route event rather than a client retry count.

For every pair that received traffic near the transition:

- record the version of the pause read used by the client;
- record the ledger sequence of the pause and unpause transactions;
- compare route count before and after the incident;
- compare liquidity before and after the incident;
- inspect successful route events; and
- document any operation submitted before the pause was finalized.

This procedure does not reverse a valid route. It makes the boundary auditable
and helps identify whether a caller submitted a transaction before observing
the emergency stop.

## Mutation inventory review

When a new state-changing entrypoint is added, classify it before merging:

| Question | Required decision |
| --- | --- |
| Does it move or account for routed value? | Gate it with the shared pause helper. |
| Does it change pair registration? | Gate it unless it is an explicitly documented recovery action. |
| Does it only read state? | Keep it available for incident visibility. |
| Does it change governance or upgrade state? | Document why recovery needs it while paused. |
| Does it call an external contract? | Review pause and reentrancy ordering together. |
| Does it emit an operational event? | Specify whether it represents an attempt or success. |

The classification belongs in the entrypoint documentation and the pause
coverage matrix. A new mutation must not rely on a reviewer noticing that it
forgot the guard among unrelated business logic.

## Test failure interpretation

If the pause integration suite fails because a mutation succeeds while paused,
the change is a release blocker. If a read fails while paused, the incident
inspection path is broken and is also a release blocker. If an idempotent
transition fails, automation may be unable to recover after an uncertain
transaction and the failure should be treated as high severity.

If only event-count assertions fail, inspect whether the implementation changed
idempotent event policy. Repeated calls may emit repeated observations under the
current policy; changing that behavior requires updating monitoring guidance and
the event contract together.

## Safe rollout

The pause flag is additive and defaults to unpaused when absent. A rollout plan
should still include:

1. build the release artifact from the reviewed commit;
2. run the public integration suite;
3. confirm the admin account can authorize both transitions;
4. confirm the monitoring account can read the flag and pair state;
5. rehearse a pause and unpause on a non-production deployment;
6. record the event topic and payload in the indexer configuration;
7. announce the operational owner for emergency decisions; and
8. only then route production traffic through the guarded API.

The rehearsal should include an uncertain RPC response and a repeated pause.
The responder must be able to determine final state from a read and proceed
without manually editing application databases.

## Documentation ownership

The source comments define the exact set of guarded and unguarded methods.
This document defines the operational interpretation. The README should link to
this policy when it describes the router's emergency behavior. If the source
policy changes, update this file, tests, monitoring labels, and release notes in
the same change.

Avoid phrases such as "all operations stop" unless the implementation truly
gates every mutation. The current policy intentionally leaves governance,
recovery, and configuration exceptions available. Precision in incident
documentation prevents responders from assuming that an emergency stop has a
stronger boundary than it actually does.

## Operator handoff

At the end of an incident, the outgoing operator should hand over:

- the final pause state and confirmation ledger;
- the reason for the pause and the approving authority;
- affected pairs and observed route counters;
- transactions whose inclusion was uncertain;
- monitoring alerts that remain open;
- the commit and artifact used for recovery; and
- the next review time for the pause policy.

The incoming operator should independently read `is_paused`, confirm the
configured admin, and review the most recent `paused` event before accepting
responsibility. This prevents a handoff note from becoming the only record of
the actual on-chain state.

## Practical command list

The minimal read and test checklist is:

```sh
cargo test --test pause_emergency
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --target wasm32-unknown-unknown --release
```

The first command exercises the public pause boundary. The next two enforce
source quality, and the final command verifies the deployment target. Run the
commands from a clean checkout when preparing a release or incident patch.
