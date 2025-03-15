//! Extractors for route/path segments

use crate::{HttpRequest, error::Error};
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use serde::de::DeserializeOwned;

use std::{
    borrow::Cow,
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut},
    str::FromStr
};

use crate::http::endpoints::{
    route::PathArguments,
    args::{
        FromPayload, 
        FromRequestParts, 
        FromRequestRef, 
        Payload, Source
    }
};

/// Wraps typed data extracted from path args
/// 
/// # Example
/// ```no_run
/// use volga::{HttpResult, Path, ok};
/// use serde::Deserialize;
/// 
/// #[derive(Deserialize)]
/// struct Params {
///     name: String,
/// }
/// 
/// async fn handle(params: Path<Params>) -> HttpResult {
///     ok!("Hello {}", params.name)
/// }
/// ```
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path<T>(pub T);

impl<T> Path<T> {
    /// Unwraps the inner `T`
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Path<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Path<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Display> Display for Path<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: DeserializeOwned> Path<T> {
    /// Parses the slice of tuples `(String, String)` into [`Path<T>`]
    #[inline]
    pub(crate) fn from_slice(route_params: &[(Cow<str>, Cow<str>)]) -> Result<Self, Error> {
        let route_str = route_params
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");
        
        serde_urlencoded::from_str::<T>(&route_str)
            .map(Path)
            .map_err(PathError::from_serde_error)
    }
}

impl<T: DeserializeOwned + Send> TryFrom<&Extensions> for Path<T> {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Error> {
        extensions
            .get::<PathArguments>()
            .ok_or_else(PathError::args_missing)
            .and_then(|params| Self::from_slice(params))
    }
}

impl<T: DeserializeOwned + Send> TryFrom<&Parts> for Path<T> {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Error> {
        let ext = &parts.extensions;
        ext.try_into()
    }
}

/// Extracts path args from request parts into `Path<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromRequestParts for Path<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts.try_into()
    }
}

/// Extracts path args from request into `Path<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromRequestRef for Path<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }
}

/// Extracts path args from request parts into `Path<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromPayload for Path<T> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(parts.try_into())
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

/// Extracts path args directly into handler method args that implements `FromStr`
impl<T: FromStr + Send> FromPayload for T {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Path((arg, value)) = payload else { unreachable!() };
        ready(value
            .parse::<T>()
            .map_err(|_| PathError::type_mismatch(arg)))
    }

    #[inline]
    fn source() -> Source {
        Source::Path
    }
}

/// Describes errors of path extractor
struct PathError;

impl PathError {
    #[inline]
    fn from_serde_error(err: serde::de::value::Error) -> Error {
        Error::client_error(format!("Path parsing error: {}", err))
    }

    #[inline]
    fn type_mismatch(arg: &str) -> Error {
        Error::client_error(format!("Path parsing error: argument `{arg}` type mismatch"))
    }

    #[inline]
    fn args_missing() -> Error {
        Error::client_error("Path parsing error: missing arguments")
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Request, http::Extensions};
    use serde::Deserialize;
    use std::borrow::Cow;
    use crate::{HttpBody, HttpRequest, Path};
    use crate::http::endpoints::route::PathArguments;
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};

    #[derive(Deserialize)]
    struct Params {
        id: u32,
        name: String
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let param = (Cow::Borrowed("id"), Cow::Borrowed("123"));
        
        let id = i32::from_payload(Payload::Path(&param)).await.unwrap();
        
        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_path_from_payload() {
        let args: PathArguments = vec![
            (Cow::Borrowed("id"), Cow::Borrowed("123")),
            (Cow::Borrowed("name"), Cow::Borrowed("John"))
        ].into_boxed_slice();

        let req = Request::get("/")
            .extension(args)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = Path::<Params>::from_payload(Payload::Parts(&parts)).await.unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }
    
    #[test]
    fn it_parses_slice() {
        let slice = [
            (Cow::Borrowed("id"), Cow::Borrowed("123")),
            (Cow::Borrowed("name"), Cow::Borrowed("John"))
        ];
        
        let path = Path::<Params>::from_slice(&slice).unwrap();
        
        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[test]
    fn it_parses_request_extensions() {
        let args: PathArguments = vec![
            (Cow::Borrowed("id"), Cow::Borrowed("123")),
            (Cow::Borrowed("name"), Cow::Borrowed("John"))
        ].into_boxed_slice();
        
        let mut ext = Extensions::new();
        ext.insert(args);

        let path = Path::<Params>::try_from(&ext).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_parts() {
        let args: PathArguments = vec![
            (Cow::Borrowed("id"), Cow::Borrowed("123")),
            (Cow::Borrowed("name"), Cow::Borrowed("John"))
        ].into_boxed_slice();

        let req = Request::get("/")
            .extension(args)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = Path::<Params>::from_parts(&parts).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_request_ref() {
        let args: PathArguments = vec![
            (Cow::Borrowed("id"), Cow::Borrowed("123")),
            (Cow::Borrowed("name"), Cow::Borrowed("John"))
        ].into_boxed_slice();

        let req = Request::get("/")
            .extension(args)
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);
        let path = Path::<Params>::from_request(&req).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }
}
