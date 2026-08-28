//! Extractor that validates the payload it wraps

use futures_util::ready;
use pin_project_lite::pin_project;

use std::{
    fmt::{self, Display, Formatter},
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    HttpRequest,
    error::Error,
    http::{
        Parts,
        endpoints::args::{
            FromPayload, FromRequestParts, FromRequestRef, Payload, Source, form::Form, json::Json,
            path::Path, query::Query,
        },
    },
    validation::Validate,
};

/// Wraps an extractor `E` and runs [`Validate::validate`] on the payload it produced,
/// before the handler is called.
///
/// Since `Json`, `Query`, `Form` and `Path` all deref to their payload,
/// `Valid` composes with each of them - see [`ValidJson`], [`ValidQuery`],
/// [`ValidForm`] and [`ValidPath`] for the shorthand.
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Json, ok, validation::{Valid, Validate, ValidationError}};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct KeyValue {
///     key: String,
/// }
///
/// impl Validate for KeyValue {
///     type Error = ValidationError;
///
///     fn validate(&self) -> Result<(), Self::Error> {
///         if self.key.is_empty() {
///             return Err(ValidationError::field("key", "key is required"));
///         }
///         Ok(())
///     }
/// }
///
/// async fn handle(val: Valid<Json<KeyValue>>) -> HttpResult {
///     ok!("Hello {}", val.key)
/// }
/// ```
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Valid<E>(pub E);

/// A [`Json<T>`] payload that has been validated
pub type ValidJson<T> = Valid<Json<T>>;

/// A [`Query<T>`] payload that has been validated
pub type ValidQuery<T> = Valid<Query<T>>;

/// A [`Form<T>`] payload that has been validated
pub type ValidForm<T> = Valid<Form<T>>;

/// A [`Path<T>`] payload that has been validated
pub type ValidPath<T> = Valid<Path<T>>;

impl<E> Valid<E> {
    /// Unwraps the inner extractor
    #[inline]
    pub fn into_inner(self) -> E {
        self.0
    }
}

impl<E> Deref for Valid<E> {
    type Target = E;

    #[inline]
    fn deref(&self) -> &E {
        &self.0
    }
}

impl<E> DerefMut for Valid<E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut E {
        &mut self.0
    }
}

impl<E: Display> Display for Valid<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Reports what an extractor describes in the OpenAPI operation, if anything
#[cfg(feature = "openapi")]
fn constraint_target(source: Source) -> Option<crate::openapi::ConstraintTarget> {
    use crate::openapi::ConstraintTarget;
    match source {
        Source::Body | Source::Full | Source::Request => Some(ConstraintTarget::RequestBody),
        Source::Parts => Some(ConstraintTarget::QueryParameter),
        Source::Path | Source::PathArgs => Some(ConstraintTarget::PathParameter),
        Source::None => None,
    }
}

/// Runs the validation over an already extracted payload
#[inline]
fn validate<E, T>(value: E) -> Result<Valid<E>, Error>
where
    E: Deref<Target = T>,
    T: Validate,
{
    match Validate::validate(&*value) {
        Ok(()) => Ok(Valid(value)),
        Err(err) => Err(err.into()),
    }
}

pin_project! {
    /// Future for the [`Valid<E>`] extractor
    pub struct ValidFromPayloadFuture<F> {
        #[pin]
        inner: F,
    }
}

impl<F, E, T> Future for ValidFromPayloadFuture<F>
where
    F: Future<Output = Result<E, Error>>,
    E: Deref<Target = T>,
    T: Validate,
{
    type Output = Result<Valid<E>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        Poll::Ready(ready!(this.inner.poll(cx)).and_then(validate))
    }
}

/// Extracts the payload with `E` and validates it
impl<E, T> FromPayload for Valid<E>
where
    E: FromPayload + Deref<Target = T>,
    T: Validate,
{
    type Future = ValidFromPayloadFuture<E::Future>;

    const SOURCE: Source = E::SOURCE;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        ValidFromPayloadFuture {
            inner: E::from_payload(payload),
        }
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        let config = E::describe_openapi(config);

        // Handler arguments are described one after another into the same operation, so the
        // constraints have to name what the wrapped extractor describes - otherwise a body
        // and a query struct sharing a field name would hand each other their bounds.
        let Some(target) = constraint_target(E::SOURCE) else {
            return config;
        };

        let constraints = crate::validation::schema_constraints(T::constraints());

        if constraints.is_empty() {
            config
        } else {
            config.with_constraints(target, &constraints)
        }
    }
}

impl<E, T> FromRequestRef for Valid<E>
where
    E: FromRequestRef + Deref<Target = T>,
    T: Validate,
{
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        E::from_request(req).and_then(validate)
    }
}

