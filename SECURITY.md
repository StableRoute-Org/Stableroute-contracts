# Security Policy

## Scope

This policy covers the StableRoute Soroban router contract in this repository,
especially `src/lib.rs` and the generated contract interface for
`StableRouteRouter`.

The current router is a routing metadata and fee-calculation contract. It does
not move user funds yet, but it already stores governance and operational state
that future fund-routing integrations will rely on.

## Threat Model

### Governance and Admin Authority

- `DataKey::Admin` stores the operational admin set during `init`.
- Admin-gated entrypoints call `require_admin`, which loads `DataKey::Admin` and
  requires the stored address to authorize the call.
- Admin authority currently controls pair registration, pair fee settings,
  pause/unpause, migration, pair limits, reported liquidity, and fee recipient
  configuration.
- A compromised admin can change operational routing configuration. Treat the
  admin key as production-sensitive infrastructure.

### Two-Step Admin Transfer

- `DataKey::PendingAdmin` stores a proposed replacement admin.
- `propose_admin_transfer` can only be called by the current admin.
- `accept_admin_transfer` requires authorization from the pending admin before
  replacing `DataKey::Admin`.
- `cancel_admin_transfer` lets the current admin clear a pending handover.
- Until acceptance, the current admin remains the effective admin.

This protects against accidental one-step transfers to a mistyped or
uncontrolled address.

### Pause Control

- `DataKey::Paused` stores the pause state.
- `pause` and `unpause` are admin-gated and publish the `paused` event.
- `register_pair` and `set_pair_fee_bps` currently reject calls while paused
  with `RouterError::ContractPaused`.

Known limitation: pause coverage is not yet a single global guard across every
admin configuration entrypoint. Future mutating entrypoints should explicitly
document whether they are pause-gated and should add tests for that decision.

### Pair Configuration and Fee Controls

- `DataKey::Pair(source, destination)` records registered routes.
- `DataKey::PairFeeBps(source, destination)` stores per-pair fees.
- `MAX_FEE_BPS` caps per-pair fees at 1,000 bps, or 10 percent.
- `BPS_DENOMINATOR` is 10,000 and fee math truncates toward zero.
- `DataKey::PairMinAmount`, `DataKey::PairMaxAmount`, and
  `DataKey::PairLiquidity` constrain `compute_route_fee`.
- `DataKey::FeeRecipient` stores the future settlement recipient, but the router
  does not currently transfer tokens.

### Storage and Migration

- Persistent storage holds admin, pair, fee, limit, liquidity, pause, pending
  admin, fee recipient, counter, timestamp, and schema version slots.
- `DataKey::SchemaVersion` tracks the storage schema.
- `migrate_v1_to_v2` is admin-gated and rejects non-v1 starting states with
  `RouterError::MigrationVersionMismatch`.

Known limitation: the contract does not currently include explicit TTL bumping
logic. Any future production deployment should define the desired TTL policy for
persistent governance and route state.

### Error Compatibility

`RouterError` variants are append-only. Do not renumber or reuse an existing
error code after release. Off-chain clients and indexers may rely on these codes
for stable error handling.

## Known Limitations

- No production token movement is implemented in this router yet.
- Pause coverage is not currently universal across every admin entrypoint.
- Configuration changes such as pair fee, min/max amount, liquidity, and fee
  recipient updates apply immediately after an authorized admin call.
- No timelock or multisig enforcement is implemented in the contract.
- No explicit TTL bumping policy is implemented.
- Liquidity is reported by an admin-controlled pathway, not proved from an
  on-chain pool balance.

## Responsible Disclosure

Please do not disclose a suspected vulnerability publicly before maintainers
have had time to investigate and coordinate a fix.

To report a vulnerability:

1. Join the StableRoute Discord: https://discord.gg/37aCpusvx
2. Ask to coordinate a private security report for `Stableroute-contracts`.
3. Include a concise description, affected entrypoints or `DataKey`s, expected
   impact, reproduction steps, and any local test output.
4. Do not include private keys, seed phrases, live user data, or unrelated
   secrets in the report.

Helpful reports usually include:

- the affected function or storage key;
- whether the issue requires admin authorization;
- whether the issue affects funds, governance control, route availability, fee
  accounting, or off-chain indexer compatibility;
- a minimal local reproduction or failing test;
- suggested mitigation if known.

## Research Rules

Stay within local builds, local tests, and publicly documented contract behavior.
Do not attempt social engineering, credential theft, denial-of-service testing,
or unauthorized testing against deployed infrastructure. If a live deployment is
involved, coordinate privately with maintainers before testing anything beyond a
read-only review.

## Maintainer Handling

When a valid report arrives, maintainers should:

1. acknowledge receipt privately;
2. identify affected versions and entrypoints;
3. decide whether a patch, migration, pause, or disclosure advisory is needed;
4. add regression tests where possible;
5. document any new `RouterError` codes without renumbering existing variants.
