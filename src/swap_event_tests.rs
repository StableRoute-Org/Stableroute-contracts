#![cfg(test)]

//! Contract tests for the route event ABI.
//!
//! `compute_route_fee` is the router's state-changing swap-like operation.
//! These tests treat its existing `route` event as a public contract: every
//! successful route has exactly one route event, bounded liquidity produces
//! one additional `liq_used` event, and read-only or reverted calls produce
//! neither event. Keeping these assertions outside the implementation makes
//! accidental event duplication or payload drift visible during review.

use crate::test::event_payloads;
use crate::{StableRouteRouter, StableRouteRouterClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    xdr::{ContractEventBody, ScVal},
    Address, Env, Symbol, TryFromVal,
};
use std::vec;

fn setup_pair(
    env: &Env,
    source: Symbol,
    destination: Symbol,
) -> (StableRouteRouterClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(StableRouteRouter, (admin.clone(),));
    let client = StableRouteRouterClient::new(env, &contract_id);
    client.register_pair(&source, &destination);
    client.set_pair_fee_bps(&source, &destination, &100_u32);
    (client, admin)
}

fn route_events(env: &Env) -> std::vec::Vec<(Symbol, Symbol, i128)> {
    event_payloads(env, symbol_short!("route"))
        .iter()
        .map(|payload| {
            TryFromVal::try_from_val(env, payload)
                .expect("route payload has the documented tuple shape")
        })
        .collect()
}

fn liquidity_events(env: &Env) -> std::vec::Vec<(Symbol, Symbol, i128)> {
    event_payloads(env, symbol_short!("liq_used"))
        .iter()
        .map(|payload| {
            TryFromVal::try_from_val(env, payload)
                .expect("liq_used payload has the documented tuple shape")
        })
        .collect()
}

fn event_topics(env: &Env) -> std::vec::Vec<Symbol> {
    env.events()
        .all()
        .events()
        .iter()
        .map(|event| {
            let ContractEventBody::V0(body) = &event.body;
            assert_eq!(
                body.topics.len(),
                1,
                "route accounting events have one topic"
            );
            let ScVal::Symbol(topic) = &body.topics[0] else {
                panic!("route accounting topic is a Symbol");
            };
            Symbol::try_from_val(env, topic).expect("topic decodes to Symbol")
        })
        .collect()
}

#[test]
fn unbounded_route_emits_one_route_event_and_no_liquidity_event() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, _admin) = setup_pair(&env, source.clone(), destination.clone());

    assert_eq!(
        client.compute_route_fee(&source, &destination, &1_000_i128),
        10
    );
    assert_eq!(route_events(&env), vec![(source, destination, 1_000)]);
    assert!(liquidity_events(&env).is_empty());
}

#[test]
fn bounded_route_emits_liquidity_debit_before_route_event() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, admin) = setup_pair(&env, source.clone(), destination.clone());
    client.set_pair_liquidity(&admin, &source, &destination, &1_000_i128);

    client.compute_route_fee(&source, &destination, &250_i128);

    assert_eq!(
        route_events(&env),
        vec![(source.clone(), destination.clone(), 250)]
    );
    assert_eq!(
        liquidity_events(&env),
        vec![(source, destination, 750)],
        "bounded accounting exposes the post-debit balance exactly once"
    );
    assert_eq!(
        event_topics(&env),
        vec![symbol_short!("liq_used"), symbol_short!("route")]
    );
}

#[test]
fn consecutive_routes_have_one_payload_each_in_call_order() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, _admin) = setup_pair(&env, source.clone(), destination.clone());

    for amount in [100_i128, 200, 300] {
        client.compute_route_fee(&source, &destination, &amount);
        assert_eq!(
            route_events(&env),
            vec![(source.clone(), destination.clone(), amount)],
            "each transaction publishes exactly one fresh route payload"
        );
    }
    assert_eq!(liquidity_events(&env).len(), 0);
}

