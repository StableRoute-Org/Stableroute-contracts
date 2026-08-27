# StableRoute — Storage Model & DataKey Reference

Authoritative reference for the router's on-chain storage
([`src/lib.rs`](../src/lib.rs)). Every `DataKey` variant is listed with its key
shape, value type, storage tier, default-when-absent, the entrypoints that
read/write it, and its TTL class. Defaults are cross-checked against the
`unwrap_or` values in the source.

## Sentinel conventions

- An **absent `bool`** reads as `false` (pair registration, paused,
  reentrancy lock).
- **`i128::MAX`** is the "unbounded" sentinel for `PairMaxAmount` and for
  liquidity *inside `compute_route_fee` only*.
- **`0`** is the default for counters, fees, timestamps (as `u64`),
  `PairMinAmount`, and cooldowns.
- An **absent `Option`** stays `None` (admin, pending admin, fee recipient,
  last-route timestamp, max fee absolute, min fee absolute, oracle) — distinct from a zero
  value.
- `SchemaVersion` defaults to **`1`** when absent (the implicit pre-migration
  default).

## Storage tiers

Contract state lives in two Soroban storage tiers:

- **Instance storage** — `Admin`, `PendingAdmin`, and `Paused`. These are the
  hot globals: every admin-gated entrypoint reads `Admin`, and every
  pause-gated entrypoint reads `Paused` before doing anything else. Bundling
  them with the contract instance avoids a separate persistent-storage read
  (and its own TTL check) on every call.
- **Persistent storage** — every other `DataKey` slot. Persistent entries are
  subject to state archival once their TTL lapses: a pair configured long ago
  but not routed recently can have its entries archived and must be restored
  (bumped) before use.

### TTL classes

| Class | Description | Write frequency | Archival risk |
|-------|-------------|-----------------|---------------|
| **Static** | Written once at construction or migration; never changed afterward | Once | Low — bump once after deploy |
| **Config** | Admin-gated governance/config writes | Rare (governance events) | Moderate — bump after each governance action |
| **Hot** | Written on every `compute_route_fee` call | Every route | **High** — each route extends TTL naturally; but infrequently-routed pairs' hot slots can archive |

The primary TTL mitigation is the natural write frequency of hot slots: every
`compute_route_fee` call extends the TTL of `TotalRoutesAllTime`,
`PairLastRouteAt`, `PairRouteCount`, `PairVolume`, and (when set)
`PairLiquidity`. The `ReentrancyLock` is also written per-route. For
infrequently-routed pairs, a dedicated TTL-extension ("bump") pass on
persistent keys is the reference mitigation.

## DataKey table

### Global singletons

