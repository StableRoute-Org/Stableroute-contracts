//! Exhaustive, stable error metadata for pool operations.
//!
//! `RouterError` is the ABI enum. This module adds the metadata needed by
//! clients and reviewers: a stable code, operation-independent category,
//! retry guidance, and whether the error can be resolved by configuration.
//! Adding metadata here does not change the serialized `RouterError` values.

use crate::RouterError;
use soroban_sdk::{contracttype, Env, Vec};

/// Broad class used by SDKs for consistent negative-path handling.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorClass {
    /// Caller input cannot satisfy the contract's preconditions.
    Input,
    /// Caller lacks the required role or signature.
    Authorization,
    /// Pair or contract state does not permit this operation.
    State,
    /// Amount, rate, or batch limit was exceeded.
    Limit,
    /// Governance or deployment lifecycle failure.
    Governance,
    /// Safety guard such as pause, reentrancy, or liquidity protection.
    Safety,
}

/// Stable symbolic name for a catalog entry.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorName {
    AlreadyInitialized,
    NotInitialized,
    SourceEqualsDestination,
    FeeBpsTooHigh,
    PairNotRegistered,
    AmountMustBePositive,
    NoPendingAdminTransfer,
    NotPendingAdmin,
    ContractPaused,
    AmountBelowMin,
    AmountAboveMax,
    InsufficientLiquidity,
    MigrationVersionMismatch,
    TimelockNotElapsed,
    ReentrantCall,
    NotAuthorized,
    RouteCooldownActive,
    BatchTooLarge,
    EmptyBatch,
    CooldownTooLarge,
    ZeroFeeCap,
    InvalidAdminAddress,
    InvalidParameterRange,
}

/// Metadata exposed by the read-only error catalog.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorDescriptor {
    /// Append-only numeric code from `RouterError`.
    pub code: u32,
    /// Stable machine-readable symbolic name.
    pub name: ErrorName,
    /// Handling category.
    pub class: ErrorClass,
    /// Whether retrying the same request can succeed without changing input.
    pub retryable: bool,
    /// Whether an administrator can normally resolve it by changing config.
    pub configuration_fix: bool,
}

/// Operations that have a typed failure contract at the pool boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolOperation {
    Register,
    ConfigureFee,
    ConfigureBounds,
    ConfigureLiquidity,
    ConfigureCooldown,
    Quote,
    Compute,
    Governance,
    Migration,
    Batch,
}

/// Map a stable error to the operation family it protects.
pub fn operation_for(error: RouterError) -> PoolOperation {
    match error {
        RouterError::AlreadyInitialized
        | RouterError::NotInitialized
        | RouterError::NoPendingAdminTransfer
        | RouterError::NotPendingAdmin
        | RouterError::TimelockNotElapsed
        | RouterError::InvalidAdminAddress => PoolOperation::Governance,
        RouterError::SourceEqualsDestination | RouterError::PairNotRegistered => {
            PoolOperation::Register
        }
        RouterError::FeeBpsTooHigh | RouterError::ZeroFeeCap => PoolOperation::ConfigureFee,
        RouterError::AmountMustBePositive
        | RouterError::AmountBelowMin
        | RouterError::AmountAboveMax
        | RouterError::InvalidParameterRange => PoolOperation::ConfigureBounds,
        RouterError::InsufficientLiquidity => PoolOperation::Compute,
        RouterError::MigrationVersionMismatch => PoolOperation::Migration,
        RouterError::ContractPaused | RouterError::ReentrantCall => PoolOperation::Compute,
        RouterError::NotAuthorized => PoolOperation::ConfigureLiquidity,
        RouterError::RouteCooldownActive => PoolOperation::Compute,
        RouterError::BatchTooLarge | RouterError::EmptyBatch => PoolOperation::Batch,
        RouterError::CooldownTooLarge => PoolOperation::ConfigureCooldown,
        RouterError::InvalidAdminAddress => PoolOperation::Governance,
    }
}

