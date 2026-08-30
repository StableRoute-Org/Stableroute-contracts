# StableRoute admin handover policy

StableRoute uses a two-step admin handover. The current admin proposes a
successor; the successor must authenticate an acceptance after the configured
timelock. This separates intent from possession of the successor key and
creates a warning window for operators and indexers.

## State machine

There are three observable states:

1. No pending transfer: the current admin is the only admin.
2. Pending transfer: a successor and an acceptance ETA are stored.
3. Completed transfer: the successor is the admin and pending state is gone.

`propose_admin_transfer` writes both the pending address and its ETA. The ETA
is calculated from the ledger timestamp and the configured timelock at the
time of proposal. Changing the timelock later does not rewrite an existing
proposal. This prevents an operator from shortening a warning window after it
has begun.

`accept_admin_transfer` requires the pending address itself to authenticate,
checks that it matches the stored address, and checks the stamped ETA. On
success it updates the admin, removes both pending slots, renews the instance
TTL, and emits the existing executed event.

## Timelock semantics

The acceptance boundary is inclusive: acceptance succeeds when
`ledger_timestamp >= eta`. Before the ETA, the call fails without changing
admin or pending state. A zero timelock remains supported for deployments
that explicitly choose immediate two-step acceptance; it still requires the
successor to call and authenticate the acceptance.

The admin should choose a delay appropriate to the protocol's risk. The
contract stores seconds, while the ledger provides the authoritative clock.
Clients must not use local wall-clock time to decide whether to submit.

## Cancellation

The current admin can call `cancel_admin_transfer` at any time. Cancellation
removes the pending address and ETA together and emits a `cancelled` event
with the address that was removed, if any. Cancellation with no pending
proposal is a safe authenticated no-op and still produces no pending state.

After cancellation, the former nominee cannot accept because the pending slot
is absent. A later proposal starts a new ETA and must be treated as a new
governance action by monitoring systems.

## Replacement proposals

Submitting a second proposal replaces the existing pending address and stamps
a new ETA using the current timelock. This makes the latest authenticated
admin decision authoritative and avoids two competing pending slots. Indexers
should retain both queued events and the final executed or cancelled outcome.

The current admin and the router contract address are rejected as successors.
Proposing the current admin is a no-op disguised as governance and is rejected
to surface operator mistakes. Proposing the router itself would make future
authentication impossible and is rejected to avoid bricking administration.

## Authorization guarantees

Every proposal, cancellation, and force-completion path authenticates the
current admin. Acceptance authenticates the pending successor and compares
the signer identity to the stored pending address. A caller cannot provide a
different address merely by knowing the pending proposal.

After completion, the old admin no longer satisfies `require_admin` and cannot
propose, cancel, pause, configure pairs, or upgrade. The new admin must use
the same authenticated governance paths. This is an authority replacement,
not an additive permission grant.

## Force completion

The existing force-completion path remains admin-controlled and honors the
same pending-address and ETA checks. It is useful when the successor cannot
submit an acceptance transaction, but it does not bypass the timelock or let
the current admin select an address that was never proposed.

Operations teams should treat force completion as an exceptional path and
record the reason, transaction hash, pending address, and ETA in the change
record. The on-chain executed event does not itself explain the human reason
for using force completion.

## Event contract

The handover event stream contains:

| Event | Meaning | Payload |
| --- | --- | --- |
| `queued` | proposal created or replaced | successor, ETA |
| `cancelled` | pending proposal removed | removed successor or none |
| `executed` | successor installed | new admin |

Events are emitted only after the corresponding state writes are prepared in
the same transaction. Indexers should key records by transaction hash and
ledger sequence and must not infer completion from a queued event alone.

If a transaction fails, no successful handover event remains committed. A
monitor that sees a stale queued event after a failed transaction should
reconcile against finalized ledger state before alerting.

## Incident response

If an unexpected proposal appears, the current admin should cancel it before
the ETA whenever possible, preserve the queued event and transaction details,
and rotate relevant signing credentials. If an unexpected execution appears,
pause affected operations, verify the new admin, and follow the emergency
governance procedure. Do not attempt to repair state by manually editing
storage or by issuing an untracked replacement.

The timelock is a reaction window, not a guarantee that operators will notice
an event. Monitoring should alert on every queued event, display the ETA in
ledger time, and include the successor and contract address in the alert.

## Compatibility

The existing `propose_admin_transfer`, `accept_admin_transfer`, and
`cancel_admin_transfer` entrypoints remain available. The new validation for
current-admin and self-contract successors rejects calls that previously
could create a no-op or unrecoverable proposal. Valid existing handovers keep
their stored ETA and acceptance semantics.

The `PendingAdminInfo` aggregate getter remains the preferred read for
automation because it observes address and ETA together. Clients should use
it immediately before display and after every governance transaction.

## Test matrix

The focused suite covers proposal/ETA storage, early acceptance rejection,
acceptance at the boundary, old-admin loss of control, cancellation and
pending-slot cleanup, replacement proposals, invalid successor addresses,
and safe cancellation when no proposal exists. The broader router suite
covers force completion, pause interaction, initialization, and event
compatibility.

