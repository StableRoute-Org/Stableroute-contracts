#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, vec, Address, Env};

use crate::{StableRouteRouter, StableRouteRouterClient, MAX_BATCH_SIZE};

fn setup() -> (Env, StableRouteRouterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(StableRouteRouter, (admin.clone(),));
    let client = StableRouteRouterClient::new(&env, &id);
    (env, client, admin)
}

fn register(
    client: &StableRouteRouterClient<'_>,
    source: &soroban_sdk::Symbol,
    destination: &soroban_sdk::Symbol,
) {
    client.register_pair(source, destination);
    client.set_pair_fee_bps(source, destination, &100);
}

#[test]
fn valid_batch_returns_one_fee_per_entry_and_updates_metrics() {
    let (_env, client, _admin) = setup();
    register(&client, &symbol_short!("USDC"), &symbol_short!("EURC"));
    register(&client, &symbol_short!("USDC"), &symbol_short!("GBP"));
    let entries = vec![
        &_env,
        (symbol_short!("USDC"), symbol_short!("EURC"), 1_000_000i128),
        (symbol_short!("USDC"), symbol_short!("GBP"), 2_000_000i128),
    ];
    let fees = client.compute_route_fees(&entries);
    assert_eq!(fees, vec![&_env, 10_000i128, 20_000i128]);
    assert_eq!(client.get_total_routes_all_time(), 2);
    assert_eq!(
        client.get_pair_route_count(&symbol_short!("USDC"), &symbol_short!("EURC")),
        1
    );
    assert_eq!(
        client.get_pair_route_count(&symbol_short!("USDC"), &symbol_short!("GBP")),
        1
    );
}

#[test]
fn invalid_item_is_rejected_before_any_effect() {
    let (_env, client, _admin) = setup();
    register(&client, &symbol_short!("USDC"), &symbol_short!("EURC"));
    let entries = vec![
        &_env,
        (symbol_short!("USDC"), symbol_short!("EURC"), 1_000_000i128),
        (
            symbol_short!("MISSING"),
            symbol_short!("EURC"),
            1_000_000i128,
        ),
    ];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.compute_route_fees(&entries)
    }));
    assert!(result.is_err());
    assert_eq!(client.get_total_routes_all_time(), 0);
    assert_eq!(
        client.get_pair_route_count(&symbol_short!("USDC"), &symbol_short!("EURC")),
        0
    );
}

#[test]
fn empty_and_oversized_batches_have_typed_failures() {
    let (env, client, _admin) = setup();
    let empty = vec![&env];
    let empty_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.compute_route_fees(&empty)
    }));
    assert!(empty_result.is_err());
    let mut entries = vec![&env];
    for _ in 0..=MAX_BATCH_SIZE {
        entries.push_back((symbol_short!("X"), symbol_short!("Y"), 1));
    }
    let large_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.compute_route_fees(&entries)
    }));
    assert!(large_result.is_err());
}

#[test]
fn non_positive_amount_is_rejected_without_route_counter_changes() {
    let (_env, client, _admin) = setup();
    register(&client, &symbol_short!("USDC"), &symbol_short!("EURC"));
    let entries = vec![&_env, (symbol_short!("USDC"), symbol_short!("EURC"), 0i128)];
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.compute_route_fees(&entries)
    }));
    assert!(result.is_err());
    assert_eq!(client.get_total_routes_all_time(), 0);
}

#[test]
fn batch_preserves_single_route_fee_policy() {
    let (_env, client, _admin) = setup();
    register(&client, &symbol_short!("USDC"), &symbol_short!("EURC"));
    client.set_pair_min_amount(&symbol_short!("USDC"), &symbol_short!("EURC"), &500);
    client.set_pair_max_amount(&symbol_short!("USDC"), &symbol_short!("EURC"), &2_000_000);
    let entries = vec![
        &_env,
        (symbol_short!("USDC"), symbol_short!("EURC"), 1_000_000i128),
    ];
    assert_eq!(client.compute_route_fees(&entries), vec![&_env, 10_000i128]);
}
