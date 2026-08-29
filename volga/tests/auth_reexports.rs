#![allow(missing_docs)]
#![cfg(feature = "jwt-auth")]

use serde::Deserialize;
use volga::auth::{Authorizer, permission, permissions, predicate, role, roles};

#[derive(Clone, Deserialize)]
struct Claims {
    role: String,
}

impl volga::auth::AuthClaims for Claims {
    fn role(&self) -> Option<&str> {
        Some(&self.role)
    }
}

/// The five built-in authorizers are documented as one family, so they have to import as
/// one - the singular `permission` used to be reachable only through `auth::authorizer`.
#[test]
fn it_reexports_every_built_in_authorizer() {
    let authorizers: [Authorizer<Claims>; 5] = [
        role("admin"),
        roles(["admin", "user"]),
        permission("write"),
        permissions(["read", "write"]),
        predicate(|claims: &Claims| claims.role == "admin"),
    ];

    let admin = Claims {
        role: "admin".into(),
    };

    assert!(authorizers.iter().all(|authorizer| {
        matches!(
            authorizer,
            Authorizer::Role(_) | Authorizer::Permission(_) | Authorizer::Predicate(_)
        )
    }));
    assert!(authorizers[0].validate(&admin));
    assert!(authorizers[4].validate(&admin));
}
