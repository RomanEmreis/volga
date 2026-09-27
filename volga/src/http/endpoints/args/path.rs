//! Extractors for route/path segments

use crate::{HttpRequest, error::Error, http::request_scope::HttpRequestScope};
use futures_util::future::{Ready, ready};
use hyper::http::{Extensions, request::Parts};
use serde::de::DeserializeOwned;

use std::{
    borrow::Cow,
    ffi::{CString, OsString},
    fmt::{self, Display, Formatter},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    num::NonZero,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use crate::http::endpoints::{
    args::{
        FromPathArg, FromPathArgs, FromPayload, FromRequestParts, FromRequestRef, Payload, Source,
    },
    route::{PathArg, PathArgs},
};

/// `Path<T>` extracts route parameters into a positional tuple `T`, or into a single
/// [`FromPathArg`] type on a route declaring one parameter, without consuming the
/// underlying path arguments.
///
/// This extractor operates on a snapshot of the matched path arguments.
/// The original path state remains available to other extractors.
///
/// **Warning:** This extractor must not be mixed with [`NamedPath<T>`] or
/// positional path parameters (e.g. `x: i32`) within the same handler.
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Path, ok};
///
/// // https://www.example.com/api/hello/{name}/{age}
/// async fn handle(
///     Path((name, age)): Path<(String, u32)>
/// ) -> HttpResult {
///     ok!("Hello {name}, you are {age} years old.")
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path<T>(pub T);

/// Unlike [`Path<T>`], this extractor deserializes parameters into a named
/// struct, preserving parameter names.
///
/// Unlike [`Path<T>`] as well, it decodes the percent-escapes in a value: `%20` is read as
/// a space and `%2F` as a `/`. A `+` or an `&` is read as it is written.
///
/// This extractor operates on a snapshot of the matched path arguments.
/// The original path state remains available to other extractors.
///
/// **Warning:** This extractor must not be mixed with [`Path<T>`] or
/// positional path parameters (e.g. `x: i32`) within the same handler.
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, NamedPath, ok};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Params {
///     name: String,
///     age: u32
/// }
///
/// // https://www.example.com/api/hello/{name}/{age}
/// async fn handle(
///     NamedPath(Params { name, age }): NamedPath<Params>
/// ) -> HttpResult {
///     ok!("Hello {name}, you are {age} years old.")
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamedPath<T: DeserializeOwned>(pub T);

impl<T> Path<T> {
    /// Unwraps the inner `T`
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: DeserializeOwned> NamedPath<T> {
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

impl<T: DeserializeOwned> Deref for NamedPath<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: DeserializeOwned> DerefMut for NamedPath<T> {
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

impl<T: DeserializeOwned + Display> Display for NamedPath<T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: FromPathArgs> Path<T> {
    /// Parses the slice of tuples `(String, String)` into [`Path<T>`]
    #[inline]
    pub(crate) fn from_slice(route_params: &PathArgs) -> Result<Self, Error> {
        T::from_path_args(route_params).map(Self)
    }
}

impl<T: DeserializeOwned> NamedPath<T> {
    /// Parses the slice of tuples `(String, String)` into [`Path<T>`]
    #[inline]
    pub(crate) fn from_slice(route_params: &PathArgs) -> Result<Self, Error> {
        let route_str = route_params.encoded()?;
        serde_urlencoded::from_str::<T>(route_str)
            .map(Self)
            .map_err(PathError::from_serde_error)
    }
}

/// Returns a reference to the `PathArgs` stored in the `HttpRequestScope` extension.
#[inline]
fn path_args_from_extensions(extensions: &Extensions) -> Result<&PathArgs, Error> {
    extensions
        .get::<HttpRequestScope>()
        .map(|s| &s.params)
        .ok_or_else(PathError::args_missing)
}

impl<T: FromPathArgs + Send> TryFrom<&Extensions> for Path<T> {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Error> {
        path_args_from_extensions(extensions).and_then(Self::from_slice)
    }
}

impl<T: DeserializeOwned + Send> TryFrom<&Extensions> for NamedPath<T> {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Error> {
        path_args_from_extensions(extensions).and_then(Self::from_slice)
    }
}

impl<T: FromPathArgs + Send> TryFrom<&Parts> for Path<T> {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Error> {
        let ext = &parts.extensions;
        ext.try_into()
    }
}

impl<T: DeserializeOwned + Send> TryFrom<&Parts> for NamedPath<T> {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Error> {
        let ext = &parts.extensions;
        ext.try_into()
    }
}

impl<T: FromPathArgs + Send> FromRequestParts for Path<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts.try_into()
    }
}

impl<T: DeserializeOwned + Send> FromRequestParts for NamedPath<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts.try_into()
    }
}

