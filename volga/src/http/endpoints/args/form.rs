//! Extractors for Form Data

use crate::{error::Error, HttpBody};
use crate::http::endpoints::args::{FromPayload, Payload, Source};
use futures_util::ready;
use http_body_util::{combinators::Collect, BodyExt};
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;
use serde::Serialize;
use mime::APPLICATION_WWW_FORM_URLENCODED;
use std::{
    future::Future,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll}
};

/// Wraps typed data extracted from [`Uri`]
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Form, ok};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Params {
///     name: String,
/// }
///
/// async fn handle(params: Form<Params>) -> HttpResult {
///     ok!("Hello {}", params.name)
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Form<T>(pub T);

impl<T> Form<T> {
    /// Unwraps the inner `T`
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Serialize> From<T> for Form<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> Deref for Form<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Form<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Display> Display for Form<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pin_project! {
    /// A future that collects an incoming body stream into bytes and deserializes it into a Form Data object.
    pub struct ExtractFormPayloadFut<T> {
        #[pin]
        fut: Collect<HttpBody>,
        _marker: PhantomData<T>
    }
}

impl<T: DeserializeOwned + Send> Future for ExtractFormPayloadFut<T> {
    type Output = Result<Form<T>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = ready!(this.fut.poll(cx))
            .map_err(FormError::collect_error)?;
        let body = result.to_bytes();
        let form = serde_urlencoded::from_bytes(&body)
            .map(Form::<T>)
            .map_err(FormError::from_serde_error);
        Poll::Ready(form)
    }
}

/// Extracts body into `Form<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromPayload for Form<T> {
    type Future = ExtractFormPayloadFut<T>;

    const SOURCE: Source = Source::Body;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Body(body) = payload else { unreachable!() };
        ExtractFormPayloadFut { fut: body.collect(), _marker: PhantomData }
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(config: crate::openapi::OpenApiRouteConfig) -> crate::openapi::OpenApiRouteConfig {
        config.with_request_type_from_deserialize::<T>(APPLICATION_WWW_FORM_URLENCODED.as_ref())
    }
}

/// Describes errors of form data extractor
struct FormError;
impl FormError {
    #[inline]
    fn from_serde_error(err: serde::de::value::Error) -> Error {
        Error::client_error(format!("Form Data parsing error: {err}"))
    }

    #[inline]
    fn collect_error(err: Error) -> Error {
        Error::client_error(format!("Form Data parsing error: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use serde::{Deserialize, Serialize};
    use super::Form;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use crate::HttpBody;

    #[derive(Serialize, Deserialize)]
    struct User {
        name: String,
        age: i32
    }

    #[derive(Serialize, Deserialize)]
    struct OptionalUser {
        name: Option<String>,
        age: Option<i32>
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let user = User { age: 33, name: "John".into() };
        let body = HttpBody::boxed(HttpBody::form(user).unwrap());

        let user = Form::<User>::from_payload(Payload::Body(body)).await.unwrap();

        assert_eq!(user.age, 33);
        assert_eq!(user.name, "John");
    }

    #[tokio::test]
    async fn it_reads_optional_from_payload() {
        let user = OptionalUser { name: Some("John".into()), age: None };
        let body = HttpBody::boxed(HttpBody::form(user).unwrap());

        let user = Form::<OptionalUser>::from_payload(Payload::Body(body)).await.unwrap();

        assert!(user.age.is_none());
        assert_eq!(user.0.name.unwrap(), "John");
    }

    #[tokio::test]
    async fn it_reads_hash_map_from_payload() {
        let user_map = HashMap::from([
            ("age", "33"),
            ("name", "John")
        ]);
        let body = HttpBody::boxed(HttpBody::form(user_map).unwrap());

        let user = Form::<HashMap<String, String>>::from_payload(Payload::Body(body)).await.unwrap();

        assert_eq!(user.get("age").unwrap(), "33");
        assert_eq!(user.get("name").unwrap(), "John");
    }

    #[tokio::test]
    async fn it_reads_hash_map_optional_from_payload() {
        let user_map = HashMap::from([
            ("name", "John")
        ]);
        let body = HttpBody::boxed(HttpBody::form(user_map).unwrap());

        let user = Form::<HashMap<String, String>>::from_payload(Payload::Body(body)).await.unwrap();

        assert!(user.get("age").is_none());
        assert_eq!(user.get("name").unwrap(), "John");
    }

    #[test]
    fn it_converts_to_form() {
        let user = User { age: 33, name: "John".into() };
        let form: Form<User> = user.into();

        assert_eq!(form.age, 33);
        assert_eq!(form.name, "John");
    }

    #[test]
    fn it_derefs_mut() {
        let user = User { age: 33, name: "John".into() };
        let mut form: Form<User> = user.into();

        *form = User { age: 30, name: "Jack".into() };
        
        assert_eq!(form.age, 30);
        assert_eq!(form.name, "Jack");
    }
}