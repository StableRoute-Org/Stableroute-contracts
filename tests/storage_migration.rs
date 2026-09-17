//! Integration test coverage for schema-versioned pool storage migration (#460).
//!
//! Exercises `get_schema_version`, `migrate`, idempotency, event emission,
//! and storage invariant retention through the public contract client.

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{symbol_short, Address, Env, Symbol};
use stableroute_contracts::{StableRouteRouter, StableRouteRouterClient};

struct Fixture {
    env: Env,
    client: StableRouteRouterClient<'static>,
    admin: Address,
}

fn fixture() -> Fixture {
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register(StableRouteRouter, (admin.clone(),));
    let client = StableRouteRouterClient::new(env, &contract);
    Fixture {
        env: env.clone(),
        client,
        admin,
    }
}

#[test]
fn schema_version_defaults_to_one_before_migration() {
    let f = fixture();
    assert_eq!(f.client.get_schema_version(), 1);
}

#[test]
fn migrate_advances_schema_version_to_two_and_emits_event() {
    use soroban_sdk::TryFromVal;
    let f = fixture();

    assert_eq!(f.client.get_schema_version(), 1);
    f.client.migrate();

    let events = f.env.events().all();
    let evs = events.events();
    assert_eq!(evs.len(), 1);

    let soroban_sdk::xdr::ContractEventBody::V0(body) = &evs[0].body;
    let soroban_sdk::xdr::ScVal::Symbol(raw_topic) = &body.topics[0] else {
        panic!("expected symbol topic");
    };
    let topic = Symbol::try_from_val(&f.env, raw_topic).unwrap();
    assert_eq!(topic, symbol_short!("migrated"));

    let soroban_sdk::xdr::ScVal::Vec(Some(sc_vec)) = &body.data else {
        panic!("expected vec in body.data");
    };
    assert_eq!(sc_vec.len(), 2);
    assert_eq!(sc_vec[0], soroban_sdk::xdr::ScVal::U32(1));
    assert_eq!(sc_vec[1], soroban_sdk::xdr::ScVal::U32(2));

    assert_eq!(f.client.get_schema_version(), 2);
}

#[test]
fn migrate_is_idempotent() {
    let f = fixture();
    f.client.migrate();
    assert_eq!(f.env.events().all().events().len(), 1);
    assert_eq!(f.client.get_schema_version(), 2);

    // Second call is a safe no-op
    f.client.migrate();
    assert_eq!(f.env.events().all().events().len(), 0);
    assert_eq!(f.client.get_schema_version(), 2);
}

#[test]
fn storage_invariants_preserved_across_migration() {
    let f = fixture();
    let usdc = symbol_short!("USDC");
    let eurc = symbol_short!("EURC");

    f.client.register_pair(&usdc, &eurc);
    f.client.set_pair_fee_bps(&usdc, &eurc, &25);
    f.client.set_pair_min_amount(&usdc, &eurc, &100);
    f.client.set_pair_max_amount(&usdc, &eurc, &1_000_000);
    f.client
        .set_pair_liquidity(&f.admin, &usdc, &eurc, &500_000);

    // Run migration
    f.client.migrate();

    // Verify all pair settings and routes function identically post-migration
    assert_eq!(f.client.get_schema_version(), 2);
    assert!(f.client.is_pair_registered(&usdc, &eurc));
    assert_eq!(f.client.get_pair_fee_bps(&usdc, &eurc), 25);
    assert_eq!(f.client.get_pair_min_amount(&usdc, &eurc), 100);
    assert_eq!(f.client.get_pair_max_amount(&usdc, &eurc), 1_000_000);
    assert_eq!(f.client.get_pair_liquidity(&usdc, &eurc), 500_000);

    let fee = f.client.compute_route_fee(&usdc, &eurc, &10_000);
    assert_eq!(fee, 25);
    assert_eq!(f.client.get_pair_liquidity(&usdc, &eurc), 490_000);
}