impl<T: FromPathArgs + Send> FromRequestRef for Path<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        path_args_from_extensions(req.extensions()).and_then(Self::from_slice)
    }
}

impl<T: DeserializeOwned + Send> FromRequestRef for NamedPath<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        path_args_from_extensions(req.extensions()).and_then(Self::from_slice)
    }
}

/// Extracts path args from request parts into `Path<T>`
/// where T is a tuple
impl<T: FromPathArgs + Send> FromPayload for Path<T> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::PathArgs;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::PathArgs(params) = payload else {
            unreachable!()
        };
        ready(Self::from_slice(params))
    }
}

/// Extracts path args from request parts into `NamedPath<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromPayload for NamedPath<T> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::PathArgs;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::PathArgs(params) = payload else {
            unreachable!()
        };
        ready(Self::from_slice(params))
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config.consumes_named_path::<T>()
    }
}

impl FromPathArg for String {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.as_ref().to_owned())
    }

    #[inline]
    fn from_owned_path_arg(arg: PathArg) -> Result<Self, Error> {
        Ok(arg.value.into_string())
    }
}

impl FromPathArg for Cow<'static, str> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(Cow::Owned(arg.value.as_ref().to_owned()))
    }

    #[inline]
    fn from_owned_path_arg(arg: PathArg) -> Result<Self, Error> {
        Ok(Cow::Owned(arg.value.into_string()))
    }
}

impl FromPathArg for Box<str> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.clone())
    }

    #[inline]
    fn from_owned_path_arg(arg: PathArg) -> Result<Self, Error> {
        Ok(arg.value)
    }
}

impl FromPathArg for Box<[u8]> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.as_bytes().into())
    }

    #[inline]
    fn from_owned_path_arg(arg: PathArg) -> Result<Self, Error> {
        Ok(arg.value.into_boxed_bytes())
    }
}

macro_rules! impl_from_path_arg {
    { $($type:ty),* $(,)? } => {
        $(impl FromPathArg for $type {
            #[inline]
            fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
                arg.parse::<$type>()
            }
        })*
    };
}

impl_from_path_arg! {
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

#[cfg(feature = "uuid")]
impl_from_path_arg! { uuid::Uuid }

/// A type read from one path argument is a handler argument of its own
impl<T: FromPathArg + Send> FromPayload for T {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Path;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        // The dispatch hands out one argument per positional extractor, and runs out when
        // the handler takes more of them than the route declares
        let Payload::Path(arg) = payload else {
            return ready(Err(PathError::more_extractors_than_args()));
        };
        ready(T::from_owned_path_arg(arg))
    }
}

/// A type read from one path argument is the `T` of `Path<T>` on a route declaring exactly
/// one parameter.
///
/// It does not pick the first of several: on `/users/{user_id}/orders/{order_id}`, a
/// `Path<OrderId>` would read the user's id as the order's.
impl<T: FromPathArg> FromPathArgs for T {
    #[inline]
    fn from_path_args(args: &PathArgs) -> Result<Self, Error> {
        let mut it = args.iter();
        match (it.next(), it.len()) {
            (Some(arg), 0) => T::from_path_arg(arg),
            _ => Err(PathError::not_a_single_arg(args.len())),
        }
    }
}