/// Return the complete append-only error list in numeric order.
pub const fn all_errors() -> [RouterError; 23] {
    [
        RouterError::AlreadyInitialized,
        RouterError::NotInitialized,
        RouterError::SourceEqualsDestination,
        RouterError::FeeBpsTooHigh,
        RouterError::PairNotRegistered,
        RouterError::AmountMustBePositive,
        RouterError::NoPendingAdminTransfer,
        RouterError::NotPendingAdmin,
        RouterError::ContractPaused,
        RouterError::AmountBelowMin,
        RouterError::AmountAboveMax,
        RouterError::InsufficientLiquidity,
        RouterError::MigrationVersionMismatch,
        RouterError::TimelockNotElapsed,
        RouterError::ReentrantCall,
        RouterError::NotAuthorized,
        RouterError::RouteCooldownActive,
        RouterError::BatchTooLarge,
        RouterError::EmptyBatch,
        RouterError::CooldownTooLarge,
        RouterError::ZeroFeeCap,
        RouterError::InvalidAdminAddress,
        RouterError::InvalidParameterRange,
    ]
}

/// Return the exact stable descriptor for a router error.
pub const fn descriptor(error: RouterError) -> ErrorDescriptor {
    let (name, class, retryable, configuration_fix) = match error {
        RouterError::AlreadyInitialized => (
            ErrorName::AlreadyInitialized,
            ErrorClass::Governance,
            false,
            false,
        ),
        RouterError::NotInitialized => (
            ErrorName::NotInitialized,
            ErrorClass::Governance,
            false,
            true,
        ),
        RouterError::SourceEqualsDestination => (
            ErrorName::SourceEqualsDestination,
            ErrorClass::Input,
            false,
            false,
        ),
        RouterError::FeeBpsTooHigh => (ErrorName::FeeBpsTooHigh, ErrorClass::Limit, false, true),
        RouterError::PairNotRegistered => {
            (ErrorName::PairNotRegistered, ErrorClass::State, false, true)
        }
        RouterError::AmountMustBePositive => (
            ErrorName::AmountMustBePositive,
            ErrorClass::Input,
            false,
            false,
        ),
        RouterError::NoPendingAdminTransfer => (
            ErrorName::NoPendingAdminTransfer,
            ErrorClass::Governance,
            false,
            true,
        ),
        RouterError::NotPendingAdmin => (
            ErrorName::NotPendingAdmin,
            ErrorClass::Authorization,
            false,
            false,
        ),
        RouterError::ContractPaused => (ErrorName::ContractPaused, ErrorClass::Safety, true, true),
        RouterError::AmountBelowMin => (ErrorName::AmountBelowMin, ErrorClass::Limit, false, true),
        RouterError::AmountAboveMax => (ErrorName::AmountAboveMax, ErrorClass::Limit, false, true),
        RouterError::InsufficientLiquidity => (
            ErrorName::InsufficientLiquidity,
            ErrorClass::Safety,
            true,
            true,
        ),
        RouterError::MigrationVersionMismatch => (
            ErrorName::MigrationVersionMismatch,
            ErrorClass::Governance,
            false,
            true,
        ),
        RouterError::TimelockNotElapsed => (
            ErrorName::TimelockNotElapsed,
            ErrorClass::Governance,
            true,
            false,
        ),
        RouterError::ReentrantCall => (ErrorName::ReentrantCall, ErrorClass::Safety, true, false),
        RouterError::NotAuthorized => (
            ErrorName::NotAuthorized,
            ErrorClass::Authorization,
            false,
            true,
        ),
        RouterError::RouteCooldownActive => (
            ErrorName::RouteCooldownActive,
            ErrorClass::Safety,
            true,
            true,
        ),
        RouterError::BatchTooLarge => (ErrorName::BatchTooLarge, ErrorClass::Limit, false, true),
        RouterError::EmptyBatch => (ErrorName::EmptyBatch, ErrorClass::Input, false, false),
        RouterError::CooldownTooLarge => {
            (ErrorName::CooldownTooLarge, ErrorClass::Limit, false, true)
        }
        RouterError::ZeroFeeCap => (ErrorName::ZeroFeeCap, ErrorClass::Input, false, true),
        RouterError::InvalidAdminAddress => (
            ErrorName::InvalidAdminAddress,
            ErrorClass::Governance,
            false,
            false,
        ),
        RouterError::InvalidParameterRange => (
            ErrorName::InvalidParameterRange,
            ErrorClass::Input,
            false,
            true,
        ),
    };
    ErrorDescriptor {
        code: error as u32,
        name,
        class,
        retryable,
        configuration_fix,
    }
}