#[test]
fn batch_routes_emit_one_route_event_per_item_without_summary_duplicates() {
    let env = Env::default();
    let first_source = symbol_short!("USDC");
    let first_destination = symbol_short!("EURC");
    let second_source = symbol_short!("XLM");
    let second_destination = symbol_short!("GBP");
    let (client, _admin) = setup_pair(&env, first_source.clone(), first_destination.clone());
    client.register_pair(&second_source, &second_destination);
    client.set_pair_fee_bps(&second_source, &second_destination, &100_u32);

    let entries = soroban_sdk::vec![
        &env,
        (first_source.clone(), first_destination.clone(), 1_000_i128),
        (
            second_source.clone(),
            second_destination.clone(),
            2_000_i128
        ),
    ];
    assert_eq!(client.compute_route_fees(&entries).len(), 2);

    assert_eq!(
        event_topics(&env),
        vec![symbol_short!("route"), symbol_short!("route")]
    );
    assert_eq!(
        route_events(&env),
        vec![
            (first_source, first_destination, 1_000),
            (second_source, second_destination, 2_000),
        ]
    );
    assert!(liquidity_events(&env).is_empty());
}

#[test]
fn bounded_batch_exposes_each_debit_and_each_route_once() {
    let env = Env::default();
    let first_source = symbol_short!("USDC");
    let first_destination = symbol_short!("EURC");
    let second_source = symbol_short!("XLM");
    let second_destination = symbol_short!("GBP");
    let (client, admin) = setup_pair(&env, first_source.clone(), first_destination.clone());
    client.register_pair(&second_source, &second_destination);
    client.set_pair_fee_bps(&second_source, &second_destination, &100_u32);
    client.set_pair_liquidity(&admin, &first_source, &first_destination, &1_000_i128);
    client.set_pair_liquidity(&admin, &second_source, &second_destination, &2_000_i128);

    let entries = soroban_sdk::vec![
        &env,
        (first_source.clone(), first_destination.clone(), 250_i128),
        (second_source.clone(), second_destination.clone(), 750_i128),
    ];
    client.compute_route_fees(&entries);

    assert_eq!(
        event_topics(&env),
        vec![
            symbol_short!("liq_used"),
            symbol_short!("route"),
            symbol_short!("liq_used"),
            symbol_short!("route"),
        ]
    );
    assert_eq!(route_events(&env).len(), 2);
    assert_eq!(liquidity_events(&env).len(), 2);
    assert_eq!(
        liquidity_events(&env),
        vec![
            (first_source, first_destination, 750),
            (second_source, second_destination, 1_250),
        ]
    );
}

#[test]
fn quote_route_is_read_only_and_emits_no_swap_events() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, _admin) = setup_pair(&env, source.clone(), destination.clone());

    assert_eq!(
        client.quote_route(&source, &destination, &1_000_i128),
        (10, 990)
    );
    assert_eq!(client.get_total_routes_all_time(), 0);
    assert!(route_events(&env).is_empty());
    assert!(liquidity_events(&env).is_empty());
}

#[test]
fn failed_route_rolls_back_state_and_events() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, admin) = setup_pair(&env, source.clone(), destination.clone());
    client.set_pair_liquidity(&admin, &source, &destination, &500_i128);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.compute_route_fee(&source, &destination, &501_i128);
    }));

    assert!(result.is_err());
    assert_eq!(client.get_total_routes_all_time(), 0);
    assert_eq!(client.get_pair_liquidity(&source, &destination), 500);
    assert!(route_events(&env).is_empty());
    assert!(liquidity_events(&env).is_empty());
}

#[test]
fn route_payload_preserves_direction_and_exact_amount() {
    let env = Env::default();
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    let (client, _admin) = setup_pair(&env, source.clone(), destination.clone());

    client.compute_route_fee(&source, &destination, &i128::MAX);

    let events = route_events(&env);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, source);
    assert_eq!(events[0].1, destination);
    assert_eq!(events[0].2, i128::MAX);
}
