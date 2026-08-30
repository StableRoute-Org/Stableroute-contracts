//! Public integration checks for the emergency-stop boundary in issue #452.
//!
//! The router already exposes one shared pause guard. These tests deliberately
//! exercise it through the generated client from outside the library module so
//! the public ABI, read availability, transition events, and recovery path are
//! covered together.

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{symbol_short, Address, Env};
use stableroute_contracts::{StableRouteRouter, StableRouteRouterClient};

struct Fixture {
    env: Env,
    client: StableRouteRouterClient<'static>,
}

fn fixture() -> Fixture {
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register(StableRouteRouter, (admin,));
    let client = StableRouteRouterClient::new(env, &contract);
    client.register_pair(&symbol_short!("USDC"), &symbol_short!("EURC"));
    Fixture {
        env: env.clone(),
        client,
    }
}

#[test]
fn pause_round_trip_is_visible_through_public_read_api() {
    let f = fixture();
    assert!(!f.client.is_paused());
    f.client.pause();
    assert!(f.client.is_paused());
    f.client.unpause();
    assert!(!f.client.is_paused());
}

#[test]
fn quote_remains_available_while_mutating_route_is_blocked() {
    let f = fixture();
    f.client
        .set_pair_fee_bps(&symbol_short!("USDC"), &symbol_short!("EURC"), &100);
    f.client.pause();

    assert_eq!(
        f.client
            .quote_route(&symbol_short!("USDC"), &symbol_short!("EURC"), &1_000),
        (10, 990)
    );
    assert!(f
        .client
        .try_compute_route_fee(&symbol_short!("USDC"), &symbol_short!("EURC"), &1_000)
        .is_err());
    assert_eq!(f.client.get_total_routes_all_time(), 0);
}

#[test]
fn unpause_restores_the_route_mutation_boundary() {
    let f = fixture();
    f.client.pause();
    f.client.unpause();

    let fee = f
        .client
        .compute_route_fee(&symbol_short!("USDC"), &symbol_short!("EURC"), &1_000);
    assert_eq!(fee, 0);
    assert_eq!(f.client.get_total_routes_all_time(), 1);
}

#[test]
fn pause_transition_emits_an_event_for_each_state_change() {
    let f = fixture();
    let before = f.env.events().all().len();
    f.client.pause();
    f.client.unpause();
    let events = f.env.events().all();

    assert_eq!(events.len(), before + 2);
    let pause_event = events.get(before).unwrap();
    let unpause_event = events.get(before + 1).unwrap();
    assert_eq!(pause_event.1, (symbol_short!("paused"),));
    assert_eq!(unpause_event.1, (symbol_short!("paused"),));
}

#[test]
fn repeated_pause_and_unpause_are_safe_for_incident_automation() {
    let f = fixture();
    f.client.pause();
    f.client.pause();
    assert!(f.client.is_paused());
    f.client.unpause();
    f.client.unpause();
    assert!(!f.client.is_paused());
}

#[test]
fn pair_reads_remain_available_during_a_pause() {
    let f = fixture();
    f.client.pause();

    let info = f
        .client
        .get_pair_info(&symbol_short!("USDC"), &symbol_short!("EURC"));
    assert!(info.registered);
    assert_eq!(info.fee_bps, 0);
    assert!(f
        .client
        .is_pair_registered(&symbol_short!("USDC"), &symbol_short!("EURC")));
}

#[test]
fn paused_batch_mutation_fails_before_processing_entries() {
    let f = fixture();
    f.client.pause();
    let mut pairs = soroban_sdk::Vec::new(&f.env);
    pairs.push_back((symbol_short!("EURC"), symbol_short!("GBP")));

    assert!(f.client.try_register_pairs(&pairs).is_err());
    assert!(!f
        .client
        .is_pair_registered(&symbol_short!("EURC"), &symbol_short!("GBP")));
}

#[test]
fn state_remains_paused_after_a_rejected_mutation() {
    let f = fixture();
    f.client.pause();
    let _ = f
        .client
        .try_register_pair(&symbol_short!("EURC"), &symbol_short!("GBP"));

    assert!(f.client.is_paused());
    assert_eq!(f.client.get_total_routes_all_time(), 0);
}
