# Pool Storage Migration & Schema Versioning Reference

Authoritative documentation for the schema-versioned storage migration and upgrade architecture in StableRoute smart contracts (#460).

---

## 1. Overview

As the pool storage model evolves across releases (e.g., introduction of typed namespaces, pair parameters, and TTL management), on-chain contract state must support safe, non-disruptive, schema-versioned migrations.

StableRoute implements a robust, idempotent storage migration path:
- **Current schema version query**: `get_schema_version(env: Env) -> u32` (defaults to `1` when uninitialized/unset).
- **Public migration entrypoint**: `migrate(env: Env)` upgrades the schema to target version `2`.
- **Legacy migration entrypoint**: `migrate_v1_to_v2(env: Env)` retained for backward compatibility.
- **Event emission**: Emits `migrated` event recording `(from_version, to_version)`.
- **TTL renewal**: Renews persistent storage TTL for migration keys via `bump_key_ttl`.

---

## 2. Architecture & Invariants

### Schema Versions

| Version | Status | Description |
|---|---|---|
| `1` | Default | Legacy unversioned storage layout. All historical pairs and routes operate in v1 layout. |
| `2` | Current Target | Schema-versioned pool storage with namespaced keys, pair TTL policies, and guarded entrypoints. |

### Entrypoint Specifications

#### `get_schema_version(env: Env) -> u32`
- **Auth**: None (public read-only query).
- **Behavior**: Reads persistent `DataKey::SchemaVersion`. Returns `1` if absent (the implicit default before any migration).
- **Side effects**: None. Does not alter storage or emit events.

#### `migrate(env: Env)`
- **Auth**: Admin (`require_admin`).
- **Validation**:
  - Requires admin authorization.
  - Reads current version `unwrap_or(1)`.
  - If `current == TARGET_SCHEMA_VERSION` (2), returns immediately as a safe no-op.
  - If `current != 1`, panics with `RouterError::MigrationVersionMismatch` (#13).
- **State Changes**:
  - Updates `DataKey::SchemaVersion` to `2`.
  - Extends persistent TTL on `DataKey::SchemaVersion` via `bump_key_ttl`.
- **Event**:
  - Topic: `symbol_short!("migrated")`
  - Payload: `(current: u32, target: u32)` -> `(1, 2)`
  - On idempotent re-runs, returns immediately without emitting duplicate events.

---

## 3. Storage Invariant Preservation

The migration process is designed to ensure zero data loss and continuous contract availability:
1. **Registered Pairs**: Existing pair configurations (`Pair`, `PairFeeBps`, `PairMinAmount`, `PairMaxAmount`, `PairLiquidity`, `PairCooldown`) remain completely valid and intact.
2. **Metrics & History**: Historical route metrics (`PairRouteCount`, `PairVolume`, `PairLastRouteAt`, `TotalRoutesAllTime`) are preserved across migration.
3. **Route Functionality**: Swaps and route calculations (`compute_route_fee`, `quote_route`) produce identical results before and after migration.

---

## 4. Operational Runbook

When deploying contract upgrades:

1. **Pause Contract**:
   ```bash
   stellar contract invoke --id <ROUTER_ID> --source admin --fn pause
   ```
2. **Upgrade WASM**:
   ```bash
   stellar contract upgrade --id <ROUTER_ID> --source admin --wasm-hash <NEW_WASM_HASH>
   ```
3. **Execute Migration**:
   ```bash
   stellar contract invoke --id <ROUTER_ID> --source admin --fn migrate
   ```
4. **Verify Schema Version**:
   ```bash
   stellar contract invoke --id <ROUTER_ID> --fn get_schema_version
   # Expected output: 2
   ```
5. **Verify Idempotency (Optional)**:
   ```bash
   stellar contract invoke --id <ROUTER_ID> --source admin --fn migrate
   # Succeeds cleanly with no error and no duplicate event
   ```
6. **Unpause Contract**:
   ```bash
   stellar contract invoke --id <ROUTER_ID> --source admin --fn unpause
   ```
