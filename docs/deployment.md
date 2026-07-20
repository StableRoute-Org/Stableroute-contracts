# Deployment Guide

This document covers the deployment process for the StableRoute contracts, specifically highlighting the constructor-based deployment pattern and the legacy `init` trap.

## Constructor Deployment

StableRoute relies on the `__constructor` function to set the operational admin atomically at contract instantiation. This prevents any front-running vulnerabilities where an attacker could call an initializer before the legitimate deployer.

### Exact Deployment Command

To deploy the contract and initialize it with an admin, use the `stellar contract deploy` command, passing the `admin` argument to the constructor:

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/stableroute_contracts.wasm \
  --source admin \
  --network testnet \
  -- \
  --admin <G...ADMIN_ADDRESS>
```

*(Note: Replace `<G...ADMIN_ADDRESS>` with the actual public key of the admin account).*

## The Legacy `init` Trap

Older Soroban tutorials typically guide developers to deploy the contract first and then invoke an `init` function to configure it. 

In StableRoute, the `init` function has been intentionally neutered and **will unconditionally panic with `AlreadyInitialized` (Error Code 1)** if called.

**Why it panics:**
The `admin` role is securely provisioned by the `__constructor` at deploy time, meaning the contract is fully initialized the moment it touches the ledger.

**Why it exists:**
The `init` function remains strictly for **ABI compatibility** and to preserve historical semantics for older clients or scripts that might try to invoke it post-deployment.

## Post-Deploy Verification

After deploying the contract, verify its state to ensure the constructor executed correctly.

### 1. Verify the Admin
Check that the `get_admin` query returns the address passed during deployment:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source admin \
  -- \
  get_admin
```

### 2. Verify the Version
Ensure the contract returns the correct identity tag `ROUTER_V2`:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source admin \
  -- \
  version
```

### 3. Verify the Schema Version
Confirm the storage schema version defaults to `1`:
```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --network testnet \
  --source admin \
  -- \
  get_schema_version
```
