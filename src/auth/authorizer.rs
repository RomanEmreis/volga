//! Generic authorization tools

use std::collections::HashSet;
use std::sync::Arc;
use super::AuthClaims;

pub(super) const DEFAULT_ERROR_MSG: &str = "Bearer error=\"insufficient_scope\" error_description=\"User does not have required role or permission\"";

/// Creates an [`Authorizer::Role`] authorizer for a single role.
///
/// Equivalent to: `Authorizer::Role([role.to_string()].into_iter().collect())`
pub fn role<C>(name: impl Into<String>) -> Authorizer<C>
where
    C: AuthClaims,
{
    Authorizer::Role(HashSet::from([name.into()]))
}

/// Creates an [` Authorizer::Role `] authorizer for multiple roles.
pub fn roles<S, I, C>(roles: I) -> Authorizer<C>
where
    C: AuthClaims,
    S: Into<String>,
    I: IntoIterator<Item = S>,
{
    Authorizer::Role(roles
        .into_iter()
        .map(Into::into)
        .collect())
}

/// Creates an [`Authorizer::Permission`] authorizer for a single permission.
pub fn permission<C>(name: impl Into<String>) -> Authorizer<C>
where
    C: AuthClaims,
{
    Authorizer::Permission(HashSet::from([name.into()]))
}

/// Creates an [`Authorizer::Permission`] authorizer for multiple permissions.
pub fn permissions<S, I, C>(permissions: I) -> Authorizer<C>
where
    C: AuthClaims,
    S: Into<String>,
    I: IntoIterator<Item = S>,
{
    Authorizer::Permission(permissions
        .into_iter()
        .map(Into::into)
        .collect())
}

/// Creates an [`Authorizer::Predicate`] authorizer from a closure or function.
pub fn predicate<C, F>(f: F) -> Authorizer<C>
where
    C: AuthClaims,
    F: Fn(&C) -> bool + Send + Sync + 'static,
{
    Authorizer::Predicate(Arc::new(f))
}

pub type ClaimsValidator<C> = dyn Fn(&C) -> bool + Send + Sync + 'static;

/// Specifies the validation rules for role-based or permission-based access control.
///
/// This enum allows you to define access policies declaratively, based on claims extracted from a JWT.
/// It supports role matching, permission matching, custom predicates, and logical composition
/// (`And`/`Or`) of other authorizers.
///
/// The `Authorizer` works with any claims type implementing [`AuthClaims`], and can be used
/// for both simple and complex access control scenarios.
///
/// # Examples
/// ```no_run
/// use volga::auth::{Authorizer, AuthClaims, role, roles};
/// use serde::Deserialize;
/// 
/// #[derive(Deserialize)]
/// struct MyClaims {
///     role: String
/// }
/// 
/// impl AuthClaims for MyClaims {
///     fn role(&self) -> Option<&str> {
///         Some(self.role.as_str())
///     }
/// }
/// 
/// let admin_only = role("admin");
/// let any_editor = roles(["editor", "contributor"]);
/// 
/// let access: Authorizer<MyClaims> = admin_only.or(any_editor);
/// 
/// assert!(access.validate(&MyClaims { role: "admin".to_string() }));
/// assert!(access.validate(&MyClaims { role: "editor".to_string() }));
/// assert!(access.validate(&MyClaims { role: "contributor".to_string() }));
/// ```
pub enum Authorizer<C: AuthClaims> {
    /// Allows access if the user's role or roles match any of the required roles.
    ///
    /// This will check both [`AuthClaims::role()`] and [`AuthClaims::roles()`].
    Role(HashSet<String>),

    /// Allows access if the user's permissions contain any of the required permissions.
    ///
    /// This assumes [`AuthClaims::permissions()`] returns a list of permission strings.
    Permission(HashSet<String>),

    /// Allows custom validation logic through a user-defined predicate.
    ///
    /// This enables flexible access control based on arbitrary logic over the claims.
    Predicate(Arc<ClaimsValidator<C>>),

    /// Allows access only if **all** inner authorizers return `true`.
    ///
    /// Logical **AND** operation.
    And(Vec<Authorizer<C>>),

    /// Allows access if **any** of the inner authorizers return `true`.
    ///
    /// Logical **OR** operation.
    Or(Vec<Authorizer<C>>),
}

