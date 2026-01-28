#![allow(missing_docs)]

#[test]
fn ui() {
    if std::env::var_os("CARGO_TARPAULIN").is_some() {
        return;
    }
    
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/http_header_ok.rs");
    tests.compile_fail("tests/ui/http_header_invalid.rs");

    #[cfg(feature = "jwt-auth-derive")]
    {
        tests.pass("tests/ui/claims_ok.rs");
        tests.compile_fail("tests/ui/claims_invalid.rs");
    }
}
