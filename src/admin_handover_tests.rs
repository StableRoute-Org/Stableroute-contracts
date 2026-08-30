#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

use crate::{StableRouteRouter, StableRouteRouterClient};

fn setup() -> (Env, StableRouteRouterClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending = Address::generate(&env);
    let id = env.register(StableRouteRouter, (admin.clone(),));
    let client = StableRouteRouterClient::new(&env, &id);
    (env, client, admin, pending)
}

#[test]
fn proposal_records_pending_admin_and_eta() {
    let (env, client, _admin, pending) = setup();
    client.set_timelock(&300);
    client.propose_admin_transfer(&pending);
    let info = client.get_pending_admin_info();
    assert_eq!(info.pending, Some(pending));
    assert_eq!(info.eta, Some(env.ledger().timestamp() + 300));
}

#[test]
fn pending_admin_cannot_accept_before_eta() {
    let (_env, client, _admin, pending) = setup();
    client.set_timelock(&300);
    client.propose_admin_transfer(&pending);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.accept_admin_transfer(&pending)
    }));
    assert!(result.is_err());
}

#[test]
fn pending_admin_accepts_at_eta_and_old_admin_loses_control() {
    let (env, client, admin, pending) = setup();
    client.set_timelock(&300);
    client.propose_admin_transfer(&pending);
    env.ledger().set_timestamp(env.ledger().timestamp() + 300);
    client.accept_admin_transfer(&pending);
    assert_eq!(client.get_admin(), Some(pending.clone()));
    assert_ne!(client.get_admin(), Some(admin));
}

#[test]
fn cancel_clears_pending_admin_and_emits_event() {
    let (_env, client, _admin, pending) = setup();
    client.propose_admin_transfer(&pending);
    client.cancel_admin_transfer();
    assert!(client.get_pending_admin().is_none());
    assert!(client.get_pending_admin_eta().is_none());
}

#[test]
fn replacement_proposal_overwrites_previous_pending_state() {
    let (_env, client, _admin, first) = setup();
    let second = Address::generate(&_env);
    client.set_timelock(&500);
    client.propose_admin_transfer(&first);
    client.propose_admin_transfer(&second);
    assert_eq!(client.get_pending_admin(), Some(second));
}

#[test]
fn self_and_current_admin_proposals_are_rejected() {
    let (_env, client, admin, _pending) = setup();
    let contract = client.address.clone();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.propose_admin_transfer(&admin)
    }))
    .is_err());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.propose_admin_transfer(&contract)
    }))
    .is_err());
}

#[test]
fn cancel_without_pending_is_safe_for_operators() {
    let (_env, client, _admin, _pending) = setup();
    client.cancel_admin_transfer();
    assert!(client.get_pending_admin_info().pending.is_none());
}
