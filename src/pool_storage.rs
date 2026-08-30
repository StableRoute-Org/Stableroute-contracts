//! Centralized namespaced storage policy for pool/pair state.
//!
//! The router's original `DataKey` variants are part of the deployed layout
//! and therefore cannot be replaced during a routine maintenance release.
//! This module gives those variants one typed namespace and one TTL policy.
//! `key_for` deliberately maps to the existing variants, preserving their
//! XDR layout while preventing call sites from inventing ad-hoc keys.

use crate::DataKey;
use soroban_sdk::{contracttype, storage::Persistent, Env, Symbol, Vec};

/// Persistent entries in a pair namespace.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolSlot {
    /// Pair registration flag.
    Registration,
    /// Relative fee in basis points.
    FeeBps,
    /// Minimum accepted route amount.
    MinAmount,
    /// Maximum accepted route amount.
    MaxAmount,
    /// Reported available liquidity.
    Liquidity,
    /// Last successful route timestamp.
    LastRouteAt,
    /// Number of successful routes.
    RouteCount,
    /// Cumulative routed volume.
    Volume,
    /// Minimum interval between routes.
    Cooldown,
}

/// Typed pair namespace. Source and destination order is significant.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolNamespace {
    pub source: Symbol,
    pub destination: Symbol,
}

/// A key descriptor useful for audits and layout tests.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolStorageKey {
    pub namespace: PoolNamespace,
    pub slot: PoolSlot,
}

/// TTL values for all pair-scoped persistent entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolTtlPolicy {
    /// Entries are extended when their remaining TTL reaches this threshold.
    pub threshold: u32,
    /// Entries are renewed to at least this many ledgers.
    pub extend_to: u32,
}

/// Roughly 30 days at five seconds per ledger.
pub const PAIR_TTL_THRESHOLD: u32 = 518_400;
/// Roughly 60 days at five seconds per ledger.
pub const PAIR_TTL_EXTEND_TO: u32 = 1_036_800;

/// The one TTL policy used for pair configuration and metrics.
pub const PAIR_TTL_POLICY: PoolTtlPolicy = PoolTtlPolicy {
    threshold: PAIR_TTL_THRESHOLD,
    extend_to: PAIR_TTL_EXTEND_TO,
};

/// Number of pair-scoped slots in the current layout.
pub const POOL_SLOT_COUNT: u32 = 9;

/// Construct an immutable namespace descriptor.
pub fn namespace(source: Symbol, destination: Symbol) -> PoolNamespace {
    PoolNamespace {
        source,
        destination,
    }
}

/// Construct an audit descriptor for a pair slot.
pub fn descriptor(source: Symbol, destination: Symbol, slot: PoolSlot) -> PoolStorageKey {
    PoolStorageKey {
        namespace: namespace(source, destination),
        slot,
    }
}

/// Map the typed descriptor to the deployed `DataKey` layout.
pub fn key_for(key: &PoolStorageKey) -> DataKey {
    let source = key.namespace.source.clone();
    let destination = key.namespace.destination.clone();
    match key.slot {
        PoolSlot::Registration => DataKey::Pair(source, destination),
        PoolSlot::FeeBps => DataKey::PairFeeBps(source, destination),
        PoolSlot::MinAmount => DataKey::PairMinAmount(source, destination),
        PoolSlot::MaxAmount => DataKey::PairMaxAmount(source, destination),
        PoolSlot::Liquidity => DataKey::PairLiquidity(source, destination),
        PoolSlot::LastRouteAt => DataKey::PairLastRouteAt(source, destination),
        PoolSlot::RouteCount => DataKey::PairRouteCount(source, destination),
        PoolSlot::Volume => DataKey::PairVolume(source, destination),
        PoolSlot::Cooldown => DataKey::PairCooldown(source, destination),
    }
}

/// Return every pair key in canonical layout order.
pub fn pair_descriptors(env: &Env, source: Symbol, destination: Symbol) -> Vec<PoolStorageKey> {
    let mut result = Vec::new(env);
    let slots = [
        PoolSlot::Registration,
        PoolSlot::FeeBps,
        PoolSlot::MinAmount,
        PoolSlot::MaxAmount,
        PoolSlot::Liquidity,
        PoolSlot::LastRouteAt,
        PoolSlot::RouteCount,
        PoolSlot::Volume,
        PoolSlot::Cooldown,
    ];
    for slot in slots {
        result.push_back(descriptor(source.clone(), destination.clone(), slot));
    }
    result
}

/// Return the canonical slot order used by layout snapshots and audits.
pub fn all_slots(env: &Env) -> Vec<PoolSlot> {
    let mut result = Vec::new(env);
    for slot in [
        PoolSlot::Registration,
        PoolSlot::FeeBps,
        PoolSlot::MinAmount,
        PoolSlot::MaxAmount,
        PoolSlot::Liquidity,
        PoolSlot::LastRouteAt,
        PoolSlot::RouteCount,
        PoolSlot::Volume,
        PoolSlot::Cooldown,
    ] {
        result.push_back(slot);
    }
    result
}