Integration tests should run the same matrix against a deployed contract and
verify event payloads, finalized timestamps, and admin behavior after each
transition. Gas checks should include the proposal, cancellation, acceptance,
and aggregate pending-state read.

## Rollback considerations

A rollback to a version that supports the same pending slots can preserve a
queued handover. Operators must confirm that the rollback version understands
the ETA slot and the cancellation event before using it. If it does not,
complete or cancel the handover under a reviewed procedure before rollback.

Never shorten an active ETA by changing configuration and assuming the queued
proposal will be recomputed. The ETA is intentionally stamped and immutable
for that proposal. A new proposal is the correct way to create a new
governance decision.

## Operator checklist

- [ ] Confirm the current admin from the contract getter.
- [ ] Confirm the successor is an external address and not the current admin.
- [ ] Set or verify the intended timelock before proposing.
- [ ] Record the queued event and exact ETA.
- [ ] Monitor the warning window for cancellation or incident response.
- [ ] Have the successor authenticate acceptance at or after the ETA.
- [ ] Verify pending slots are cleared after acceptance.
- [ ] Verify the old admin cannot exercise privileged operations.
- [ ] Archive queued, cancelled, and executed events with transaction hashes.
- [ ] Use a new proposal for any changed successor or changed intent.

This policy keeps admin rotation explicit, reviewable, time-bounded, and
recoverable without weakening the router's existing authorization controls.

## State observation rules

Automation should read `get_pending_admin_info` rather than reading the
pending address and ETA in separate transactions. The aggregate result is a
consistent snapshot for display and alerting. A `None` pending address and a
`None` ETA represent the normal no-transfer state; a partial pair should be
treated as a storage anomaly and investigated.

The admin slot is authoritative after an executed event. A queued event does
not change the admin slot, and a cancelled event does not create a successor.
Clients must not optimistically grant the successor admin permissions before
the acceptance transaction is finalized.

## Authorization failure behavior

The contract authenticates the current admin before accepting a proposal or
cancellation. It authenticates the pending address before acceptance. A
wrong pending address fails the acceptance path without clearing the pending
slot, so the legitimate nominee can still retry after the failure.

An acceptance before ETA fails without consuming the proposal. It is safe for
clients to retry at or after the exact stamped timestamp, but they should
re-read the pending snapshot first because the current admin may have
cancelled or replaced the proposal during the warning window.

## Monitoring invariants

Monitoring should continuously check these invariants:

- a pending address has a pending ETA;
- an absent pending address has no pending ETA;
- the ETA is not earlier than its proposal timestamp;
- an executed event is preceded by a queued event for the same successor;
- a cancelled event removes the matching pending successor;
- the current admin changes only on an executed transition;
- the old admin does not appear as the current admin after execution.

An invariant violation should be reported as a contract or indexer incident,
not silently repaired by writing a new proposal. The transaction history and
finalized storage state are the evidence needed to determine the cause.

## Change-management guidance

Before proposing a successor, the change owner should record the reason,
successor address, configured delay, expected ETA, and rollback contact. The
successor should independently verify the target contract address and chain
before signing acceptance. The warning window is useful only if both parties
review the same target and ETA.

For a routine rotation, announce the queued transaction, monitor until ETA,
accept, and verify the resulting admin. For an emergency rotation, announce
the reason through the incident channel while preserving the same on-chain
proposal and timelock rules. Do not bypass the protocol because the rotation
is urgent; use the documented governance exception only if it exists in a
separately reviewed deployment.

## Address hygiene

The successor address must be copied from an approved change record and
checked character-for-character before signing. Human-readable labels are not
stored on-chain and cannot substitute for the address. Hardware-wallet and
multisig operators should display the full contract address and successor
address in their approval review.

The router contract address is rejected as a successor because a contract
cannot produce the external authentication needed to accept the role. The
current admin is rejected as a successor because the operation would not
change authority and could conceal an operator error.

## Reconciliation after finality

After acceptance, query the admin, pending snapshot, and any admin-gated
read-only configuration. Confirm that the pending address and ETA are absent
and that the new admin can authenticate a benign governance read or approved
configuration call. Confirm that the old admin is no longer the stored admin.

After cancellation, confirm both pending slots are absent and that the admin
slot is unchanged. After a replacement proposal, confirm the ETA corresponds
to the replacement transaction's ledger timestamp and current timelock, not
the previous proposal's ETA.

## Test failure analysis

If a test observes a pending successor after a supposedly successful
acceptance, inspect whether the ledger timestamp reached ETA and whether the
acceptance was signed by the exact pending address. If a test observes an
unexpected cancellation, compare the authenticated admin and transaction
hash. Do not weaken assertions to accommodate an ambiguous state transition.

Tests should assert state after both success and failure. For failure cases,
the pending address, ETA, admin, and relevant event count should remain
unchanged. For success cases, the state transition and event payload should
agree.

## Indexer replay behavior

