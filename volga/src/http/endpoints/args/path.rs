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
        FromPathArgs,
        FromPathArg,
        Payload, Source
    }
};

/// `Path<T>` extracts route parameters into a positional tuple `T`
/// without consuming the underlying path arguments.
///
/// This extractor operates on a snapshot of the matched path arguments.
/// The original path state remains available to other extractors.
///
/// ⚠️ This extractor must not be mixed with [`NamedPath<T>`] or
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
/// This extractor operates on a snapshot of the matched path arguments.
/// The original path state remains available to other extractors.
///
/// ⚠️ This extractor must not be mixed with [`Path<T>`] or
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

impl<T: FromPathArgs> Path<T>  {
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

impl<T: FromPathArgs + Send> TryFrom<&Extensions> for Path<T> {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Error> {
        extensions
            .get::<PathArgs>()
            .ok_or_else(PathError::args_missing)
            .and_then(Self::from_slice)
    }
}

impl<T: DeserializeOwned + Send> TryFrom<&Extensions> for NamedPath<T> {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Error> {
        extensions
            .get::<PathArgs>()
            .ok_or_else(PathError::args_missing)
            .and_then(Self::from_slice)
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
        let args = req
            .extensions()
            .get::<PathArgs>()
            .ok_or_else(PathError::args_missing)?;
        Self::from_slice(args)
    }
}

impl<T: DeserializeOwned + Send> FromRequestRef for NamedPath<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        let args = req
            .extensions()
            .get::<PathArgs>()
            .ok_or_else(PathError::args_missing)?;
        Self::from_slice(args)
    }
}

/// Extracts path args from request parts into `Path<T>`
/// where T is a tuple
impl<T: FromPathArgs + Send> FromPayload for Path<T> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::PathArgs;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::PathArgs(params) = payload else { unreachable!() };
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
        let Payload::PathArgs(params) = payload else { unreachable!() };
        ready(Self::from_slice(params))
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config.consumes_named_path::<T>()
    }
}

impl FromPayload for String {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Path;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.as_ref().to_owned())
    }
}

impl FromPathArg for String {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.as_ref().to_owned())
    }
}

impl FromPayload for Cow<'static, str> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Path;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(Cow::Owned(param.value.as_ref().to_owned()))
    }
}

impl FromPathArg for Cow<'static, str> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(Cow::Owned(arg.value.as_ref().to_owned()))
    }
}

impl FromPayload for Box<str> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Path;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.as_ref().into())
    }
}

impl FromPathArg for Box<str> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.as_ref().into())
    }
}

impl FromPayload for Box<[u8]> {
    type Future = Ready<Result<Self, Error>>;
    
    const SOURCE: Source = Source::Path;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Path(param) = payload else { unreachable!() };
        ok(param.value.as_bytes().into())
    }
}

impl FromPathArg for Box<[u8]> {
    #[inline]
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
        Ok(arg.value.as_bytes().into())
    }
}

