//! Extractors for route/path segments

use crate::{HttpRequest, error::Error};
use futures_util::future::{ready, ok, Ready};
use hyper::http::{request::Parts, Extensions};
use serde::de::DeserializeOwned;

use std::{
    net::{IpAddr, SocketAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6},
    ffi::{CString, OsString},
    path::PathBuf,
    num::NonZero,
    borrow::Cow,
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut}
};

use crate::http::endpoints::{
    route::{PathArg, PathArgs},
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
    pub(crate) fn from_slice(route_params: &PathArgs) -> Result<Self, Error> {
        let route_str = route_params
            .iter()
            .map(PathArg::query_format)
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
            .get::<PathArgs>()
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

impl FromPayload for String {
    type Future = Ready<Result<Self, Error>>;
    
    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.to_string())
    }
    
    #[inline]
    fn source() -> Source {
        Source::Path
    }
}

impl FromPayload for Cow<'static, str> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(Cow::Owned(param.value.to_string()))
    }

    #[inline]
    fn source() -> Source {
        Source::Path
    }
}

impl FromPayload for Box<str> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.clone())
    }

    #[inline]
    fn source() -> Source {
        Source::Path
    }
}

impl FromPayload for Box<[u8]> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.clone().into_boxed_bytes())
    }

    #[inline]
    fn source() -> Source {
        Source::Path
    }
}

macro_rules! impl_from_payload {
    { $($type:ty),* $(,)? } => {
        $(impl FromPayload for $type {
            type Future = Ready<Result<Self, Error>>;
            #[inline]
            fn from_payload(payload: Payload) -> Self::Future {
                let Payload::Path(param) = payload else { unreachable!() };
                ready(param.value
                    .parse::<$type>()
                    .map_err(|_| PathError::type_mismatch(param.name.as_ref())))
            }
            #[inline]
            fn source() -> Source {
                Source::Path
            }
        })*
    };
}

impl_from_payload! {
    bool, 
    char,
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    f32, f64,
    NonZero<i8>, NonZero<i16>, NonZero<i32>, NonZero<i64>, NonZero<i128>, NonZero<isize>,
    NonZero<u8>, NonZero<u16>, NonZero<u32>, NonZero<u64>, NonZero<u128>, NonZero<usize>,
    IpAddr, SocketAddr, Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6,
    CString, OsString,
    PathBuf
}

/// Describes errors of path extractor
struct PathError;

impl PathError {
    #[inline]
    fn from_serde_error(err: serde::de::value::Error) -> Error {
        Error::client_error(format!("Path parsing error: {err}"))
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
    use crate::{HttpBody, HttpRequest, Path};
    use crate::http::endpoints::route::{PathArg, PathArgs};
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};

    #[derive(Deserialize)]
    struct Params {
        id: u32,
        name: String
    }

    #[tokio::test]
    async fn it_reads_isize_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = isize::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i8_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i8::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i16_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i16::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i32_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i32::from_payload(Payload::Path(&param)).await.unwrap();
        
        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i64_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i64::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i128_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i128::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_usize_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = usize::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u8_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u8::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u16_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u16::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u32_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u32::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u128_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u128::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_string_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = String::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_str_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<str>::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(&*id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_bytes_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<[u8]>::from_payload(Payload::Path(&param)).await.unwrap();

        assert_eq!(&*id, [b'1', b'2', b'3']);
    }

    #[tokio::test]
    async fn it_reads_path_from_payload() {
        let args: PathArgs = vec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];

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
        let args: PathArgs = vec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];
        
        let path = Path::<Params>::from_slice(&args).unwrap();
        
        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[test]
    fn it_parses_request_extensions() {
        let args: PathArgs = vec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];
        
        let mut ext = Extensions::new();
        ext.insert(args);

        let path = Path::<Params>::try_from(&ext).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_parts() {
        let args: PathArgs = vec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];

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
        let args: PathArgs = vec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];

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