| DataKey | Value type | Tier | TTL class | Default when absent | Read by | Written by |
|---|---|---|---|---|---|---|
| `Admin` | `Address` | **instance** | **Static** | `None` → `NotInitialized` (#2) | `get_admin`, `require_admin` | `__constructor`, `accept_admin_transfer`, `force_admin_transfer` |
| `PendingAdmin` | `Address` | **instance** | **Config** | `None` | `get_pending_admin`, `get_pending_admin_info`, `accept_admin_transfer`, `force_admin_transfer` | `propose_admin_transfer`; removed by `accept_admin_transfer`, `force_admin_transfer`, `cancel_admin_transfer` |
| `PendingAdminEta` | `u64` | persistent | **Config** | `None` | `get_pending_admin_eta`, `get_pending_admin_info`, `accept_admin_transfer`, `force_admin_transfer` | `propose_admin_transfer`; removed by `accept_admin_transfer`, `force_admin_transfer`, `cancel_admin_transfer` |
| `Timelock` | `u64` | persistent | **Config** | `0` (instant handover) | `get_timelock`, `propose_admin_transfer` | `set_timelock` |
| `Paused` | `bool` | **instance** | **Config** | `false` | `is_paused`, `register_pair`, `register_pairs`, `set_pair_fee_bps`, `set_pair_fees_bps`, `compute_route_fee` | `pause`, `unpause` |
| `FeeRecipient` | `Address` | persistent | **Config** | `None` | `get_fee_recipient` | `set_fee_recipient` |
| `MaxFeeAbsolute` | `i128` | persistent | **Config** | `None` | `get_max_fee_absolute`, `apply_fee_cap` (in `compute_route_fee` and `quote_route`) | `set_max_fee_absolute` |
| `MinFeeAbsolute` | `i128` | persistent | **Config** | `None` | `get_min_fee_absolute`, `apply_fee_floor` (in `compute_route_fee` and `quote_route`) | `set_min_fee_absolute` |
| `Oracle` | `Address` | persistent | **Config** | `None` | `get_oracle`, `set_pair_liquidity` (dual-auth check) | `set_oracle`; removed by `remove_oracle` |
| `TotalRoutesAllTime` | `u64` | persistent | **Hot** | `0` | `get_total_routes_all_time` | `compute_route_fee` (saturating `+1`) |
| `SchemaVersion` | `u32` | persistent | **Static** | `1` (implicit v1) | `get_schema_version` | `migrate_v1_to_v2` |
| `ReentrancyLock` | `bool` | persistent | **Hot** | `false` | `enter_nonreentrant` | `enter_nonreentrant` (→ `true`), `exit_nonreentrant` (→ `false`) |

### Per-pair slots — `(Symbol, Symbol)`

All per-pair slots are keyed by `(source, destination)` tuple. Direction
matters: `(USDC, EURC)` and `(EURC, USDC)` are independent storage slots.

| DataKey | Value type | Tier | TTL class | Default when absent | Read by | Written by |
|---|---|---|---|---|---|---|
| `Pair` | `bool` | persistent | **Config** | `false` (not registered) | `is_pair_registered`, `is_pair_active`, `get_pair_info`, `get_pair_info_ext`, `require_pair_registered`, `compute_route_fee`, `quote_route` | `register_pair`, `register_pairs`; removed by `unregister_pair` |
| `PairFeeBps` | `u32` | persistent | **Config** | `0` (free) | `get_pair_fee_bps`, `get_pair_info`, `get_pair_info_ext`, `compute_route_fee`, `quote_route` | `set_pair_fee_bps`, `set_pair_fees_bps`; cleared by `clear_pair_config` (`unregister_pair`) |
| `PairMinAmount` | `i128` | persistent | **Config** | `0` (no floor) | `get_pair_min_amount`, `get_pair_info`, `get_pair_info_ext`, `compute_route_fee` | `set_pair_min_amount`; cleared by `clear_pair_config` (`unregister_pair`) |
| `PairMaxAmount` | `i128` | persistent | **Config** | `i128::MAX` (no ceiling) | `get_pair_max_amount`, `get_pair_info`, `get_pair_info_ext`, `compute_route_fee` | `set_pair_max_amount`; cleared by `clear_pair_config` (`unregister_pair`) |
| `PairLiquidity` | `i128` | persistent | **Hot**† | `0` (getters), `i128::MAX` (`compute_route_fee` only) | `get_pair_liquidity`, `get_pair_info`, `get_pair_info_ext`, `is_pair_active`, `compute_route_fee` | `set_pair_liquidity`, `compute_route_fee` (decrement); cleared by `clear_pair_config` (`unregister_pair`) |
| `PairLastRouteAt` | `u64` | persistent | **Hot** | `None` (`Option`); `0` in `get_pair_info`/`get_pair_info_ext` | `get_pair_last_route_at`, `get_pair_info`, `get_pair_info_ext`, `compute_route_fee` (cooldown check) | `compute_route_fee`; removed by `purge_pair_metrics` |
| `PairRouteCount` | `u64` | persistent | **Hot** | `0` | `get_pair_route_count`, `get_pair_info_ext` | `compute_route_fee` (saturating `+1`); removed by `purge_pair_metrics` |
| `PairVolume` | `i128` | persistent | **Hot** | `0` | `get_pair_volume`, `get_pair_info_ext` | `compute_route_fee` (saturating `+amount`); removed by `purge_pair_metrics` |
| `PairCooldown` | `u64` | persistent | **Config** | `0` (disabled) | `get_pair_cooldown`, `get_pair_info_ext`, `compute_route_fee` (rate-limit gate) | `set_pair_cooldown`; cleared by `clear_pair_config` (`unregister_pair`) |

† **Liquidity default is context-dependent.** `get_pair_liquidity`,
`get_pair_info`, `get_pair_info_ext`, and `is_pair_active` treat an absent
slot as `0`. But `compute_route_fee` reads it with `unwrap_or(i128::MAX)` —
i.e. an unconfigured pair is treated as having *unbounded* liquidity for
routing. Set an explicit liquidity value to enforce the
`InsufficientLiquidity` (#12) guard.

## Clear-on-unregister slots

`unregister_pair` removes `Pair` and calls `clear_pair_config`, which
removes these per-pair config slots so that re-registering the same corridor
starts from documented defaults:

- `PairFeeBps`
- `PairMinAmount`
- `PairMaxAmount`
- `PairLiquidity`
- `PairCooldown`

These operational-history slots are **deliberately preserved** across
unregister/re-register cycles:

- `PairLastRouteAt`
- `PairRouteCount`
- `PairVolume`

Use `purge_pair_metrics` as an explicit, opt-in way to discard a pair's
lifetime history.

## `compute_route_fee` write summary

On every successful route, the following slots are written (extending their
persistent TTL):

| Slot | Operation | Guard |
|------|-----------|-------|
| `ReentrancyLock` | `true` → … → `false` | non-reentrant gate |
| `TotalRoutesAllTime` | `saturating_add(1)` | protocol-wide |
| `PairRouteCount` | `saturating_add(1)` | per-pair |
| `PairVolume` | `saturating_add(amount)` | per-pair |
| `PairLastRouteAt` | `env.ledger().timestamp()` | per-pair |
| `PairLiquidity` | `saturating_sub(amount)` | **only when set** (≠ `i128::MAX`) |

## Versioning

`version()` returns the compiled contract version (`ROUTER_V2`);
`get_schema_version()` returns the persisted storage-layout version
(defaults to `1`, advanced to `2` by `migrate_v1_to_v2`). The two are
independent — see the migration entrypoints in `src/lib.rs`.

## Namespaced key policy

Pair storage is accessed through the typed namespace in
[`src/pool_storage.rs`](../src/pool_storage.rs). A namespace is the ordered
pair `(source, destination)` and a slot is one of the following typed values:

| Slot | Existing `DataKey` mapping | Cleared on unregister |
|---|---|---|
| `Registration` | `Pair(source, destination)` | yes |
| `FeeBps` | `PairFeeBps(source, destination)` | yes |
| `MinAmount` | `PairMinAmount(source, destination)` | yes |
| `MaxAmount` | `PairMaxAmount(source, destination)` | yes |
| `Liquidity` | `PairLiquidity(source, destination)` | yes |
| `LastRouteAt` | `PairLastRouteAt(source, destination)` | no |
| `RouteCount` | `PairRouteCount(source, destination)` | no |
| `Volume` | `PairVolume(source, destination)` | no |
| `Cooldown` | `PairCooldown(source, destination)` | yes |

The mapping is intentionally one-to-one with the deployed enum variants. It
is a namespace and review boundary, not a migration to a new serialized key.
Changing a slot's mapping would change its XDR encoding and is therefore an
ABI/storage migration requiring a new schema version and an explicit repair
plan. Adding a new slot is append-only and must include a layout test and a
default value in this document.

## Access-time TTL bumping

`bump_pair_ttl` renews every existing key in a pair namespace using one policy:

- threshold: 518,400 ledgers (about 30 days at five seconds per ledger);
- extension target: 1,036,800 ledgers (about 60 days);
- storage tier: persistent only;
- missing entries: skipped, never created as a side effect of a read.

The registration read invokes the helper, so route checks, pair inspection,
and configuration validation refresh live pair slots. Pair writes also invoke
the helper after storing their value. This means a pair that is accessed but
not routed remains available for configuration and inspection, while an
unregistered pair still returns the documented default without leaving an
orphan slot.

TTL bumping is deliberately centralized. Callers must not use a different
threshold for one field, because that would make a single logical pair expire
partially and produce inconsistent reads. The helper checks `has` before
`extend_ttl` because Soroban does not treat an absent persistent key as a
renewable value. A transaction that writes a new key and then bumps it gets a
fresh TTL; a transaction that fails rolls back both the write and the bump.

## Layout compatibility tests

The namespace tests cover directionality, all nine slot-to-`DataKey` mappings,
stable diagnostic labels, config/history clearing rules, and TTL policy bounds.
Contract integration tests should additionally verify that:

1. `(USDC, EURC)` and `(EURC, USDC)` never share a slot;
2. a fee update does not alter liquidity or route metrics;
3. a read of a missing pair does not create storage;
4. a pair access extends TTL for an existing slot;
5. a pair write extends TTL for the newly created slot;
6. unregister clears configuration but preserves history;
7. purge removes only explicitly requested history;
8. a second schema migration is rejected;
9. a new release reads all pre-existing v1 keys unchanged.

These checks make the storage layout an explicit compatibility contract rather
than an accidental consequence of individual call sites.

## Operator runbook

When a pair is slow-moving, an operator can call a read-only pair getter to
renew the existing namespace entries. The getter does not register a pair and
does not write a missing key. If a pair has been archived already, use the
normal admin configuration or registration transaction to restore its state;
do not introduce a second spelling of the pair or a hand-built symbol key.

Before a release that changes storage code:

- compare the `DataKey` enum with the slot mapping table;
- run the layout completeness test and the pre-existing v1 test suite;
- confirm the threshold and extension target are unchanged;
- inspect the generated XDR for every existing `DataKey` variant;
- verify that reverse-direction pairs remain independent;
- exercise unregister and explicit metric purge separately;
- test an absent pair and confirm no persistent key is created;
- test a configured pair and confirm every present slot is bumped;
- record the schema version and migration decision in release notes.

The TTL helper is safe to call repeatedly. It extends only when the host says
the key is near its threshold, so regular reads do not keep increasing a key's
TTL without bound beyond the configured target. All slots in one namespace
share the same policy, which avoids partial archival where a dashboard sees a
fee from one epoch and metrics from another.

The typed `PoolSlot` ordinal is an audit convention, not a replacement for the
serialized `DataKey` discriminant. Keep the existing enum variant order and
field order stable. If a new pair slot is necessary, append its typed slot,
map it to a new `DataKey` variant, document its default and clear policy, add
one directionality test, and bump the storage schema only when a migration is
actually required. A refactor that continues to map to the old variant does
not need a migration.
