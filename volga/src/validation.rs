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
    /// Only the fields of this type are described - a nested type publishes its own.
    #[inline]
    fn constraints() -> &'static [Constraint] {
        &[]
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

/// The kinds of constraint that map onto an OpenAPI schema keyword
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ConstraintKind {
    /// `minLength`
    MinLength(usize),
    /// `maxLength`
    MaxLength(usize),
    /// `minItems`
    MinItems(usize),
    /// `maxItems`
    MaxItems(usize),
    /// `minProperties`
    MinProperties(usize),
    /// `maxProperties`
    MaxProperties(usize),
    /// `minimum`
    Minimum(f64),
    /// `maximum`
    Maximum(f64),
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

    /// Merges the failures of `other` in, keeping the fields they name
    #[inline]
    pub fn merge(&mut self, other: Self) -> &mut Self {
        self.entries.extend(other.entries);
        self
    }

    /// Merges the failures of `other` in, prefixing each field with `prefix`.
    ///
    /// A failure that names no field is attributed to `prefix` itself, so a nested
    /// type's own message lands on the field that holds it.
    #[inline]
    pub fn merge_at(&mut self, prefix: &str, other: Self) -> &mut Self {
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

#[cfg(feature = "openapi")]
impl From<ConstraintKind> for crate::openapi::SchemaConstraint {
    #[inline]
    fn from(kind: ConstraintKind) -> Self {
        match kind {
            ConstraintKind::MinLength(value) => Self::MinLength(value),
            ConstraintKind::MaxLength(value) => Self::MaxLength(value),
            ConstraintKind::MinItems(value) => Self::MinItems(value),
            ConstraintKind::MaxItems(value) => Self::MaxItems(value),
            ConstraintKind::MinProperties(value) => Self::MinProperties(value),
            ConstraintKind::MaxProperties(value) => Self::MaxProperties(value),
            ConstraintKind::Minimum(value) => Self::Minimum(value),
            ConstraintKind::Maximum(value) => Self::Maximum(value),
        }
    }
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
