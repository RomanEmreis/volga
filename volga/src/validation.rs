//! Tools for validating the extracted request data

use std::{
    borrow::Cow,
    error::Error as StdError,
    fmt::{self, Display, Formatter},
};

use crate::{error::Error, http::StatusCode};

pub use self::valid::{Valid, ValidForm, ValidJson, ValidPath, ValidQuery};

/// Derives the [`Validate`] implementation from the field attributes
#[cfg(feature = "validation-derive")]
pub use volga_macros::Validate;

pub mod rules;
pub mod valid;

/// Describes a type that can validate itself.
///
/// Volga knows nothing about the rules: [`Valid`] calls [`validate`](Validate::validate)
/// while extracting the payload, hands it over to the next extractor when it succeeds,
/// and turns the failure into an error response otherwise.
///
/// # Example
/// ```no_run
/// use volga::validation::{Validate, ValidationError};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct KeyValue {
///     key: String,
///     value: String,
/// }
///
/// impl Validate for KeyValue {
///     type Error = ValidationError;
///
///     fn validate(&self) -> Result<(), Self::Error> {
///         let mut err = ValidationError::new();
///         if self.key.is_empty() {
///             err.push("key", "key is required");
///         }
///
///         if self.value.len() > 4096 {
///             err.push("value", "value is too long");
///         }
///         err.into_result()
///     }
/// }
/// ```
pub trait Validate {
    /// An error that describes why the validation did not pass.
    ///
    /// The `Into<Error>` bound is what lets the failure carry its own status code.
    /// A foreign error type that cannot implement it can be wrapped into [`Invalid`].
    type Error: Into<Error>;

    /// Validates `self`
    fn validate(&self) -> Result<(), Self::Error>;

    /// Describes the constraints this type's own fields carry, so that they can be
    /// published in the OpenAPI schema alongside being enforced.
    ///
    /// Empty by default; `#[derive(Validate)]` fills it in from the field attributes.
    /// Only the fields of this type are described - a nested type publishes its own,
    /// reached through [`ConstraintKind::Nested`].
    ///
    /// Built on each call rather than borrowed from a `const`, because a table naming a
    /// generic parameter's own table cannot live in one. It is read when a route is
    /// described, not when a request is served.
    #[inline]
    fn constraints() -> Vec<Constraint> {
        Vec::new()
    }
}

/// A single constraint that a field carries, as OpenAPI describes it
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraint {
    /// The name of the field, as it appears on the wire
    pub field: &'static str,

    /// What the constraint says
    pub kind: ConstraintKind,
}

impl Constraint {
    /// Creates a new [`Constraint`] for `field`
    #[inline]
    #[must_use]
    pub const fn new(field: &'static str, kind: ConstraintKind) -> Self {
        Self { field, kind }
    }
}

/// A numeric bound, kept in the shape it was written in.
///
/// An integer bound beyond `2^53` cannot be held by an `f64` without moving, and the check
/// at runtime compares the exact value - so publishing it as a float would describe a
/// contract the server does not honour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumericBound {
    /// A whole number
    Int(i64),
    /// A whole number past [`i64::MAX`], which only an unsigned bound reaches
    UInt(u64),
    /// A fractional number
    Float(f64),
}

impl NumericBound {
    /// Renders the bound as the JSON number that describes it, if one can hold it
    #[cfg(feature = "openapi")]
    fn to_number(self) -> Option<serde_json::Number> {
        match self {
            Self::Int(value) => Some(value.into()),
            Self::UInt(value) => Some(value.into()),
            Self::Float(value) => serde_json::Number::from_f64(value),
        }
    }
}

impl From<i64> for NumericBound {
    #[inline]
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<u64> for NumericBound {
    #[inline]
    fn from(value: u64) -> Self {
        Self::UInt(value)
    }
}

impl From<f64> for NumericBound {
    #[inline]
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

/// The kinds of constraint that map onto an OpenAPI schema keyword
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ConstraintKind {
    /// A lower bound on the size of the value - the characters of a string, the elements of
    /// a collection, the members of a map. Which keyword publishes it is decided against the
    /// schema, which is the only place the shape of the value is actually known: a type
    /// alias or a newtype hides it from everything upstream.
    MinSize(usize),
    /// An upper bound on the size of the value; see [`ConstraintKind::MinSize`]
    MaxSize(usize),
    /// `minimum`
    Minimum(NumericBound),
    /// `maximum`
    Maximum(NumericBound),
    /// The field validates itself, and these are the constraints its own fields carry
    Nested(fn() -> Vec<Constraint>),
    /// The field is a collection whose elements validate themselves
    Each(fn() -> Vec<Constraint>),
}

/// A single validation failure: an optional field name and a message
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    field: Option<Cow<'static, str>>,
    message: Cow<'static, str>,
}

/// A set of validation failures that responds with `400 Bad Request` by default.
///
/// # Example
/// ```no_run
/// use volga::validation::ValidationError;
///
/// let mut err = ValidationError::new();
/// err.push("per_page", "must be between 1 and 100");
///
/// assert!(err.into_result().is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    status: StatusCode,
    entries: Vec<Entry>,
}