/// Return the append-only ordinal assigned to a slot in the audit layout.
pub fn slot_index(slot: PoolSlot) -> u32 {
    match slot {
        PoolSlot::Registration => 0,
        PoolSlot::FeeBps => 1,
        PoolSlot::MinAmount => 2,
        PoolSlot::MaxAmount => 3,
        PoolSlot::Liquidity => 4,
        PoolSlot::LastRouteAt => 5,
        PoolSlot::RouteCount => 6,
        PoolSlot::Volume => 7,
        PoolSlot::Cooldown => 8,
    }
}

/// Extend every existing pair entry without creating missing storage slots.
///
/// The `has` check is important: a read of an unregistered pair must remain a
/// read and must not create an orphan persistent entry. A later registration
/// writes the key and the next bump renews it under this same policy.
pub fn bump_pair_ttl(env: &Env, source: &Symbol, destination: &Symbol) {
    let descriptors = pair_descriptors(env, source.clone(), destination.clone());
    let persistent = env.storage().persistent();
    for descriptor in descriptors.iter() {
        let key = key_for(&descriptor);
        bump_key_ttl_with_storage(&persistent, &key);
    }
}

/// Extend one existing slot. Hot call sites use this narrower helper to keep
/// batch registration gas bounded; read surfaces can refresh only the field
/// they touch, while a maintenance pass can call `bump_pair_ttl` for all.
pub fn bump_key_ttl(env: &Env, key: &DataKey) {
    let persistent = env.storage().persistent();
    bump_key_ttl_with_storage(&persistent, key);
}

fn bump_key_ttl_with_storage(storage: &Persistent, key: &DataKey) {
    if storage.has(key) {
        storage.extend_ttl(key, PAIR_TTL_POLICY.threshold, PAIR_TTL_POLICY.extend_to);
    }
}

/// Return a stable, human-readable slot label for off-chain diagnostics.
///
/// Labels are not storage keys and must never be used as a substitute for
/// `key_for`; they exist to make audit logs and layout snapshots legible.
pub fn slot_label(slot: PoolSlot) -> &'static str {
    match slot {
        PoolSlot::Registration => "pair.registration",
        PoolSlot::FeeBps => "pair.fee_bps",
        PoolSlot::MinAmount => "pair.min_amount",
        PoolSlot::MaxAmount => "pair.max_amount",
        PoolSlot::Liquidity => "pair.liquidity",
        PoolSlot::LastRouteAt => "pair.last_route_at",
        PoolSlot::RouteCount => "pair.route_count",
        PoolSlot::Volume => "pair.volume",
        PoolSlot::Cooldown => "pair.cooldown",
    }
}

/// Return whether the slot contains operational history rather than config.
pub fn is_metric_slot(slot: PoolSlot) -> bool {
    matches!(
        slot,
        PoolSlot::LastRouteAt | PoolSlot::RouteCount | PoolSlot::Volume
    )
}

/// Return whether a slot is live configuration rather than history.
pub fn is_config_slot(slot: PoolSlot) -> bool {
    !is_metric_slot(slot)
}

/// Validate the invariants required before adding a slot to the layout.
pub fn layout_is_complete(env: &Env, source: Symbol, destination: Symbol) -> bool {
    let slots = all_slots(env);
    if slots.len() != POOL_SLOT_COUNT {
        return false;
    }
    let descriptors = pair_descriptors(env, source, destination);
    if descriptors.len() != POOL_SLOT_COUNT {
        return false;
    }
    for slot in slots.iter() {
        if slot_index(slot) >= POOL_SLOT_COUNT || slot_label(slot).is_empty() {
            return false;
        }
    }
    true
}