/// Look up an error by its wire code without exposing a generic failure.
pub const fn from_code(code: u32) -> Option<RouterError> {
    match code {
        1 => Some(RouterError::AlreadyInitialized),
        2 => Some(RouterError::NotInitialized),
        3 => Some(RouterError::SourceEqualsDestination),
        4 => Some(RouterError::FeeBpsTooHigh),
        5 => Some(RouterError::PairNotRegistered),
        6 => Some(RouterError::AmountMustBePositive),
        7 => Some(RouterError::NoPendingAdminTransfer),
        8 => Some(RouterError::NotPendingAdmin),
        9 => Some(RouterError::ContractPaused),
        10 => Some(RouterError::AmountBelowMin),
        11 => Some(RouterError::AmountAboveMax),
        12 => Some(RouterError::InsufficientLiquidity),
        13 => Some(RouterError::MigrationVersionMismatch),
        14 => Some(RouterError::TimelockNotElapsed),
        15 => Some(RouterError::ReentrantCall),
        16 => Some(RouterError::NotAuthorized),
        17 => Some(RouterError::RouteCooldownActive),
        18 => Some(RouterError::BatchTooLarge),
        19 => Some(RouterError::EmptyBatch),
        20 => Some(RouterError::CooldownTooLarge),
        21 => Some(RouterError::ZeroFeeCap),
        22 => Some(RouterError::InvalidAdminAddress),
        23 => Some(RouterError::InvalidParameterRange),
        _ => None,
    }
}

/// Return the catalog as a read-only Soroban value for client discovery.
pub fn catalog(env: &Env) -> Vec<ErrorDescriptor> {
    let mut result = Vec::new(env);
    for error in all_errors() {
        result.push_back(descriptor(error));
    }
    result
}

/// Return the retry recommendation without requiring clients to unpack metadata.
pub const fn should_retry(error: RouterError) -> bool {
    descriptor(error).retryable
}

/// Return whether an administrator action is a plausible remediation.
pub const fn has_configuration_fix(error: RouterError) -> bool {
    descriptor(error).configuration_fix
}

/// Return only errors associated with one operation family.
pub fn catalog_for_operation(env: &Env, operation: PoolOperation) -> Vec<ErrorDescriptor> {
    let mut result = Vec::new(env);
    for error in all_errors() {
        if operation_for(error) == operation {
            result.push_back(descriptor(error));
        }
    }
    result
}