impl<C: AuthClaims> Authorizer<C> {
    /// Validates the given claims against this authorizer's rule set.
    ///
    /// Returns `true` if access is allowed, `false` otherwise.
    ///
    /// This method evaluates role and permission membership or applies custom logic
    /// as defined in the authorizer.
    pub fn validate(&self, claims: &C) -> bool {
        match self {
            Authorizer::Predicate(pred) => pred(claims),
            Authorizer::And(auths) => auths
                .iter()
                .all(|a| a.validate(claims)),
            Authorizer::Or(auths) => auths
                .iter()
                .any(|a| a.validate(claims)),
            Authorizer::Role(roles) => {
                match (claims.role(), claims.roles()) {
                    (Some(r), None) => roles.contains(r),
                    (_, Some(rs)) => rs.iter().any(|r| roles.contains(r)),
                    (None, None) => false,
                }
            },
            Authorizer::Permission(perms) => claims
                .permissions()
                .is_some_and(|p| p.iter().any(|perm| perms.contains(perm)))
        }
    }
    
    /// Combines the current authorizer with another one via logical **AND** (And).
    ///
    /// If both operands are already `And`, then their contents are combined into one list.
    /// This avoids unnecessary nesting: `And([And([a]), And([b])]) → And([a, b])`
    pub fn and(self, other: Authorizer<C>) -> Self {
        match (self, other) {
            (Authorizer::And(mut a), Authorizer::And(mut b)) => {
                a.append(&mut b);
                Authorizer::And(a)
            }
            (Authorizer::And(mut a), b) => {
                a.push(b);
                Authorizer::And(a)
            }
            (a, Authorizer::And(mut b)) => {
                let mut v = vec![a];
                v.append(&mut b);
                Authorizer::And(v)
            }
            (a, b) => Authorizer::And(vec![a, b]),
        }
    }

    /// Combines the current authorizer with another via logical **OR** (Or).
    ///
    /// Behavior is similar to [`Authorizer::and()`], but for the `Or` type.
    pub fn or(self, other: Authorizer<C>) -> Self {
        match (self, other) {
            (Authorizer::Or(mut a), Authorizer::Or(mut b)) => {
                a.append(&mut b);
                Authorizer::Or(a)
            }
            (Authorizer::Or(mut a), b) => {
                a.push(b);
                Authorizer::Or(a)
            }
            (a, Authorizer::Or(mut b)) => {
                let mut v = vec![a];
                v.append(&mut b);
                Authorizer::Or(v)
            }
            (a, b) => Authorizer::Or(vec![a, b]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Authorizer, AuthClaims, role, roles};

    #[derive(serde::Deserialize)]
    struct Claims {
        role: String,
    }

    impl AuthClaims for Claims {
        fn role(&self) -> Option<&str> {
            Some(&self.role)
        }
    }
    
    #[test]
    fn it_tests_the_and_flattening() {
        let a = role::<Claims>("admin");
        let b = role::<Claims>("editor");
        let c = role::<Claims>("moderator");

        let ab = a.and(b); // And([admin, editor])
        let abc = ab.and(c); // And([admin, editor, moderator])

        match abc {
            Authorizer::And(inner) => {
                assert_eq!(inner.len(), 3);
                assert!(matches!(inner[0], Authorizer::Role(ref s) if s.contains(&"admin".to_owned())));
                assert!(matches!(inner[1], Authorizer::Role(ref s) if s.contains(&"editor".to_owned())));
                assert!(matches!(inner[2], Authorizer::Role(ref s) if s.contains(&"moderator".to_owned())));
            }
            _ => panic!("Expected And variant"),
        }
    }

    #[test]
    fn it_tests_the_or_flattening() {
        let a = role("admin");
        let b = role("editor");
        let c = roles(["viewer"]);

        let ab = a.or(b); // Or([admin, editor])
        let abc: Authorizer<Claims> = ab.or(c); // Or([admin, editor, viewer])

        match abc {
            Authorizer::Or(inner) => {
                assert_eq!(inner.len(), 3);
                assert!(matches!(inner[0], Authorizer::Role(ref s) if s.contains(&"admin".to_owned())));
                assert!(matches!(inner[1], Authorizer::Role(ref s) if s.contains(&"editor".to_owned())));
                assert!(matches!(inner[2], Authorizer::Role(ref s) if s.contains(&"viewer".to_owned())));
            }
            _ => panic!("Expected Or variant"),
        }
    }

    #[test]
    fn it_tests_mixed_and_or_structure() {
        let a = role::<Claims>("admin");
        let b = role::<Claims>("editor");
        let c = role::<Claims>("moderator");

        let or_expr = a.or(b); // Or([admin, editor])
        let combined = or_expr.and(c); // And([Or([...]), moderator])

        match combined {
            Authorizer::And(inner) => {
                assert_eq!(inner.len(), 2);
                match &inner[0] {
                    Authorizer::Or(or_inner) => {
                        assert_eq!(or_inner.len(), 2);
                    }
                    _ => panic!("Expected Or inside And[0]"),
                }
                assert!(matches!(inner[1], Authorizer::Role(ref s) if s.contains(&"moderator".to_owned())));
            }
            _ => panic!("Expected And variant"),
        }
    }
}