/// Return whether the slot is cleared when a pair is unregistered.
pub fn is_cleared_on_unregister(slot: PoolSlot) -> bool {
    matches!(
        slot,
        PoolSlot::Registration
            | PoolSlot::FeeBps
            | PoolSlot::MinAmount
            | PoolSlot::MaxAmount
            | PoolSlot::Liquidity
            | PoolSlot::Cooldown
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::symbol_short;

    fn pair(slot: PoolSlot) -> PoolStorageKey {
        descriptor(symbol_short!("USDC"), symbol_short!("EURC"), slot)
    }

    #[test]
    fn namespace_is_directional() {
        let forward = namespace(symbol_short!("USDC"), symbol_short!("EURC"));
        let reverse = namespace(symbol_short!("EURC"), symbol_short!("USDC"));
        assert_ne!(forward, reverse);
    }

    #[test]
    fn every_slot_maps_to_a_pair_variant() {
        assert_eq!(
            key_for(&pair(PoolSlot::Registration)),
            DataKey::Pair(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::FeeBps)),
            DataKey::PairFeeBps(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::MinAmount)),
            DataKey::PairMinAmount(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::MaxAmount)),
            DataKey::PairMaxAmount(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::Liquidity)),
            DataKey::PairLiquidity(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::LastRouteAt)),
            DataKey::PairLastRouteAt(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::RouteCount)),
            DataKey::PairRouteCount(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::Volume)),
            DataKey::PairVolume(symbol_short!("USDC"), symbol_short!("EURC"))
        );
        assert_eq!(
            key_for(&pair(PoolSlot::Cooldown)),
            DataKey::PairCooldown(symbol_short!("USDC"), symbol_short!("EURC"))
        );
    }

    #[test]
    fn labels_are_unique_and_stable() {
        let labels = [
            slot_label(PoolSlot::Registration),
            slot_label(PoolSlot::FeeBps),
            slot_label(PoolSlot::MinAmount),
            slot_label(PoolSlot::MaxAmount),
            slot_label(PoolSlot::Liquidity),
            slot_label(PoolSlot::LastRouteAt),
            slot_label(PoolSlot::RouteCount),
            slot_label(PoolSlot::Volume),
            slot_label(PoolSlot::Cooldown),
        ];
        for (index, label) in labels.iter().enumerate() {
            assert!(!label.is_empty());
            assert!(!labels[index + 1..].contains(label));
        }
    }

    #[test]
    fn metric_slots_are_not_cleared_by_normal_unregister() {
        assert!(!is_cleared_on_unregister(PoolSlot::LastRouteAt));
        assert!(!is_cleared_on_unregister(PoolSlot::RouteCount));
        assert!(!is_cleared_on_unregister(PoolSlot::Volume));
        assert!(is_metric_slot(PoolSlot::Volume));
    }

    #[test]
    fn config_slots_are_cleared() {
        assert!(is_cleared_on_unregister(PoolSlot::Registration));
        assert!(is_cleared_on_unregister(PoolSlot::FeeBps));
        assert!(is_cleared_on_unregister(PoolSlot::Cooldown));
        assert!(!is_metric_slot(PoolSlot::Cooldown));
    }

    #[test]
    fn ttl_policy_has_a_safe_renewal_window() {
        assert!(PAIR_TTL_POLICY.threshold > 0);
        assert!(PAIR_TTL_POLICY.extend_to > PAIR_TTL_POLICY.threshold);
    }

    #[test]
    fn labels_identify_the_storage_namespace_without_being_keys() {
        let descriptor = pair(PoolSlot::FeeBps);
        assert_eq!(slot_label(descriptor.slot), "pair.fee_bps");
        assert_eq!(descriptor.namespace.source, symbol_short!("USDC"));
        assert_eq!(descriptor.namespace.destination, symbol_short!("EURC"));
    }

    #[test]
    fn canonical_slot_order_is_append_only() {
        let env = Env::default();
        let slots = all_slots(&env);
        assert_eq!(slots.len(), POOL_SLOT_COUNT);
        for (index, slot) in slots.iter().enumerate() {
            assert_eq!(slot_index(slot), index as u32);
        }
    }

    #[test]
    fn each_descriptor_has_the_same_namespace() {
        let env = Env::default();
        let descriptors = pair_descriptors(&env, symbol_short!("USDC"), symbol_short!("EURC"));
        assert_eq!(descriptors.len(), POOL_SLOT_COUNT);
        for descriptor in descriptors.iter() {
            assert_eq!(descriptor.namespace.source, symbol_short!("USDC"));
            assert_eq!(descriptor.namespace.destination, symbol_short!("EURC"));
        }
    }

    #[test]
    fn layout_completeness_rejects_no_current_slots() {
        let env = Env::default();
        assert!(layout_is_complete(
            &env,
            symbol_short!("USDC"),
            symbol_short!("EURC")
        ));
    }

    #[test]
    fn config_and_metric_partitions_cover_the_layout() {
        let env = Env::default();
        let mut config = 0;
        let mut metrics = 0;
        for slot in all_slots(&env).iter() {
            if is_config_slot(slot) {
                config += 1;
            }
            if is_metric_slot(slot) {
                metrics += 1;
            }
        }
        assert_eq!(config + metrics, POOL_SLOT_COUNT);
        assert_eq!(metrics, 3);
        assert_eq!(config, 6);
    }

    #[test]
    fn unregister_partition_matches_documented_history_policy() {
        let env = Env::default();
        for slot in all_slots(&env).iter() {
            assert_eq!(is_metric_slot(slot), !is_cleared_on_unregister(slot));
        }
    }

    #[test]
    fn all_slot_labels_are_namespaced() {
        let env = Env::default();
        for slot in all_slots(&env).iter() {
            assert!(slot_label(slot).starts_with("pair."));
        }
    }

    #[test]
    fn slot_ordinals_have_no_gaps() {
        let env = Env::default();
        let mut expected = 0;
        for slot in all_slots(&env).iter() {
            assert_eq!(slot_index(slot), expected);
            expected += 1;
        }
        assert_eq!(expected, POOL_SLOT_COUNT);
    }
}
