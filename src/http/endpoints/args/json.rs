//! Extractors for typed JSON data

use futures_util::ready;
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;

use http_body_util::{combinators::Collect, BodyExt};
use serde::Serialize;
use crate::{error::Error, HttpBody};

use std::{
    future::Future,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll}
};

use crate::http::{
    endpoints::args::{
        FromPayload,
        Payload,
        Source
    }
};

/// Wraps typed JSON data
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Json, ok};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct User {
///     name: String,
/// }
///
/// async fn handle(user: Json<User>) -> HttpResult {
///     ok!("Hello {}", user.name)
/// }
/// ```
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Unwraps the inner `T`
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Serialize> From<T> for Json<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Display> Display for Json<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

pin_project! {
    /// A future that collects an incoming body stream into bytes and deserializes it into a JSON object.
    pub struct ExtractJsonPayloadFut<T> {
        #[pin]
        fut: Collect<HttpBody>,
        _marker: PhantomData<T>
    }
}

impl<T: DeserializeOwned + Send> Future for ExtractJsonPayloadFut<T> {
    type Output = Result<Json<T>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let result = ready!(this.fut.poll(cx))
            .map_err(JsonError::collect_error)?;
        let body = result.to_bytes();
        let json = serde_json::from_slice(&body)
            .map(Json::<T>)
            .map_err(JsonError::from_serde_error);
        Poll::Ready(json)
    }
}

/// Extracts JSON data from request body into `Json<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromPayload for Json<T> {
    type Future = ExtractJsonPayloadFut<T>;

    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Body(body) = payload {
            ExtractJsonPayloadFut { fut: body.collect(), _marker: PhantomData }
        } else {
            unreachable!()
        }
    }

    fn source() -> Source {
        Source::Body
    }
}

struct JsonError;

impl JsonError {
    #[inline]
    fn from_serde_error(err: serde_json::Error) -> Error {
        Error::client_error(format!("JSON parsing error: {}", err))
    }

    #[inline]
    fn collect_error(err: std::io::Error) -> Error {
        Error::client_error(format!("JSON parsing error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use crate::HttpBody;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use super::Json;
    
    #[derive(Serialize, Deserialize)]
    struct User {
        age: i32,
        name: String,
    }
    
    #[tokio::test]
    async fn it_reads_from_payload() {
        let user = User { age: 33, name: "John".into() };
        let body = HttpBody::boxed(HttpBody::json(user));
        
        let user = Json::<User>::from_payload(Payload::Body(body)).await.unwrap();
        
        assert_eq!(user.age, 33);
        assert_eq!(user.name, "John");
    }
    
    #[test]
    fn it_converts_to_json() {
        let user = User { age: 33, name: "John".into() };
        let json: Json<User> = user.into();

        assert_eq!(json.age, 33);
        assert_eq!(json.name, "John");
    }
}