use super::*;
use crate::test::event_payloads;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Symbol,
};

fn setup(env: &Env) -> (StableRouteRouterClient<'_>, Address, Symbol, Symbol) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let id = env.register(StableRouteRouter, (admin.clone(),));
    let client = StableRouteRouterClient::new(env, &id);
    let source = symbol_short!("USDC");
    let destination = symbol_short!("EURC");
    client.register_pair(&source, &destination);
    (client, admin, source, destination)
}

fn params(fee_bps: u32, min_amount: i128, max_amount: i128) -> PairParameters {
    PairParameters {
        fee_bps,
        min_amount,
        max_amount,
    }
}

#[test]
fn admin_update_applies_all_values_and_exposes_one_snapshot() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    let next = params(125, 100, 50_000);

    client.set_pair_parameters(&source, &destination, &next);

    assert_eq!(client.get_pair_parameters(&source, &destination), next);
    let info = client.get_pair_info(&source, &destination);
    assert_eq!(info.fee_bps, 125);
    assert_eq!(info.min_amount, 100);
    assert_eq!(info.max_amount, 50_000);
}

#[test]
fn update_event_contains_old_and_new_parameter_snapshots() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    let next = params(25, 10, 1_000);

    client.set_pair_parameters(&source, &destination, &next);

    let payloads = event_payloads(&env, symbol_short!("param_set"));
    assert_eq!(payloads.len(), 1, "one successful update emits one event");
    let (_, _, old, new): (Symbol, Symbol, PairParameters, PairParameters) =
        soroban_sdk::TryFromVal::try_from_val(&env, &payloads[0]).unwrap();
    assert_eq!(old, params(0, 0, i128::MAX));
    assert_eq!(new, next);
}

#[test]
fn read_view_is_auth_free_and_returns_defaults_before_configuration() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);

    assert_eq!(
        client.get_pair_parameters(&source, &destination),
        params(0, 0, i128::MAX)
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn fee_above_bound_is_rejected_before_writes() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    client.set_pair_parameters(&source, &destination, &params(MAX_FEE_BPS + 1, 0, 100));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn negative_minimum_is_rejected() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    client.set_pair_parameters(&source, &destination, &params(10, -1, 100));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn non_positive_maximum_is_rejected() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    client.set_pair_parameters(&source, &destination, &params(10, 0, 0));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn inverted_amount_range_is_rejected() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    client.set_pair_parameters(&source, &destination, &params(10, 101, 100));
}

#[test]
fn failed_validation_leaves_the_previous_snapshot_and_emits_no_update() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    let initial = params(50, 10, 500);
    client.set_pair_parameters(&source, &destination, &initial);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_pair_parameters(&source, &destination, &params(60, 501, 500));
    }));

    assert!(result.is_err());
    assert_eq!(client.get_pair_parameters(&source, &destination), initial);
}

#[test]
#[should_panic]
fn unregistered_pair_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(StableRouteRouter, (admin,));
    let client = StableRouteRouterClient::new(&env, &id);
    client.set_pair_parameters(
        &symbol_short!("USDC"),
        &symbol_short!("EURC"),
        &params(10, 0, 100),
    );
}

#[test]
#[should_panic]
fn non_admin_cannot_update_parameters() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let id = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "__constructor",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    env.register_at(&id, StableRouteRouter, (admin,));
    let client = StableRouteRouterClient::new(&env, &id);
    client.set_pair_parameters(
        &symbol_short!("USDC"),
        &symbol_short!("EURC"),
        &params(10, 0, 100),
    );
}

#[test]
fn paused_router_rejects_combined_updates_without_event() {
    let env = Env::default();
    let (client, _admin, source, destination) = setup(&env);
    client.pause();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.set_pair_parameters(&source, &destination, &params(10, 0, 100));
    }));

    assert!(result.is_err());
    assert!(event_payloads(&env, symbol_short!("param_set")).is_empty());
}