impl<E, T> FromRequestParts for Valid<E>
where
    E: FromRequestParts + Deref<Target = T>,
    T: Validate,
{
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        E::from_parts(parts).and_then(validate)
    }
}

#[cfg(test)]
mod tests {
    use super::{Valid, ValidQuery};
    use crate::http::endpoints::args::{
        FromPayload, FromRequestParts, FromRequestRef, Payload, Source,
    };
    use crate::validation::{Validate, ValidationError};
    use crate::{HttpBody, HttpRequest, Json, Query, http::StatusCode};
    use hyper::Request;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct KeyValue {
        key: String,
        value: String,
    }

    impl Validate for KeyValue {
        type Error = ValidationError;

        fn validate(&self) -> Result<(), Self::Error> {
            let mut err = ValidationError::new();
            if self.key.is_empty() {
                err.push("key", "key is required");
            }
            if self.value.len() > 8 {
                err.push("value", "value is too long");
            }
            err.into_result()
        }
    }

    #[derive(Debug, Deserialize)]
    struct Filter {
        per_page: u32,
    }

    impl std::fmt::Display for Filter {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.per_page)
        }
    }

    impl Validate for Filter {
        type Error = ValidationError;

        fn validate(&self) -> Result<(), Self::Error> {
            if self.per_page == 0 || self.per_page > 100 {
                return Err(ValidationError::field(
                    "per_page",
                    "must be between 1 and 100",
                ));
            }
            Ok(())
        }
    }

    fn body(key: &str, value: &str) -> HttpBody {
        let payload = KeyValue {
            key: key.into(),
            value: value.into(),
        };
        HttpBody::boxed(HttpBody::json(payload).unwrap())
    }

    #[tokio::test]
    async fn it_extracts_valid_payload() {
        let valid = Valid::<Json<KeyValue>>::from_payload(Payload::Body(body("name", "John")))
            .await
            .unwrap();

        assert_eq!(valid.key, "name");
        assert_eq!(valid.into_inner().into_inner().value, "John");
    }

    #[tokio::test]
    async fn it_rejects_invalid_payload() {
        let err = Valid::<Json<KeyValue>>::from_payload(Payload::Body(body("", "0123456789")))
            .await
            .err()
            .unwrap();

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            err.to_string(),
            "key: key is required; value: value is too long"
        );
    }

    #[tokio::test]
    async fn it_returns_extractor_error_without_validating() {
        let body = HttpBody::boxed(HttpBody::json("not a key value").unwrap());
        let err = Valid::<Json<KeyValue>>::from_payload(Payload::Body(body))
            .await
            .err()
            .unwrap();

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert!(err.to_string().starts_with("JSON parsing error"));
    }

    #[tokio::test]
    async fn it_validates_query() {
        let req = Request::get("/items?per_page=25").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let valid = ValidQuery::<Filter>::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();

        assert_eq!(valid.per_page, 25);
    }

    #[tokio::test]
    async fn it_rejects_invalid_query() {
        let req = Request::get("/items?per_page=1000000").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let err = ValidQuery::<Filter>::from_payload(Payload::Parts(&parts))
            .await
            .err()
            .unwrap();

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.to_string(), "per_page: must be between 1 and 100");
    }

    #[test]
    fn it_validates_when_extracted_from_parts() {
        let req = Request::get("/items?per_page=0").body(()).unwrap();
        let (parts, _) = req.into_parts();

        assert!(Valid::<Query<Filter>>::from_parts(&parts).is_err());
    }

    #[test]
    fn it_validates_when_extracted_from_request_ref() {
        let req = Request::get("/items?per_page=25")
            .body(HttpBody::empty())
            .unwrap();
        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let valid = Valid::<Query<Filter>>::from_request(&req).unwrap();

        assert_eq!(valid.per_page, 25);
        assert_eq!(valid.to_string(), "25");

        let req = Request::get("/items?per_page=0")
            .body(HttpBody::empty())
            .unwrap();
        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        assert!(Valid::<Query<Filter>>::from_request(&req).is_err());
    }

    #[test]
    fn it_derefs_to_the_wrapped_extractor() {
        let mut valid = Valid(Query(Filter { per_page: 25 }));

        valid.0.per_page = 50;

        assert_eq!(valid.per_page, 50);
        assert_eq!(valid.into_inner().into_inner().per_page, 50);
    }

    #[test]
    fn it_forwards_the_source_of_the_inner_extractor() {
        assert_eq!(Valid::<Json<KeyValue>>::SOURCE, Source::Body);
        assert_eq!(Valid::<Query<Filter>>::SOURCE, Source::Parts);
    }
}
