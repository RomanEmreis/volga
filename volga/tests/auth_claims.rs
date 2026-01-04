#![allow(missing_docs)]
#![cfg(all(feature = "macros", feature = "jwt-auth-full"))]

use volga::auth::AuthClaims;
use volga_macros::Claims;
use serde::{Deserialize, Serialize};

#[test]
fn it_derives_claims_for_struct_with_role() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        role: String,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        role: "admin".to_string(),
    };

    // Test that role() method is implemented
    assert_eq!(claims.role(), Some("admin"));
    assert_eq!(claims.roles(), None);
    assert_eq!(claims.permissions(), None);
}

#[test]
fn it_derives_claims_for_struct_with_roles() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        roles: Vec<String>,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        roles: vec!["admin".to_string(), "user".to_string()],
    };

    // Test that roles() method is implemented
    assert_eq!(claims.role(), None);
    assert_eq!(claims.roles(), Some(&["admin".to_string(), "user".to_string()][..]));
    assert_eq!(claims.permissions(), None);
}

#[test]
fn it_derives_claims_for_struct_with_permissions() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        permissions: Vec<String>,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        permissions: vec!["read".to_string(), "write".to_string()],
    };

    // Test that permissions() method is implemented
    assert_eq!(claims.role(), None);
    assert_eq!(claims.roles(), None);
    assert_eq!(
        claims.permissions(),
        Some(&["read".to_string(), "write".to_string()][..])
    );
}

#[test]
fn it_derives_claims_for_struct_with_all_fields() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        role: String,
        roles: Vec<String>,
        permissions: Vec<String>,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        role: "admin".to_string(),
        roles: vec!["admin".to_string(), "moderator".to_string()],
        permissions: vec!["read".to_string(), "write".to_string(), "delete".to_string()],
    };

    // Test that all methods are implemented
    assert_eq!(claims.role(), Some("admin"));
    assert_eq!(
        claims.roles(),
        Some(&["admin".to_string(), "moderator".to_string()][..])
    );
    assert_eq!(
        claims.permissions(),
        Some(&["read".to_string(), "write".to_string(), "delete".to_string()][..])
    );
}

#[test]
fn it_derives_claims_for_struct_without_special_fields() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        iss: String,
        exp: u64,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        iss: "issuer".to_string(),
        exp: 1234567890,
    };

    // Test that default implementations are used
    assert_eq!(claims.role(), None);
    assert_eq!(claims.roles(), None);
    assert_eq!(claims.permissions(), None);
}

#[test]
fn it_derives_claims_for_complex_struct() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct ComplexClaims {
        sub: String,
        iss: String,
        aud: String,
        company: String,
        role: String,
        roles: Vec<String>,
        permissions: Vec<String>,
        exp: u64,
        custom_field: Option<String>,
    }

    let claims = ComplexClaims {
        sub: "user123".to_string(),
        iss: "auth-service".to_string(),
        aud: "api".to_string(),
        company: "acme-corp".to_string(),
        role: "owner".to_string(),
        roles: vec!["owner".to_string(), "admin".to_string()],
        permissions: vec!["*".to_string()],
        exp: 9999999999,
        custom_field: Some("custom".to_string()),
    };

    assert_eq!(claims.role(), Some("owner"));
    assert_eq!(
        claims.roles(),
        Some(&["owner".to_string(), "admin".to_string()][..])
    );
    assert_eq!(claims.permissions(), Some(&["*".to_string()][..]));
}

#[test]
fn it_works_with_empty_roles_and_permissions() {
    #[derive(Claims, Clone, Serialize, Deserialize)]
    struct TestClaims {
        sub: String,
        roles: Vec<String>,
        permissions: Vec<String>,
    }

    let claims = TestClaims {
        sub: "user123".to_string(),
        roles: vec![],
        permissions: vec![],
    };

    assert_eq!(claims.roles(), Some(&[][..]));
    assert_eq!(claims.permissions(), Some(&[][..]));
}