macro_rules! impl_tuple_path {
    ($($T:ident),+) => {
        impl<$($T),+> FromPathArgs for ($($T,)+)
        where
            $($T: FromPathArg),+
        {
            #[inline]
            #[allow(non_snake_case)]
            fn from_path_args(args: &PathArgs) -> Result<Self, Error> {
                let mut it = args.iter();
                $(
                    let arg = it.next().ok_or_else(PathError::args_missing)?;
                    let $T = <$T as FromPathArg>::from_path_arg(arg)?;
                )+
                Ok(($($T,)+))
            }
        }
    };
}

impl_tuple_path! { T1 }
impl_tuple_path! { T1, T2 }
impl_tuple_path! { T1, T2, T3 }
impl_tuple_path! { T1, T2, T3, T4 }
impl_tuple_path! { T1, T2, T3, T4, T5 }
impl_tuple_path! { T1, T2, T3, T4, T5, T6 }
impl_tuple_path! { T1, T2, T3, T4, T5, T6, T7 }
impl_tuple_path! { T1, T2, T3, T4, T5, T6, T7, T8 }
impl_tuple_path! { T1, T2, T3, T4, T5, T6, T7, T8, T9 }
impl_tuple_path! { T1, T2, T3, T4, T5, T6, T7, T8, T9, T10 }

/// Describes errors of path extractor
struct PathError;

impl PathError {
    #[inline]
    fn from_serde_error(err: serde::de::value::Error) -> Error {
        Error::client_error(format!("Path parsing error: {err}"))
    }

    #[inline]
    fn args_missing() -> Error {
        Error::client_error("Path parsing error: missing arguments")
    }

    /// The handler and its route disagree, which no request can fix
    #[cold]
    fn more_extractors_than_args() -> Error {
        Error::server_error(
            "Path parsing error: the handler reads more path parameters than the route declares",
        )
    }

    /// The route and `Path<T>` disagree, which no request can fix
    #[cold]
    fn not_a_single_arg(declared: usize) -> Error {
        Error::server_error(format!(
            "Path parsing error: `Path<T>` of a single type reads one path parameter, but the \
            route declares {declared}; read them as a tuple, `Path<(..)>`, or by name, \
            `NamedPath<T>`"
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::http::endpoints::args::{
        FromPathArg, FromPayload, FromRequestParts, FromRequestRef, Payload,
    };
    use crate::http::endpoints::route::{PathArg, PathArgs};
    use crate::http::request_scope::HttpRequestScope;
    use crate::{HttpBody, HttpRequest, NamedPath, Path};
    use hyper::{Request, http::Extensions};
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Params {
        id: u32,
        name: String,
    }

    fn create_path_args() -> PathArgs {
        smallvec::smallvec![
            PathArg {
                name: "id".into(),
                value: "123".into()
            },
            PathArg {
                name: "name".into(),
                value: "John".into()
            }
        ]
        .into()
    }

    #[tokio::test]
    async fn it_reads_isize_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = isize::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_isize_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = isize::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i8_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i8::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i8_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i8::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i16_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i16::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i16_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i16::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i32_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i32::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i32_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i32::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i64_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i64_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i64::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i128_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i128::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i128_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = i128::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_usize_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = usize::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_usize_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = usize::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u8_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u8::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u8_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u8::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u16_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u16::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u16_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u16::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u32_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u32::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u32_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u32::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u64_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u64_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u64::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u128_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u128::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u128_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = u128::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_string_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = String::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, "123");
    }

    #[test]
    fn it_reads_string_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = String::from_path_arg(&param).unwrap();

        assert_eq!(id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_str_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = Box::<str>::from_payload(Payload::Path(param))
            .await
            .unwrap();

        assert_eq!(&*id, "123");
    }

    #[test]
    fn it_reads_box_str_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = Box::<str>::from_path_arg(&param).unwrap();

        assert_eq!(&*id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_bytes_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = Box::<[u8]>::from_payload(Payload::Path(param))
            .await
            .unwrap();

        assert_eq!(&*id, [b'1', b'2', b'3']);
    }

    #[test]
    fn it_reads_box_bytes_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };
        let id = Box::<[u8]>::from_path_arg(&param).unwrap();

