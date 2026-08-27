# StableRoute pool error taxonomy

`RouterError` is the contract's typed negative-path ABI. Every pool failure
uses one of its append-only numeric codes; callers should never need to parse a
panic string or infer an error from an empty return value. The read-only
`get_error_catalog` entrypoint exposes the same code/name/class metadata used
by the SDK and the test matrix.

## Codes

| Code | Error | Class | Retryable | Configuration fix |
|---:|---|---|---|---|
| 1 | `AlreadyInitialized` | Governance | no | no |
| 2 | `NotInitialized` | Governance | no | yes |
| 3 | `SourceEqualsDestination` | Input | no | no |
| 4 | `FeeBpsTooHigh` | Limit | no | yes |
| 5 | `PairNotRegistered` | State | no | yes |
| 6 | `AmountMustBePositive` | Input | no | no |
| 7 | `NoPendingAdminTransfer` | Governance | no | yes |
| 8 | `NotPendingAdmin` | Authorization | no | no |
| 9 | `ContractPaused` | Safety | yes | yes |
| 10 | `AmountBelowMin` | Limit | no | yes |
| 11 | `AmountAboveMax` | Limit | no | yes |
| 12 | `InsufficientLiquidity` | Safety | yes | yes |
| 13 | `MigrationVersionMismatch` | Governance | no | yes |
| 14 | `TimelockNotElapsed` | Governance | yes | no |
| 15 | `ReentrantCall` | Safety | yes | no |
| 16 | `NotAuthorized` | Authorization | no | yes |
| 17 | `RouteCooldownActive` | Safety | yes | yes |
| 18 | `BatchTooLarge` | Limit | no | yes |
| 19 | `EmptyBatch` | Input | no | no |
| 20 | `CooldownTooLarge` | Limit | no | yes |
| 21 | `ZeroFeeCap` | Input | no | yes |

Codes are append-only. Never renumber, reuse, or change the meaning of an
existing code. A future failure must append a new variant and add its catalog
metadata, exact trigger, and negative-path tests in the same change.

## Handling rules

Input and authorization errors are deterministic for the same call and should
be corrected before retrying. Retryable safety errors can succeed after state
changes: an administrator may unpause the router, liquidity may be refreshed,
a cooldown may elapse, or a reentrant invocation may finish. The SDK should
use the catalog's `retryable` bit as guidance, not as permission to retry in a
tight loop.

Configuration-fix metadata means that an administrator can normally resolve
the condition, not that the caller is authorized to change configuration.
`NotAuthorized` remains a caller/role failure even when an admin could rotate
the oracle or update a setting. `PairNotRegistered` requires registration;
setting a fee or liquidity value must never create an implicit pair.

## Pool negative-path matrix

The taxonomy is exhaustive across the pool boundary:

| Operation | Preconditions | Expected error |
|---|---|---|
| constructor/init | duplicate initialization | `AlreadyInitialized` |
| admin read/write | absent admin | `NotInitialized` |
| register | identical source/destination | `SourceEqualsDestination` |
| register/configure | unknown pair | `PairNotRegistered` |
| fee configuration | fee above max | `FeeBpsTooHigh` |
| route/quote | non-positive amount | `AmountMustBePositive` |
| route | amount below floor | `AmountBelowMin` |
| route | amount above ceiling | `AmountAboveMax` |
| route/quote | finite liquidity exhausted | `InsufficientLiquidity` |
| route | cooldown not elapsed | `RouteCooldownActive` |
| pair config | unauthorized oracle/caller | `NotAuthorized` |
| batch | zero entries | `EmptyBatch` |
| batch | more than max entries | `BatchTooLarge` |
| cooldown config | cooldown above max | `CooldownTooLarge` |
| fee cap config | zero cap | `ZeroFeeCap` |
| admin handover | no pending admin | `NoPendingAdminTransfer` |
| admin handover | wrong pending caller | `NotPendingAdmin` |
| admin handover | timelock active | `TimelockNotElapsed` |
| migration | schema is not v1 | `MigrationVersionMismatch` |
| any gated write | router paused | `ContractPaused` |
| guarded route | lock already held | `ReentrantCall` |

The same typed error is used by `quote_route` and `compute_route_fee` for the
same precondition where both operations expose it. Quote is read-only and does
not acquire the route reentrancy lock, but it still follows the registration,
amount, bounds, and liquidity taxonomy.

## Client integration

Clients should decode the contract error code, look it up with `from_code` or
the catalog, and retain the raw code in logs. Unknown future codes must remain
unknown; do not coerce them to a generic error or assume that a missing code
means success. The symbolic name is intended for display and metrics, while
the numeric code is the stable switch value.

Events and off-chain logs should include the operation, pair direction, and
error code without embedding arbitrary user input in metric labels. A failed
transaction is atomic, so an error means the guarded state mutation did not
commit. A caller may still see a prior event from an earlier successful
transaction for the same pair; error handling must not treat that as a rollback
signal.

## Review checklist

- new failures use `RouterError`, never an untyped `panic!`;
- error code is appended and documented;
- `get_error_catalog` metadata is updated;
- exact `#[should_panic(expected = "Error(Contract, #N)")]` coverage exists;
- success paths and failure paths are checked for storage mutations;
- paused, unauthorized, malformed, boundary, and reentrant cases are tested;
- `cargo fmt`, `cargo clippy --all-targets --all-features`, and `cargo test`
  are run before release.