impl Default for ValidationError {
    #[inline]
    fn default() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            entries: Vec::new(),
        }
    }
}

impl ValidationError {
    /// Creates an empty [`ValidationError`] to accumulate failures into
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a [`ValidationError`] that describes a single failed `field`
    #[inline]
    #[must_use]
    pub fn field(
        field: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        let mut err = Self::new();
        err.push(field, message);
        err
    }

    /// Creates a [`ValidationError`] that describes a failure not bound to a specific field
    #[inline]
    #[must_use]
    pub fn message(message: impl Into<Cow<'static, str>>) -> Self {
        let mut err = Self::new();
        err.push_message(message);
        err
    }

    /// Overrides the HTTP status code this error responds with
    #[inline]
    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Adds a failure for the `field`
    #[inline]
    pub fn push(
        &mut self,
        field: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> &mut Self {
        self.entries.push(Entry {
            field: Some(field.into()),
            message: message.into(),
        });

        self
    }

    /// Adds a failure that is not bound to a specific field
    #[inline]
    pub fn push_message(&mut self, message: impl Into<Cow<'static, str>>) -> &mut Self {
        self.entries.push(Entry {
            field: None,
            message: message.into(),
        });

        self
    }

    /// Returns the HTTP status code this error responds with
    #[inline]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns `true` if nothing failed
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of collected failures
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iterates over the collected failures as `(field, message)` pairs
    #[inline]
    pub fn entries(&self) -> impl Iterator<Item = (Option<&str>, &str)> {
        self.entries
            .iter()
            .map(|entry| (entry.field.as_deref(), entry.message.as_ref()))
    }

    /// Merges the failures of `other` in, keeping the fields they name.
    ///
    /// A status `other` asked for is adopted when this error is still answering with the
    /// default, so a rule that wants `422` gets it whether it ran alone or alongside others.
    #[inline]
    pub fn merge(&mut self, other: Self) -> &mut Self {
        self.adopt_status(other.status);
        self.entries.extend(other.entries);
        self
    }

    /// Takes on a status another error asked for, unless one has already been asked for here
    #[inline]
    fn adopt_status(&mut self, status: StatusCode) {
        if self.status == StatusCode::BAD_REQUEST {
            self.status = status;
        }
    }

    /// Merges the failures of `other` in, prefixing each field with `prefix`.
    ///
    /// A failure that names no field is attributed to `prefix` itself, so a nested
    /// type's own message lands on the field that holds it.
    #[inline]
    pub fn merge_at(&mut self, prefix: &str, other: Self) -> &mut Self {
        self.adopt_status(other.status);
        self.entries.reserve(other.entries.len());
        for entry in other.entries {
            let field = match entry.field {
                Some(field) => format!("{prefix}.{field}"),
                None => prefix.to_owned(),
            };
            self.entries.push(Entry {
                field: Some(field.into()),
                message: entry.message,
            });
        }
        self
    }

    /// Turns the accumulated failures into a [`Result`],
    /// which is `Ok(())` when nothing has been collected
    #[inline]
    pub fn into_result(self) -> Result<(), Self> {
        if self.is_empty() { Ok(()) } else { Err(self) }
    }
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.entries.is_empty() {
            return f.write_str("validation failed");
        }

        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }

            match &entry.field {
                Some(field) => write!(f, "{field}: {}", entry.message)?,
                None => f.write_str(&entry.message)?,
            }
        }

        Ok(())
    }
}

impl StdError for ValidationError {}

impl From<ValidationError> for Error {
    #[inline]
    fn from(err: ValidationError) -> Self {
        Error::from_parts(err.status, None, err)
    }
}

/// Compared by what the constraint says.
///
/// The two nesting kinds carry a function rather than a table, since a table cannot name
/// another type's table in a `const`; those compare by address, which is the most any
/// comparison of function pointers can promise.
impl PartialEq for ConstraintKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MinSize(this), Self::MinSize(that))
            | (Self::MaxSize(this), Self::MaxSize(that)) => this == that,
            (Self::Minimum(this), Self::Minimum(that))
            | (Self::Maximum(this), Self::Maximum(that)) => this == that,
            (Self::Nested(this), Self::Nested(that)) | (Self::Each(this), Self::Each(that)) => {
                std::ptr::fn_addr_eq(*this, *that)
            }
            _ => false,
        }
    }
}