/// Return the number of errors in one class. Useful for catalog sanity checks.
pub fn class_count(class: ErrorClass) -> u32 {
    let errors = all_errors();
    let mut count = 0;
    let mut index = 0;
    while index < errors.len() {
        if descriptor(errors[index]).class == class {
            count += 1;
        }
        index += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_contiguous_and_append_only() {
        let errors = all_errors();
        for (index, error) in errors.iter().enumerate() {
            assert_eq!(descriptor(*error).code, index as u32 + 1);
        }
    }

    #[test]
    fn every_code_round_trips() {
        for error in all_errors() {
            let code = descriptor(error).code;
            assert_eq!(from_code(code), Some(error));
        }
        assert_eq!(from_code(0), None);
        assert_eq!(from_code(24), None);
        assert_eq!(from_code(u32::MAX), None);
    }

    #[test]
    fn every_descriptor_has_metadata() {
        for error in all_errors() {
            let details = descriptor(error);
            assert!(details.code > 0);
            assert!(matches!(details.name, _));
            assert!(matches!(details.class, _));
        }
    }

    #[test]
    fn safety_errors_are_retryable_when_state_can_change() {
        assert!(descriptor(RouterError::ContractPaused).retryable);
        assert!(descriptor(RouterError::InsufficientLiquidity).retryable);
        assert!(descriptor(RouterError::RouteCooldownActive).retryable);
        assert!(!descriptor(RouterError::AmountMustBePositive).retryable);
    }

    #[test]
    fn deterministic_input_and_authorization_errors_are_not_retryable() {
        for error in [
            RouterError::SourceEqualsDestination,
            RouterError::AmountMustBePositive,
            RouterError::NotPendingAdmin,
            RouterError::NotAuthorized,
            RouterError::EmptyBatch,
        ] {
            assert!(!descriptor(error).retryable);
        }
    }

    #[test]
    fn configuration_fix_metadata_is_explicit() {
        assert!(descriptor(RouterError::FeeBpsTooHigh).configuration_fix);
        assert!(descriptor(RouterError::PairNotRegistered).configuration_fix);
        assert!(descriptor(RouterError::CooldownTooLarge).configuration_fix);
        assert!(!descriptor(RouterError::SourceEqualsDestination).configuration_fix);
    }

    #[test]
    fn operation_mapping_covers_every_error() {
        for error in all_errors() {
            assert!(matches!(
                operation_for(error),
                PoolOperation::Register
                    | PoolOperation::ConfigureFee
                    | PoolOperation::ConfigureBounds
                    | PoolOperation::ConfigureLiquidity
                    | PoolOperation::ConfigureCooldown
                    | PoolOperation::Quote
                    | PoolOperation::Compute
                    | PoolOperation::Governance
                    | PoolOperation::Migration
                    | PoolOperation::Batch
            ));
        }
    }

    #[test]
    fn catalog_order_matches_wire_order() {
        let env = Env::default();
        let entries = catalog(&env);
        assert_eq!(entries.len(), 23);
        for (index, entry) in entries.iter().enumerate() {
            assert_eq!(entry.code, index as u32 + 1);
        }
    }

    #[test]
    fn unknown_codes_are_not_coerced_into_known_errors() {
        for code in [24, 42, 100, u32::MAX] {
            assert!(from_code(code).is_none());
        }
    }

    #[test]
    fn retry_and_configuration_helpers_match_descriptors() {
        for error in all_errors() {
            assert_eq!(should_retry(error), descriptor(error).retryable);
            assert_eq!(
                has_configuration_fix(error),
                descriptor(error).configuration_fix
            );
        }
    }

    #[test]
    fn operation_catalogs_are_disjoint_subsets() {
        let env = Env::default();
        let operations = [
            PoolOperation::Register,
            PoolOperation::ConfigureFee,
            PoolOperation::ConfigureBounds,
            PoolOperation::ConfigureLiquidity,
            PoolOperation::ConfigureCooldown,
            PoolOperation::Quote,
            PoolOperation::Compute,
            PoolOperation::Governance,
            PoolOperation::Migration,
            PoolOperation::Batch,
        ];
        let mut total = 0;
        for operation in operations {
            let entries = catalog_for_operation(&env, operation);
            total += entries.len();
            for entry in entries.iter() {
                assert_eq!(operation_for(from_code(entry.code).unwrap()), operation);
            }
        }
        assert_eq!(total, all_errors().len() as u32);
    }

    #[test]
    fn class_counts_cover_all_codes() {
        let total = class_count(ErrorClass::Input)
            + class_count(ErrorClass::Authorization)
            + class_count(ErrorClass::State)
            + class_count(ErrorClass::Limit)
            + class_count(ErrorClass::Governance)
            + class_count(ErrorClass::Safety);
        assert_eq!(total, all_errors().len() as u32);
    }

    #[test]
    fn exact_boundary_errors_have_exact_metadata() {
        assert_eq!(descriptor(RouterError::BatchTooLarge).code, 18);
        assert_eq!(descriptor(RouterError::EmptyBatch).code, 19);
        assert_eq!(descriptor(RouterError::CooldownTooLarge).code, 20);
        assert_eq!(descriptor(RouterError::ZeroFeeCap).code, 21);
        assert_eq!(descriptor(RouterError::InvalidAdminAddress).code, 22);
        assert_eq!(descriptor(RouterError::InvalidParameterRange).code, 23);
        assert_eq!(
            descriptor(RouterError::BatchTooLarge).class,
            ErrorClass::Limit
        );
        assert_eq!(descriptor(RouterError::EmptyBatch).class, ErrorClass::Input);
    }

    #[test]
    fn state_and_safety_paths_remain_distinguishable() {
        assert_eq!(
            descriptor(RouterError::PairNotRegistered).class,
            ErrorClass::State
        );
        assert_eq!(
            descriptor(RouterError::InsufficientLiquidity).class,
            ErrorClass::Safety
        );
        assert_eq!(
            descriptor(RouterError::ContractPaused).class,
            ErrorClass::Safety
        );
    }
}