        assert_eq!(&*id, [b'1', b'2', b'3']);
    }

    #[tokio::test]
    async fn it_reads_f32_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "12.3".into(),
        };
        let id = f32::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 12.3);
    }

    #[test]
    fn it_reads_f32_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "12.3".into(),
        };
        let id = f32::from_path_arg(&param).unwrap();

        assert_eq!(id, 12.3);
    }

    #[tokio::test]
    async fn it_reads_f64_from_payload() {
        let param = PathArg {
            name: "id".into(),
            value: "12.3".into(),
        };
        let id = f64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 12.3);
    }

    #[test]
    fn it_reads_f64_from_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "12.3".into(),
        };
        let id = f64::from_path_arg(&param).unwrap();

        assert_eq!(id, 12.3);
    }

    #[tokio::test]
    async fn it_reads_bool_from_payload() {
        let param = PathArg {
            name: "flag".into(),
            value: "true".into(),
        };
        let flag = bool::from_payload(Payload::Path(param)).await.unwrap();

        assert!(flag);
    }

    #[test]
    fn it_reads_bool_from_path_arg() {
        let param = PathArg {
            name: "flag".into(),
            value: "true".into(),
        };
        let flag = bool::from_path_arg(&param).unwrap();

        assert!(flag);
    }

    #[tokio::test]
    async fn it_reads_char_from_payload() {
        let param = PathArg {
            name: "char".into(),
            value: "a".into(),
        };
        let char = char::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(char, 'a');
    }

    #[test]
    fn it_reads_char_from_path_arg() {
        let param = PathArg {
            name: "char".into(),
            value: "a".into(),
        };
        let char = char::from_path_arg(&param).unwrap();

        assert_eq!(char, 'a');
    }

    #[tokio::test]
    async fn it_reads_named_path_from_payload() {
        let args = create_path_args();

        let path = NamedPath::<Params>::from_payload(Payload::PathArgs(&args))
            .await
            .unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_payload() {
        let args = create_path_args();

        let path = Path::<(u32, String)>::from_payload(Payload::PathArgs(&args))
            .await
            .unwrap()
            .0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[test]
    fn it_parses_named_path_from_slice() {
        let args = create_path_args();

        let path = NamedPath::<Params>::from_slice(&args).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[test]
    fn it_parses_path_from_slice() {
        let args = create_path_args();

        let path = Path::<(u32, String)>::from_slice(&args).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[test]
    fn it_parses_named_path_from_request_extensions() {
        let args = create_path_args();

        let mut ext = Extensions::new();
        ext.insert(HttpRequestScope {
            params: args,
            ..HttpRequestScope::default()
        });

        let path = NamedPath::<Params>::try_from(&ext).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[test]
    fn it_parses_path_from_request_extensions() {
        let args = create_path_args();

        let mut ext = Extensions::new();
        ext.insert(HttpRequestScope {
            params: args,
            ..HttpRequestScope::default()
        });

        let path = Path::<(u32, String)>::try_from(&ext).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[tokio::test]
    async fn it_reads_named_path_from_parts() {
        let args = create_path_args();

        let req = Request::get("/")
            .extension(HttpRequestScope {
                params: args,
                ..HttpRequestScope::default()
            })
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = NamedPath::<Params>::from_parts(&parts).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_parts() {
        let args = create_path_args();

        let req = Request::get("/")
            .extension(HttpRequestScope {
                params: args,
                ..HttpRequestScope::default()
            })
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = Path::<(u32, String)>::from_parts(&parts).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[tokio::test]
    async fn it_reads_named_path_from_request_ref() {
        let args = create_path_args();

        let req = Request::get("/")
            .extension(HttpRequestScope {
                params: args,
                ..HttpRequestScope::default()
            })
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);
        let path = NamedPath::<Params>::from_request(&req).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_request_ref() {
        let args = create_path_args();

        let req = Request::get("/")
            .extension(HttpRequestScope {
                params: args,
                ..HttpRequestScope::default()
            })
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);
        let path = Path::<(u32, String)>::from_request(&req).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[test]
    fn it_exposes_the_name_and_value_of_a_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "a%20b".into(),
        };

        assert_eq!(param.name(), "id");
        assert_eq!(param.value(), "a%20b");
    }

    #[test]
    fn it_parses_a_path_arg() {
        let param = PathArg {
            name: "id".into(),
            value: "123".into(),
        };

        assert_eq!(param.parse::<u64>().unwrap(), 123);
    }

    #[test]
    fn it_answers_400_for_a_path_arg_that_does_not_parse() {
        let param = PathArg {
            name: "id".into(),
            value: "nope".into(),
        };

        let err = param.parse::<u64>().unwrap_err();

        assert_eq!(err.status, hyper::StatusCode::BAD_REQUEST);
        assert!(err.to_string().contains("argument `id` type mismatch"));
    }

    #[test]
    fn it_iterates_path_args_in_order() {
        let args = create_path_args();

        assert_eq!(args.len(), 2);
        assert!(!args.is_empty());
        assert!(PathArgs::new().is_empty());

        let names: Vec<_> = args.iter().map(PathArg::name).collect();
        assert_eq!(names, ["id", "name"]);
    }

    #[test]
    fn it_reads_a_single_path_arg_type_as_path() {
        let args: PathArgs = std::iter::once(PathArg {
            name: "id".into(),
            value: "123".into(),
        })
        .collect();

        let Path(id) = Path::<u32>::from_slice(&args).unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_fails_to_read_a_single_path_arg_type_from_no_args() {
        let err = Path::<u32>::from_slice(&PathArgs::new()).unwrap_err();

        assert_eq!(err.status, hyper::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn it_does_not_read_a_single_path_arg_type_from_the_first_of_several() {
        let err = Path::<u32>::from_slice(&create_path_args()).unwrap_err();

        assert_eq!(err.status, hyper::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(err.to_string().contains("the route declares 2"));
    }

    #[tokio::test]
    async fn it_answers_500_for_a_positional_param_the_route_does_not_declare() {
        let err = u32::from_payload(Payload::None).await.unwrap_err();

        assert_eq!(err.status, hyper::StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn it_reads_none_for_an_optional_positional_param_the_route_does_not_declare() {
        let value = Option::<u32>::from_payload(Payload::None).await.unwrap();

        assert_eq!(value, None);
    }

    #[test]
    fn it_debugs_path_args_without_their_internal_state() {
        let args = create_path_args();
        let _ = args.encoded();

        let debug = format!("{args:?}");

        assert!(debug.contains("\"id\""));
        assert!(!debug.contains("encoded"));
    }

    #[test]
    fn it_fails_to_read_a_tuple_longer_than_the_args() {
        let err = Path::<(u32, String, u32)>::from_slice(&create_path_args()).unwrap_err();

        assert_eq!(err.status, hyper::StatusCode::BAD_REQUEST);
    }

    #[cfg(feature = "uuid")]
    mod uuid {
        use crate::Path;
        use crate::http::endpoints::args::{FromPathArg, FromPayload, Payload};
        use crate::http::endpoints::route::{PathArg, PathArgs};
        use uuid::Uuid;

        const ID: &str = "0199a0f1-1111-7000-8000-000000000001";

        fn arg(value: &str) -> PathArg {
            PathArg {
                name: "id".into(),
                value: value.into(),
            }
        }

        #[tokio::test]
        async fn it_reads_uuid_from_payload() {
            let id = Uuid::from_payload(Payload::Path(arg(ID))).await.unwrap();

            assert_eq!(id, Uuid::parse_str(ID).unwrap());
        }

        #[test]
        fn it_reads_uuid_from_path_arg() {
            let id = Uuid::from_path_arg(&arg(ID)).unwrap();

            assert_eq!(id, Uuid::parse_str(ID).unwrap());
        }

        #[test]
        fn it_reads_uuid_as_path() {
            let args: PathArgs = std::iter::once(arg(ID)).collect();

            let Path(id) = Path::<Uuid>::from_slice(&args).unwrap();

            assert_eq!(id, Uuid::parse_str(ID).unwrap());
        }

        #[test]
        fn it_answers_400_for_a_malformed_uuid() {
            let err = Uuid::from_path_arg(&arg("nope")).unwrap_err();

            assert_eq!(err.status, hyper::StatusCode::BAD_REQUEST);
        }
    }
}