/// Converts a constraint table into the form the OpenAPI layer applies,
/// descending into the nested types as it goes.
#[cfg(feature = "openapi")]
pub(crate) fn schema_constraints(
    constraints: Vec<Constraint>,
) -> Vec<crate::openapi::FieldConstraint> {
    schema_constraints_within(constraints, &mut Vec::new())
}

/// Walks a constraint table, carrying the tables already open on this path.
///
/// A type that nests itself would describe an endless schema, so a table already being
/// walked is not entered again. Nothing else is cut off: a model nested as deeply as it
/// likes is published as deeply as it goes.
#[cfg(feature = "openapi")]
fn schema_constraints_within(
    constraints: Vec<Constraint>,
    open: &mut Vec<fn() -> Vec<Constraint>>,
) -> Vec<crate::openapi::FieldConstraint> {
    use crate::openapi::{FieldConstraint, SchemaConstraint};

    constraints
        .into_iter()
        .filter_map(|constraint| {
            let mut descend = |table: fn() -> Vec<Constraint>| {
                if open
                    .iter()
                    .any(|walked| std::ptr::fn_addr_eq(*walked, table))
                {
                    return Vec::new();
                }
                open.push(table);
                let nested = schema_constraints_within(table(), open);
                open.pop();
                nested
            };

            let schema_constraint = match constraint.kind {
                ConstraintKind::MinSize(value) => SchemaConstraint::MinSize(value),
                ConstraintKind::MaxSize(value) => SchemaConstraint::MaxSize(value),
                // A bound no JSON number can hold - an infinity, a NaN - describes nothing
                // a client could check, so it is left out rather than rounded into a lie
                ConstraintKind::Minimum(bound) => SchemaConstraint::Minimum(bound.to_number()?),
                ConstraintKind::Maximum(bound) => SchemaConstraint::Maximum(bound.to_number()?),
                ConstraintKind::Nested(table) => SchemaConstraint::Nested(descend(table)),
                ConstraintKind::Each(table) => SchemaConstraint::Each(descend(table)),
            };
            Some(FieldConstraint::new(constraint.field, schema_constraint))
        })
        .collect()
}

/// Wraps a foreign error into something [`Validate::Error`] accepts.
///
/// A validation crate's own error type and [`Error`] are both foreign to a user crate,
/// so `impl From<TheirError> for volga::error::Error` cannot be written there.
/// `Invalid` is the newtype that bridges the two, responding with `400 Bad Request`.
///
/// # Example
/// ```no_run
/// use volga::validation::{Invalid, Validate};
///
/// # #[derive(Debug)]
/// # struct TheirError;
/// # impl std::fmt::Display for TheirError {
/// #     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("nope") }
/// # }
/// # impl std::error::Error for TheirError {}
/// # struct KeyValue;
/// # impl KeyValue {
/// #     fn check(&self) -> Result<(), TheirError> { Ok(()) }
/// # }
/// impl Validate for KeyValue {
///     type Error = Invalid<TheirError>;
///
///     fn validate(&self) -> Result<(), Self::Error> {
///         self.check().map_err(Invalid)
///     }
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalid<E>(pub E);

impl<E> Invalid<E> {
    /// Unwraps the inner error
    #[inline]
    pub fn into_inner(self) -> E {
        self.0
    }
}

impl<E: Display> Display for Invalid<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<E: StdError + 'static> StdError for Invalid<E> {
    #[inline]
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.0)
    }
}

impl<E: StdError + Send + Sync + 'static> From<Invalid<E>> for Error {
    #[inline]
    fn from(err: Invalid<E>) -> Self {
        Error::client_error(err.0)
    }
}

/// Renders a [`ValidationError`] as [Problem Details](https://datatracker.ietf.org/doc/html/rfc9457),
/// with an `errors` extension that maps a field name to the messages it collected.
///
/// Failures that are not bound to a field are grouped under an empty key.
/// Returns the untouched [`Error`] back when it does not carry a [`ValidationError`].
#[cfg(feature = "problem-details")]
pub(crate) fn try_into_problem(err: Error) -> Result<crate::error::Problem, Error> {
    use std::collections::HashMap;

    let (status, instance, inner) = err.into_parts();
    let err = match inner.downcast::<ValidationError>() {
        Ok(err) => err,
        Err(inner) => return Err(Error::from_parts(status, instance, inner)),
    };

    let mut errors: HashMap<&str, Vec<&str>> = HashMap::with_capacity(err.len());
    for (field, message) in err.entries() {
        errors
            .entry(field.unwrap_or_default())
            .or_default()
            .push(message);
    }

    let problem = crate::error::Problem::new(status.as_u16())
        .with_detail(err.to_string())
        .add_param("errors", errors);

    Ok(match instance {
        Some(instance) => problem.with_instance(instance),
        None => problem,
    })
}

