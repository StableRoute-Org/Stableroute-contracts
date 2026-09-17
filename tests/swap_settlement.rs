//! Public integration tests for Issue #455: reentrancy / double-application
//! guard on the pool swap settlement path.
//!
//! Validates once-only settlement guarantees, double-application rejections,
//! receipt verification, event emissions, and pair isolation through the
//! public `StableRouteRouterClient`.

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{symbol_short, Address, Env, TryFromVal};
use stableroute_contracts::{StableRouteRouter, StableRouteRouterClient, SwapError};

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
    client.register_pair(&symbol_short!("USDC"), &symbol_short!("EURC"));
    client.set_pair_fee_bps(&symbol_short!("USDC"), &symbol_short!("EURC"), &50);
    client.set_pair_liquidity(
        &admin,
        &symbol_short!("USDC"),
        &symbol_short!("EURC"),
        &1_000_000,
    );
    Fixture {
        env: env.clone(),
        client,
        admin,
    }
}

#[test]
fn public_settle_swap_returns_receipt_and_commits_settled_flag() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");
    let swap_id = 1001u64;

    assert!(!f.client.is_swap_settled(&swap_id));
    let receipt = f.client.settle_swap(&swap_id, &src, &dst, &10_000, &0);
    assert_eq!(receipt.fee, 50);
    assert_eq!(receipt.version, 1);
    assert!(f.client.is_swap_settled(&swap_id));
    assert_eq!(f.client.get_swap_version(&src, &dst), 1);
}

#[test]
fn second_settlement_of_same_swap_id_rejected_with_already_settled() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");
    let swap_id = 1002u64;

    let receipt = f.client.settle_swap(&swap_id, &src, &dst, &10_000, &0);
    assert_eq!(receipt.version, 1);

    // Attempting to settle the exact same swap_id with updated expected_version
    let err_updated_ver = f.client.try_settle_swap(&swap_id, &src, &dst, &10_000, &1);
    assert_eq!(err_updated_ver, Err(Ok(SwapError::AlreadySettled)));

    // Attempting to settle the exact same swap_id with stale expected_version
    let err_stale_ver = f.client.try_settle_swap(&swap_id, &src, &dst, &10_000, &0);
    assert_eq!(err_stale_ver, Err(Ok(SwapError::AlreadySettled)));

    // Ensure version did not advance past 1
    assert_eq!(f.client.get_swap_version(&src, &dst), 1);
}

#[test]
fn settle_pool_swap_alias_behaves_identically() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");
    let swap_id = 1003u64;

    let receipt = f.client.settle_pool_swap(&swap_id, &src, &dst, &5_000, &0);
    assert_eq!(receipt.fee, 25);
    assert_eq!(receipt.version, 1);
    assert!(f.client.is_swap_settled(&swap_id));

    let duplicate = f
        .client
        .try_settle_pool_swap(&swap_id, &src, &dst, &5_000, &1);
    assert_eq!(duplicate, Err(Ok(SwapError::AlreadySettled)));
}

#[test]
fn unrelated_swap_ids_settle_sequentially() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");

    let r1 = f.client.settle_swap(&2001, &src, &dst, &10_000, &0);
    assert_eq!(r1.version, 1);
    assert!(f.client.is_swap_settled(&2001));

    let r2 = f.client.settle_swap(&2002, &src, &dst, &10_000, &1);
    assert_eq!(r2.version, 2);
    assert!(f.client.is_swap_settled(&2002));

    assert_eq!(f.client.get_swap_version(&src, &dst), 2);
}

#[test]
fn cross_pair_settlements_are_isolated() {
    let f = fixture();
    let usdc = symbol_short!("USDC");
    let eurc = symbol_short!("EURC");
    let gbp = symbol_short!("GBP");

    f.client.register_pair(&eurc, &gbp);
    f.client.set_pair_fee_bps(&eurc, &gbp, &20);
    f.client.set_pair_liquidity(&f.admin, &eurc, &gbp, &500_000);

    let r1 = f.client.settle_swap(&3001, &usdc, &eurc, &10_000, &0);
    assert_eq!(r1.version, 1);

    // Swap ID 3002 on a different pair starts from version 0
    let r2 = f.client.settle_swap(&3002, &eurc, &gbp, &10_000, &0);
    assert_eq!(r2.version, 1);

    assert_eq!(f.client.get_swap_version(&usdc, &eurc), 1);
    assert_eq!(f.client.get_swap_version(&eurc, &gbp), 1);
}

#[test]
fn settlement_emits_settled_and_version_events() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");
    let swap_id = 4001u64;

    f.client.settle_swap(&swap_id, &src, &dst, &10_000, &0);
    let events = f.env.events().all();
    assert!(events.events().len() >= 2);

    let settled_event = &events.events()[events.events().len() - 2];
    let soroban_sdk::xdr::ContractEventBody::V0(settled_body) = &settled_event.body;
    let soroban_sdk::xdr::ScVal::Symbol(raw_settled_topic) = &settled_body.topics[0] else {
        panic!("expected symbol topic");
    };
    let settled_topic = soroban_sdk::Symbol::try_from_val(&f.env, raw_settled_topic).unwrap();
    assert_eq!(settled_topic, symbol_short!("settled"));

    let ver_event = &events.events()[events.events().len() - 1];
    let soroban_sdk::xdr::ContractEventBody::V0(ver_body) = &ver_event.body;
    let soroban_sdk::xdr::ScVal::Symbol(raw_ver_topic) = &ver_body.topics[0] else {
        panic!("expected symbol topic");
    };
    let ver_topic = soroban_sdk::Symbol::try_from_val(&f.env, raw_ver_topic).unwrap();
    assert_eq!(ver_topic, symbol_short!("swap_ver"));
}

#[test]
fn paused_and_unregistered_settlement_rejected_without_flag() {
    let f = fixture();
    let src = symbol_short!("USDC");
    let dst = symbol_short!("EURC");
    let unknown = symbol_short!("JPY");

    // Unregistered pair
    let unreg = f.client.try_settle_swap(&5001, &src, &unknown, &10_000, &0);
    assert_eq!(unreg, Err(Ok(SwapError::PairNotRegistered)));
    assert!(!f.client.is_swap_settled(&5001));

    // Contract paused
    f.client.pause();
    let paused = f.client.try_settle_swap(&5002, &src, &dst, &10_000, &0);
    assert_eq!(paused, Err(Ok(SwapError::ContractPaused)));
    assert!(!f.client.is_swap_settled(&5002));
}
