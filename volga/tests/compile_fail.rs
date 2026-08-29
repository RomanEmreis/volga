//! Locks in the errors the `Validate` derive reports for an attribute it cannot honour.
//!
//! Only messages this crate writes itself are asserted here. What rustc says - the
//! non-constant `range` bound, for one - is left to the positive tests, since its wording
//! belongs to the compiler and moves between toolchains, and this repository builds on two.

#![allow(missing_docs)]
#![cfg(feature = "validation-derive")]

#[test]
fn it_reports_an_attribute_it_cannot_honour() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
