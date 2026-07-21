# Roles and Capability Boundaries

This contract separates authority between two independent roles:

- **Admin**
- **Oracle**

The design follows the principle of least privilege.

The admin manages contract configuration and governance.

The oracle only updates market liquidity data.

Neither role should be used for the other's responsibilities.

---

# Capability Matrix

| Entrypoint | Admin | Oracle | Public |
|------------|:-----:|:------:|:------:|
| version | | | ✅ |
| get_schema_version | | | ✅ |
| migrate_v1_to_v2 | Admin | | |
| init | Admin | | |
| is_paused | | | ✅ |
| pause | ✅ | | |
| unpause | ✅ | | |
| get_timelock | | | ✅ |
| set_timelock | ✅ | | |
| get_pending_admin_eta | | | ✅ |
| cancel_admin_transfer | ✅ | | |
| get_pending_admin | | | ✅ |
| get_pending_admin_info | | | ✅ |
| accept_admin_transfer | Pending Admin | | |
| propose_admin_transfer | ✅ | | |
| force_admin_transfer | ✅ | | |
| get_admin | | | ✅ |
| register_pair | ✅ | | |
| register_pairs | ✅ | | |
| is_pair_active | | | ✅ |
| get_pair_info | | | ✅ |
| get_pair_info_ext | | | ✅ |
| quote_route | | | ✅ |
| get_pair_last_route_at | | | ✅ |
| set_pair_cooldown | ✅ | | |
| get_pair_cooldown | | | ✅ |
| get_total_routes_all_time | | | ✅ |
| get_pair_route_count | | | ✅ |
| get_pair_volume | | | ✅ |
| set_fee_recipient | ✅ | | |
| get_fee_recipient | | | ✅ |
| get_max_fee_absolute | | | ✅ |
| set_max_fee_absolute | ✅ | | |
| get_pair_liquidity | | | ✅ |
| get_oracle | | | ✅ |
| set_oracle | ✅ | | |
| remove_oracle | ✅ | | |
| set_pair_liquidity | ✅ (authorization) | ✅ (authorization) | |
| get_pair_max_amount | | | ✅ |
| set_pair_max_amount | ✅ | | |
| get_pair_min_amount | | | ✅ |
| set_pair_min_amount | ✅ | | |
| unregister_pair | ✅ | | |
| purge_pair_metrics | ✅ | | |
| is_pair_registered | | | ✅ |
| set_pair_fee_bps | ✅ | | |
| set_pair_fees_bps | ✅ | | |
| get_pair_fee_bps | | | ✅ |
| compute_route_fee | | | ✅ |
| route_tag | | | ✅ |
| upgrade | ✅ | | |

---

# Role Responsibilities

## Admin

The admin controls governance and configuration.

Responsibilities include:

- Registering trading pairs
- Removing trading pairs
- Configuring limits
- Configuring fees
- Configuring cooldowns
- Configuring fee recipient
- Assigning or removing the oracle
- Pausing and unpausing the contract
- Upgrading the contract
- Managing admin transfer

The admin **does not directly publish liquidity values**.

---

## Oracle

The oracle has one operational responsibility:

- Publish liquidity values for registered pairs.

Liquidity updates require authorization from both:

- the configured oracle
- the contract admin

This dual-authorization model reduces the impact of a compromised oracle.

The oracle cannot:

- register pairs
- remove pairs
- pause the contract
- upgrade the contract
- modify fees
- change governance
- appoint another oracle

---

# Key Rotation

## Admin Rotation

1. Current admin calls `propose_admin_transfer`.
2. Wait for the configured timelock.
3. New admin calls `accept_admin_transfer`.
4. Verify `get_admin`.

If the current admin retains control during the timelock, the transfer may be cancelled.

---

## Oracle Rotation

1. Admin selects a replacement oracle.
2. Admin calls `set_oracle`.
3. Verify with `get_oracle`.
4. Begin submitting liquidity updates using the new oracle.

---

# Compromised Oracle Recovery

If an oracle key is suspected to be compromised:

1. Stop using the compromised key.
2. Admin calls `set_oracle` with a replacement.
3. Verify the new oracle using `get_oracle`.
4. Resume liquidity updates.

If necessary, the admin may temporarily remove the oracle using `remove_oracle`.

---

# Compromised Admin Recovery

If the admin key is compromised:

- Use the pending-admin workflow if available.
- If governance permits, perform an emergency admin transfer.
- If the compromised admin still has control, rotate immediately using the transfer functions.

Because the admin controls upgrades and configuration, compromise of this key represents the highest-risk scenario.

---

# Least Privilege Summary

The contract intentionally separates governance from operational data updates.

- Admin governs.
- Oracle reports liquidity.

Neither role should routinely perform the other's duties.

The dual-auth requirement on `set_pair_liquidity` provides an additional safeguard by requiring participation from both trusted parties.