Indexers must process queued, cancelled, and executed events in ledger order.
If an event is delivered twice, deduplicate by transaction hash and event
index. A queued event can be superseded by another queued event; it should
remain in history while the latest pending snapshot is used for current state.

An indexer outage must not cause an operator to submit a replacement proposal
without first reading the contract. The contract getters are the fallback
source of truth. Once the indexer catches up, it should reconcile its event
projection with the finalized contract state.

## Versioning considerations

Adding fields to the aggregate pending getter would be an ABI change and must
append fields without reordering existing ones. New event topics should be
introduced additively. Existing event names and payload shapes should remain
stable for downstream watchers.

If a future release introduces a stronger timelock policy, it should apply to
new proposals and clearly document how already queued proposals behave. A
release must not silently reinterpret a stored ETA, because doing so can
shorten the warning window that an operator already relied on.

## Security summary

The two-step flow prevents a single proposal transaction from immediately
changing authority. The ETA provides time to detect and cancel mistakes. The
pending-address check prevents an unrelated account from accepting. The
explicit invalid-address checks prevent self-locking and no-op rotations. The
event stream and aggregate getter make the lifecycle observable.

These controls do not protect a compromised current admin that remains
authorized through the entire timelock. Operational key protection,
monitoring, and incident response remain necessary. The contract provides a
reaction window and deterministic state transitions; it cannot replace
governance review.

## Runbook examples

### Planned rotation

1. Verify the current admin and successor addresses from the change record.
2. Read the current timelock and choose the approved delay.
3. Submit `propose_admin_transfer` from the current admin.
4. Verify the queued event and aggregate pending snapshot.
5. Monitor the ETA and cancel if the proposal is incorrect.
6. At or after ETA, have the successor submit acceptance.
7. Verify the executed event and new admin getter.

### Cancelled rotation

If review finds an incorrect successor, the current admin submits
`cancel_admin_transfer`. The operator confirms the pending address and ETA are
both absent, then records the cancellation reason. A corrected successor must
be proposed through a new transaction and receives a new ETA.

### Lost successor key

If the pending key is unavailable, the current admin may use the reviewed
force-completion path after ETA if organizational policy permits it. The
operator records why force completion was chosen and verifies the same
executed event and post-transition permissions as normal acceptance.

## What this does not guarantee

The contract cannot determine whether an address belongs to the intended human
or organization. It cannot recover a lost private key, validate an off-chain
approval, or guarantee that an indexer is online. Those responsibilities
belong to key management, governance, monitoring, and deployment operations.

The contract also does not make an admin rotation multi-signature. The current
admin authenticates the proposal and the pending successor authenticates the
acceptance. Deployments requiring quorum approval should place a multisig or
governance account in the admin slot and retain this handover as the account
rotation guard.

## Maintainer review checklist

- [ ] Confirm the implementation changes only handover behavior.
- [ ] Confirm new successor validation uses append-only error numbering.
- [ ] Confirm pending address and ETA are cleared together.
- [ ] Confirm cancellation is authenticated and observable.
- [ ] Confirm acceptance checks the stamped ETA, not a recalculated delay.
- [ ] Confirm old-admin permissions are tested after completion.
- [ ] Confirm the aggregate getter remains ABI-compatible.
- [ ] Confirm events are documented for indexers.
- [ ] Confirm tests cover early, boundary, wrong-account, and cancellation paths.
- [ ] Confirm the full CI and gas checks are executed before merge.

## Data-retention guidance

Retain the proposal, cancellation, and execution transaction hashes with the
deployment manifest. Include the contract address, network, signer identity,
timelock, successor, ETA, and final admin. Do not retain private keys or raw
wallet secrets in the manifest or PR.

This history is useful when a later incident asks whether a handover was
authorized, whether the warning window elapsed, and which address accepted.
It also lets maintainers reconcile the on-chain event stream with deployment
automation without relying on mutable screenshots or chat messages.

## Design boundary

The handover state is deliberately separate from pair configuration and route
accounting. A failed proposal or cancellation must not change pair state, and
a paused router must still permit reviewed governance recovery. Future changes
to either area should preserve this separation and add regression tests for
cross-feature interactions.

The review record should include the test network and the observed ledger
timestamps for early rejection and boundary acceptance. This makes the
timelock guarantee independently verifiable during release review.

After a rollback, repeat the getter and event reconciliation. A rollback is a
code change, not a substitute for a governance transition, and it must not
silently recreate or discard a pending proposal.

Maintainers should treat any discrepancy between the event stream and storage
as a release blocker until the finalized transaction trace explains it.

This ensures that proposed, cancelled, accepted, and rolled-back transitions
remain distinguishable throughout the lifetime of the router deployment.

The handover test evidence is part of the operational contract: every
deployment should retain it alongside the ABI snapshot and gas report.

This record should also capture whether the proposal was replaced or
cancelled, so incident reviewers can reconstruct the complete decision path.

The audit record is append-only from the reviewer's perspective and should
never contain credentials, private keys, or unverified successor labels.

This keeps the change history safe to share during audits and incident review.

Reviewers should verify that this record is attached before approving release.
