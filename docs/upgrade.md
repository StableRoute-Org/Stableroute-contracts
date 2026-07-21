# StableRoute — Upgrade & Migration Runbook

Operational reference for upgrading deployed StableRoute contracts and
applying schema migrations. The recommended deployment sequence is:

1. Pause the contract.
2. Upgrade the contract WASM.
3. Verify the deployment.
4. Execute the required migration.
5. Verify the schema version.
6. Unpause the contract.

Maintaining this order prevents schema version mismatches and reduces
operational risk during deployments.

## Upgrade sequence

| Step | Action | Verification |
|------|--------|--------------|
| 1 | Pause the contract | Contract reports paused. |
| 2 | Upgrade the contract WASM | Upgrade completes successfully. |
| 3 | Verify deployment | New contract version is active. |
| 4 | Execute `migrate_v1_to_v2` | Migration completes without error. |
| 5 | Call `get_schema_version` | Returns the expected schema version. |
| 6 | Unpause the contract | Normal operation resumes. |

The upgrade operation is intentionally **not** gated by the paused state,
allowing emergency WASM deployments while the contract remains paused.

## Schema version

`migrate_v1_to_v2` updates the stored schema version from **1** to **2**.
The migration validates the current schema version before applying any
changes and rejects unexpected starting versions.

| Stage | Expected `get_schema_version` |
|-------|-------------------------------|
| Before migration | `1` |
| After migration | `2` |

## CLI examples

Replace `<CONTRACT_ID>` and `<WASM_HASH>` with deployment-specific values.

### Pause

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --fn pause
```

### Upgrade

```bash
stellar contract upgrade \
  --id <CONTRACT_ID> \
  --wasm-hash <WASM_HASH>
```

### Migrate

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --fn migrate_v1_to_v2
```

### Verify schema version

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --fn get_schema_version
```

Expected output:

- Before migration: `1`
- After migration: `2`

### Unpause

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --fn unpause
```

## Rollback

Keep the contract paused until verification completes.

- **Upgrade failure before migration:** Redeploy the previous WASM. No schema changes have been applied.
- **Migration failure:** The migration validates the current schema version before updating state. If validation fails, the schema version remains unchanged.
- **Post-migration issues:** Verify `get_schema_version`, redeploy the appropriate WASM if required, and only unpause after confirming the deployed contract and schema version are consistent.