macro_rules! impl_from_payload {
    { $($type:ty),* $(,)? } => {
        $(impl FromPathArg for $type {
            #[inline]
            fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
                arg.value.parse::<$type>()
                    .map_err(|_| PathError::type_mismatch(arg.name.as_ref()))
            }
        })*
        $(impl FromPayload for $type {
            type Future = Ready<Result<Self, Error>>;
            const SOURCE: Source = Source::Path;
            #[inline]
            fn from_payload(payload: Payload<'_>) -> Self::Future {
                let Payload::Path(arg) = payload else { unreachable!() };
                ready(<$type as FromPathArg>::from_path_arg(&arg))
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
    use crate::{HttpBody, HttpRequest, Path, NamedPath};
    use crate::http::endpoints::route::{PathArg, PathArgs};
    use crate::http::endpoints::args::{FromPathArg, FromPayload, FromRequestParts, FromRequestRef, Payload};

    #[derive(Deserialize)]
    struct Params {
        id: u32,
        name: String
    }
    
    fn create_path_args() -> PathArgs {
        smallvec::smallvec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ].into()
    }

    #[tokio::test]
    async fn it_reads_isize_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = isize::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_isize_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = isize::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i8_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i8::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i8_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i8::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i16_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i16::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i16_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i16::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }
    
    #[tokio::test]
    async fn it_reads_i32_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i32::from_payload(Payload::Path(param)).await.unwrap();
        
        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i32_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i32::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i64_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i64_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i64::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_i128_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i128::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_i128_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = i128::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_usize_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = usize::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_usize_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = usize::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u8_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u8::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u8_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u8::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u16_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u16::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u16_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u16::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u32_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u32::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u32_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u32::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u64_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u64_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u64::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_u128_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u128::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 123);
    }

    #[test]
    fn it_reads_u128_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = u128::from_path_arg(&param).unwrap();

        assert_eq!(id, 123);
    }

    #[tokio::test]
    async fn it_reads_string_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = String::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, "123");
    }

    #[test]
    fn it_reads_string_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = String::from_path_arg(&param).unwrap();

        assert_eq!(id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_str_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<str>::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(&*id, "123");
    }

    #[test]
    fn it_reads_box_str_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<str>::from_path_arg(&param).unwrap();

        assert_eq!(&*id, "123");
    }

    #[tokio::test]
    async fn it_reads_box_bytes_from_payload() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<[u8]>::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(&*id, [b'1', b'2', b'3']);
    }

    #[test]
    fn it_reads_box_bytes_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "123".into() };
        let id = Box::<[u8]>::from_path_arg(&param).unwrap();

        assert_eq!(&*id, [b'1', b'2', b'3']);
    }

    #[tokio::test]
    async fn it_reads_f32_from_payload() {
        let param = PathArg { name: "id".into(), value: "12.3".into() };
        let id = f32::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 12.3);
    }

    #[test]
    fn it_reads_f32_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "12.3".into() };
        let id = f32::from_path_arg(&param).unwrap();

        assert_eq!(id, 12.3);
    }

    #[tokio::test]
    async fn it_reads_f64_from_payload() {
        let param = PathArg { name: "id".into(), value: "12.3".into() };
        let id = f64::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(id, 12.3);
    }

    #[test]
    fn it_reads_f64_from_path_arg() {
        let param = PathArg { name: "id".into(), value: "12.3".into() };
        let id = f64::from_path_arg(&param).unwrap();

        assert_eq!(id, 12.3);
    }

    #[tokio::test]
    async fn it_reads_bool_from_payload() {
        let param = PathArg { name: "flag".into(), value: "true".into() };
        let flag = bool::from_payload(Payload::Path(param)).await.unwrap();

        assert!(flag);
    }

    #[test]
    fn it_reads_bool_from_path_arg() {
        let param = PathArg { name: "flag".into(), value: "true".into() };
        let flag = bool::from_path_arg(&param).unwrap();

        assert!(flag);
    }

    #[tokio::test]
    async fn it_reads_char_from_payload() {
        let param = PathArg { name: "char".into(), value: "a".into() };
        let char = char::from_payload(Payload::Path(param)).await.unwrap();

        assert_eq!(char, 'a');
    }

    #[test]
    fn it_reads_char_from_path_arg() {
        let param = PathArg { name: "char".into(), value: "a".into() };
        let char = char::from_path_arg(&param).unwrap();

        assert_eq!(char, 'a');
    }

    #[tokio::test]
    async fn it_reads_named_path_from_payload() {
        let args = create_path_args();

        let path = NamedPath::<Params>::from_payload(Payload::PathArgs(&args)).await.unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_payload() {
        let args = create_path_args();

        let path = Path::<(u32, String)>::from_payload(Payload::PathArgs(&args)).await.unwrap().0;

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
        let args= create_path_args();
        
        let mut ext = Extensions::new();
        ext.insert(args);

        let path = NamedPath::<Params>::try_from(&ext).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[test]
    fn it_parses_path_from_request_extensions() {
        let args= create_path_args();

        let mut ext = Extensions::new();
        ext.insert(args);

        let path = Path::<(u32, String)>::try_from(&ext).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[tokio::test]
    async fn it_reads_named_path_from_parts() {
        let args= create_path_args();

        let req = Request::get("/")
            .extension(args)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = NamedPath::<Params>::from_parts(&parts).unwrap();

        assert_eq!(path.id, 123u32);
        assert_eq!(path.name, "John")
    }

    #[tokio::test]
    async fn it_reads_path_from_parts() {
        let args= create_path_args();

        let req = Request::get("/")
            .extension(args)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let path = Path::<(u32, String)>::from_parts(&parts).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }

    #[tokio::test]
    async fn it_reads_named_path_from_request_ref() {
        let args= create_path_args();

        let req = Request::get("/")
            .extension(args)
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
        let args= create_path_args();

        let req = Request::get("/")
            .extension(args)
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);
        let path = Path::<(u32, String)>::from_request(&req).unwrap().0;

        assert_eq!(path.0, 123u32);
        assert_eq!(path.1, "John")
    }
}
