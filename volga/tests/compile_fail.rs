//! Locks in the compile errors that are part of volga's contract.
//!
//! Only messages this crate writes itself are asserted here - a derive's own diagnostics,
//! and the `#[diagnostic::on_unimplemented]` text the extractor and handler traits carry.
//! What rustc says on its own - the non-constant `range` bound, for one - is left to the
//! positive tests, since its wording belongs to the compiler and moves between toolchains,
//! and this repository builds on two. `syn`'s parse errors are asserted, since they come
//! from a pinned dependency rather than the toolchain.
//!
//! Not covered here: the `compile_error!` guards in `volga` and `volga-oauth-client` that
//! require an HTTP transport. `trybuild` builds every case against the feature set the
//! outer test run resolved, so there is no way to switch a feature off for one case; those
//! two are checked by a `cargo check --no-default-features` step in CI that expects a
//! failure.

#![allow(missing_docs)]

/// `trybuild` drives a nested `cargo build` per case, which under coverage instrumentation
/// takes longer than tarpaulin's per-test timeout - and measures nothing while it does:
/// these cases run a proc-macro and then rustc, never a line of volga's runtime, and
/// `volga-macros/src` is excluded from the report anyway. The coverage job sets this.
///
/// Every group below is gated on the features its cases need, so a feature set that leaves
/// all of them out leaves this unused as well.
#[allow(dead_code)]
fn skipped() -> bool {
    std::env::var_os("SKIP_UI_TESTS").is_some()
}

#[test]
#[cfg(feature = "validation-derive")]
fn it_reports_an_attribute_the_validate_derive_cannot_honour() {
    if skipped() {
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/validate/*.rs");
}

#[test]
#[cfg(feature = "macros")]
fn it_reports_a_header_struct_that_is_not_unit_like() {
    if skipped() {
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/http_header/*.rs");
}

#[test]
#[cfg(all(feature = "jwt-derive", feature = "jwt-auth"))]
fn it_reports_claims_derived_for_something_other_than_a_struct() {
    if skipped() {
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/claims/*.rs");
}

// The two groups below assert an unsatisfied trait bound, and rustc frames our
// `#[diagnostic::on_unimplemented]` text with a list of the types that *do* implement the
// trait - which is exactly the set of built-in extractors the feature set compiled in. Both
// are therefore gated on `full`, the one set that turns every extractor on; `--all-features`
// adds nothing to it that implements either trait.

#[test]
#[cfg(feature = "full")]
fn it_reports_a_handler_argument_that_is_not_an_extractor() {
    if skipped() {
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/extractors/*.rs");
}

#[test]
#[cfg(feature = "full")]
fn it_reports_a_middleware_argument_that_cannot_borrow_the_request() {
    if skipped() {
        return;
    }
    trybuild::TestCases::new().compile_fail("tests/ui/middleware/*.rs");
}