#[cfg(test)]
mod tests {
    use super::{Invalid, ValidationError};
    use crate::{error::Error, http::StatusCode};
    use std::{
        error::Error as StdError,
        fmt::{self, Display, Formatter},
    };

    #[derive(Debug)]
    struct ForeignError;

    impl Display for ForeignError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str("foreign error")
        }
    }

    impl StdError for ForeignError {}

    #[test]
    fn it_collects_failures() {
        let mut err = ValidationError::new();
        assert!(err.is_empty());
        assert!(err.clone().into_result().is_ok());

        err.push("key", "key is required");
        err.push_message("payload is inconsistent");

        assert_eq!(err.len(), 2);
        assert_eq!(
            err.entries().collect::<Vec<_>>(),
            vec![
                (Some("key"), "key is required"),
                (None, "payload is inconsistent")
            ]
        );
        assert_eq!(
            err.to_string(),
            "key: key is required; payload is inconsistent"
        );
        assert!(err.into_result().is_err());
    }

    #[test]
    fn it_creates_single_entry_errors() {
        assert_eq!(
            ValidationError::field("key", "key is required").to_string(),
            "key: key is required"
        );
        assert_eq!(
            ValidationError::message("payload is inconsistent").to_string(),
            "payload is inconsistent"
        );
        assert_eq!(ValidationError::new().to_string(), "validation failed");
    }

    #[test]
    fn it_responds_with_bad_request_by_default() {
        let err = ValidationError::field("key", "key is required");

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(Error::from(err).status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_overrides_the_status() {
        let err = ValidationError::field("key", "key is required")
            .with_status(StatusCode::UNPROCESSABLE_ENTITY);

        assert_eq!(err.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(Error::from(err).status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn it_wraps_a_foreign_error() {
        let err = Invalid(ForeignError);

        assert_eq!(err.to_string(), "foreign error");
        assert!(StdError::source(&err).is_some());
        assert_eq!(err.into_inner().to_string(), "foreign error");

        let err = Error::from(Invalid(ForeignError));

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.to_string(), "foreign error");
    }

    #[cfg(feature = "problem-details")]
    #[test]
    fn it_renders_problem_details() {
        let mut err = ValidationError::new();
        err.push("per_page", "must be between 1 and 100");
        err.push("per_page", "must be a number");
        err.push_message("payload is inconsistent");

        let problem = super::try_into_problem(err.into()).unwrap();

        assert_eq!(problem.status, 400);
        assert_eq!(
            problem.detail.as_deref(),
            Some(
                "per_page: must be between 1 and 100; per_page: must be a number; payload is inconsistent"
            )
        );
        assert_eq!(
            problem.extensions["errors"]["per_page"],
            serde_json::json!(["must be between 1 and 100", "must be a number"])
        );
        assert_eq!(
            problem.extensions["errors"][""],
            serde_json::json!(["payload is inconsistent"])
        );
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn it_stops_walking_a_table_that_names_itself() {
        // A type holding a collection of itself is a legitimate model, and its constraint
        // table is genuinely cyclic - the walk has to notice rather than run out of stack
        use super::{Constraint, ConstraintKind};

        fn node() -> Vec<Constraint> {
            vec![
                Constraint::new("name", ConstraintKind::MinSize(1)),
                Constraint::new("children", ConstraintKind::Each(node)),
            ]
        }

        let constraints = super::schema_constraints(node());

        assert_eq!(constraints.len(), 2);
        let crate::openapi::SchemaConstraint::Each(children) = &constraints[1].constraint else {
            panic!("expected the collection to be described");
        };
        // One level down the cycle is closed, and what is above it is still published
        assert_eq!(children.len(), 2);
        assert!(matches!(
            children[1].constraint,
            crate::openapi::SchemaConstraint::Each(ref inner) if inner.is_empty()
        ));
    }

    #[cfg(feature = "problem-details")]
    #[test]
    fn it_leaves_other_errors_untouched() {
        let err = super::try_into_problem(Error::server_error("boom"))
            .err()
            .unwrap();

        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.to_string(), "boom");
    }
}
