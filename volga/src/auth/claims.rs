//! Utilities for custom claims

use serde::de::DeserializeOwned;

/// Trait representing extractable authorization claims from a JWT payload.
///
/// Types implementing this trait allow the framework to access optional authorization
/// information such as roles or permissions, enabling role-based or permission-based
/// access control.
///
/// This trait is intended to be implemented by your custom claims struct, which is typically
/// deserialized from the JWT payload. All methods are optional; by default, they return `None`.
/// You can override only the methods relevant to your use case.
///
/// # Example
///
/// ```no_run
/// use serde::Deserialize;
/// use volga::auth::AuthClaims;
///
/// #[derive(Debug, Deserialize)]
/// struct MyClaims {
///     sub: String,
///     role: String,
/// }
///
/// impl AuthClaims for MyClaims {
///     fn role(&self) -> Option<&str> {
///         Some(&self.role)
///     }
/// }
/// ```
pub trait AuthClaims: DeserializeOwned {
    /// Returns the primary role associated with the claims.
    ///
    /// This is useful for role-based access control (RBAC) where only a single role is expected.
    /// If multiple roles are used, prefer implementing the [`AuthClaims::roles()`] method.
    ///
    /// By default, returns `None`.
    fn role(&self) -> Option<&str> {
        None
    }

    /// Returns the list of roles associated with the claims.
    ///
    /// Useful when a subject can have multiple roles.
    /// If a single role is used instead, you may override [`AuthClaims::role()`] instead.
    ///
    /// By default, returns `None`.
    fn roles(&self) -> Option<&[String]> {
        None
    }

    /// Returns the list of permissions granted to the subject.
    ///
    /// By default, returns `None`.
    fn permissions(&self) -> Option<&[String]> {
        None
    }
}

//#[cfg(all(feature = "jwt-auth", not(feature = "jwt-auth-full")))]
#[macro_export]
macro_rules! claims {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field:ident : $type:ty
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                pub $field: $type,
            )*
        }

        impl $crate::auth::AuthClaims for $name {
            $(
                claims!(@gen_impl $field);
            )*
        }
    };

    (@gen_impl role) => {
        fn role(&self) -> Option<&str> {
            Some(&self.role)
        }
    };
    (@gen_impl roles) => {
        fn roles(&self) -> Option<&[String]> {
            Some(&self.roles)
        }
    };
    (@gen_impl permissions) => {
        fn permissions(&self) -> Option<&[String]> {
            Some(&self.permissions)
        }
    };
    
    (@gen_impl $field:ident) => {};
}

#[cfg(test)]
mod tests {
    use super::super::{Authorizer, role, roles, permissions, predicate};
    use serde::{Serialize, Deserialize};

    claims! {
        #[derive(Debug, Serialize, Deserialize)]
        struct MyClaims {
            sub: String,
            role: String,
            permissions: Vec<String>,
        }
    }

    claims! {
        #[derive(Debug, Serialize, Deserialize)]
        struct MyClaims2 {
            sub: String,
            roles: Vec<String>,
        }
    }
    
    #[test]
    fn it_creates_claims_and_test_role() {
        let claims = MyClaims {
            sub: "123".to_string(),
            role: "admin".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };
        
        let validate: Authorizer<MyClaims> = role("admin");
        assert!(validate.validate(&claims))
    }

    #[test]
    fn it_creates_claims_and_test_roles() {
        let claims = MyClaims2 {
            sub: "123".to_string(),
            roles: vec!["admin".to_string(), "user".to_string()],
        };

        let validate: Authorizer<MyClaims2> = roles(["admin", "user", "editor"]);
        assert!(validate.validate(&claims))
    }

    #[test]
    fn it_creates_claims_and_test_permissions() {
        let claims = MyClaims {
            sub: "123".to_string(),
            role: "user".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        let validate: Authorizer<MyClaims> = permissions(["write"]);
        assert!(validate.validate(&claims))
    }

    #[test]
    fn it_creates_claims_and_test_predicate() {
        let claims = MyClaims {
            sub: "123".to_string(),
            role: "user".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };
        let validate = predicate(|c: &MyClaims| c.sub == "123");
        assert!(validate.validate(&claims))
    }
}