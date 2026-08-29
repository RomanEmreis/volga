//! Locks in the errors the `Validate` derive reports for an attribute it cannot honour.
//!
//! Only messages this crate writes itself are asserted here. What rustc says - the
//! non-constant `range` bound, for one - is left to the positive tests, since its wording
//! belongs to the compiler and moves between toolchains, and this repository builds on two.

#![allow(missing_docs)]
#![cfg(feature = "validation-derive")]

#[test]
fn it_reports_an_attribute_it_cannot_honour() {
    // `trybuild` drives a nested `cargo build` per case, which under coverage instrumentation
    // takes longer than tarpaulin's per-test timeout - and measures nothing while it does:
    // these cases run the proc-macro and then rustc, never a line of volga's runtime, and
    // `volga-macros/src` is excluded from the report anyway. The coverage job sets this.
    if std::env::var_os("SKIP_UI_TESTS").is_some() {
        return;
    }

    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
