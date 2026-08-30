//! Public-client integration coverage for issue #451.
//!
//! The unit tests in `src/lib.rs` exercise the implementation in-process. This
//! suite uses the generated contract client from outside the crate so the
//! versioned swap ABI, typed result, public read view, and event boundary are
//! covered as an integrator sees them.

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{symbol_short, Address, Env};
use stableroute_contracts::{StableRouteRouter, StableRouteRouterClient, SwapError, SwapReceipt};

struct Fixture {
    env: Env,
    client: StableRouteRouterClient<'static>,
    admin: Address,
}

fn fixture() -> Fixture {
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register_contract(None, StableRouteRouter);
    let client = StableRouteRouterClient::new(env, &contract);
    client.init(&admin);
    client.register_pair(&symbol_short!("USDC"), &symbol_short!("EURC"));
    Fixture {
        env: env.clone(),
        client,
        admin,
    }
}

#[test]
fn public_swap_returns_receipt_and_advances_version() {
    let f = fixture();
    let receipt = f
        .client
        .swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &1_000, &0);

    assert_eq!(receipt.fee, 0);
    assert_eq!(receipt.version, 1);
    assert_eq!(
        f.client
            .get_swap_version(&symbol_short!("USDC"), &symbol_short!("EURC")),
        1
    );
}

#[test]
fn stale_version_contains_current_version_in_typed_error() {
    let f = fixture();
    f.client
        .swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &100, &0);

    let error = f
        .client
        .try_swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &100, &0);
    assert_eq!(error, Err(Ok(SwapError::VersionConflict(1))));
}

#[test]
fn two_operations_from_one_base_allow_only_the_first() {
    let f = fixture();
    let first = f
        .client
        .try_swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &250, &0);
    let second = f
        .client
        .try_swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &500, &0);

    assert_eq!(first, Ok(Ok(SwapReceipt { fee: 0, version: 1 })));
    assert_eq!(second, Err(Ok(SwapError::VersionConflict(1))));
}

#[test]
fn versions_do_not_leak_between_pairs() {
    let f = fixture();
    f.client
        .register_pair(&symbol_short!("EURC"), &symbol_short!("GBP"));
    f.client
        .swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &100, &0);

    assert_eq!(
        f.client
            .get_swap_version(&symbol_short!("EURC"), &symbol_short!("GBP")),
        0
    );
    assert!(f
        .client
        .try_swap(&symbol_short!("EURC"), &symbol_short!("GBP"), &100, &0,)
        .is_ok());
}

#[test]
fn successful_swap_emits_version_transition_event() {
    let f = fixture();
    let before = f.env.events().all().len();
    f.client
        .swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &100, &0);
    let events = f.env.events().all();

    assert_eq!(events.len(), before + 2);
    let last = events.last().unwrap();
    assert_eq!(last.1, (symbol_short!("swap_ver"),));
}

#[test]
fn paused_swap_is_rejected_without_advancing_version() {
    let f = fixture();
    f.client.pause();

    assert_eq!(
        f.client
            .try_swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &100, &0,),
        Err(Ok(SwapError::ContractPaused))
    );
    assert_eq!(
        f.client
            .get_swap_version(&symbol_short!("USDC"), &symbol_short!("EURC")),
        0
    );
}

#[test]
fn invalid_input_is_rejected_before_version_write() {
    let f = fixture();
    assert_eq!(
        f.client
            .try_swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &0, &0,),
        Err(Ok(SwapError::AmountMustBePositive))
    );
    assert_eq!(
        f.client
            .get_swap_version(&symbol_short!("USDC"), &symbol_short!("EURC")),
        0
    );
}

#[test]
fn unregistered_pair_is_rejected_before_version_write() {
    let f = fixture();
    assert_eq!(
        f.client
            .try_swap(&symbol_short!("GBP"), &symbol_short!("EURC"), &100, &0,),
        Err(Ok(SwapError::PairNotRegistered))
    );
    assert_eq!(
        f.client
            .get_swap_version(&symbol_short!("GBP"), &symbol_short!("EURC")),
        0
    );
}

#[test]
fn fee_and_liquidity_accounting_remain_atomic_with_version_bump() {
    let f = fixture();
    f.client
        .set_pair_fee_bps(&symbol_short!("USDC"), &symbol_short!("EURC"), &100);
    f.client.set_pair_liquidity(
        &f.admin,
        &symbol_short!("USDC"),
        &symbol_short!("EURC"),
        &10_000,
    );

    let receipt = f
        .client
        .swap(&symbol_short!("USDC"), &symbol_short!("EURC"), &1_000, &0);
    assert_eq!(receipt.fee, 10);
    assert_eq!(receipt.version, 1);
    assert_eq!(
        f.client
            .get_pair_liquidity(&symbol_short!("USDC"), &symbol_short!("EURC")),
        9_000
    );
}
