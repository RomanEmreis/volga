//! Extractors for HTTP request parts and body

use hyper::http::request::Parts;
use std::{future::Future, ops::RangeInclusive};

use crate::{HttpBody, HttpRequest, error::Error, http::request_scope::HttpRequestScope};

pub use crate::http::endpoints::route::{PathArg, PathArgs};

#[cfg(feature = "di")]
use crate::di::{Container, Dependencies, FromContainer};

pub mod byte_stream;
pub mod cancellation_token;
pub mod client_ip;
pub mod file;
pub mod form;
#[cfg(feature = "static-files")]
pub mod host_env;
pub mod json;
#[cfg(feature = "multipart")]
pub mod multipart;
pub mod option;
pub mod path;
pub mod query;
pub mod request;
pub mod result;
pub mod shutdown_handle;
pub mod sse;
pub mod vec;

/// Holds the payload for extractors
pub(crate) enum Payload<'a> {
    None,
    Request(Box<HttpRequest>),
    Full(&'a Parts, HttpBody),
    Parts(&'a Parts),
    Path(PathArg),
    Body(HttpBody),
    PathArgs(&'a PathArgs),
}

/// Describes a data source for extractors to read from
#[derive(Debug, PartialEq)]
pub(crate) enum Source {
    None,
    Request,
    Full,
    Parts,
    Path,
    Body,
    PathArgs,
}

/// Any number of path parameters
pub(crate) const ANY_PATH_ARITY: RangeInclusive<usize> = 0..=usize::MAX;

/// The numbers of path parameters a route can declare for `extractors`, in the order a
/// handler takes them, to be read from it.
///
/// Each one narrows what the ones before it allow. A positional extractor is handed the
/// parameter after those read ahead of it, so the second of `|x: i32, y: i32|` needs a route
/// declaring at least 2. One read as `None` where there are too few needs none, though it
/// still takes its turn: the `y` of `|x: Option<i32>, y: i32|` is the second as well.
pub(crate) const fn path_arity(
    extractors: &[(Source, RangeInclusive<usize>)],
) -> RangeInclusive<usize> {
    let (mut min, mut max) = (0, usize::MAX);
    let mut position = 0;
    let mut i = 0;

    while i < extractors.len() {
        let (source, arity) = &extractors[i];
        let read_ahead = if matches!(source, Source::Path) {
            position += 1;
            position - 1
        } else {
            0
        };

        let (start, end) = (*arity.start(), *arity.end());
        if start > 0 && start.saturating_add(read_ahead) > min {
            min = start.saturating_add(read_ahead);
        }
        if end.saturating_add(read_ahead) < max {
            max = end.saturating_add(read_ahead);
        }

        i += 1;
    }

    min..=max
}

/// Specifies extractors to read data from HTTP request
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from a request",
    label = "not an extractor",
    note = "a handler takes extractors: `Json<T>`, `Query<T>`, `Path<T>`, `Form<T>`, `Vec<T>` (a JSON array, as `Json<Vec<T>>` reads one), `File`, `ByteStream`, `ClientIp`, `CancellationToken`, `HttpRequest`, `Dc<T>` (feature `di`), `Multipart` (feature `multipart`)",
    note = "`Option<T>` and `Result<T, volga::error::Error>` wrap any of them, `Valid<E>` wraps one whose payload implements `Validate`, and a tuple of up to 10 is an extractor too"
)]
pub trait FromRequest: Sized {
    /// Extracts data from HTTP request
    fn from_request(req: HttpRequest) -> impl Future<Output = Result<Self, Error>> + Send;

    /// Describes Open API schema
    #[cfg(feature = "openapi")]
    #[doc(hidden)]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config
    }

    /// Declares the services these extractors resolve from the DI container, for
    /// [`App::run`](crate::App::run) to check that each of them is registered
    #[inline]
    #[doc(hidden)]
    #[cfg(feature = "di")]
    fn dependencies(_deps: &mut Dependencies) {}

    /// The numbers of path parameters a route can declare for these extractors to be read
    /// from it, which the route a handler taking them is mapped on is checked against
    #[doc(hidden)]
    const PATH_ARITY: RangeInclusive<usize> = ANY_PATH_ARITY;
}

/// Specifies extractors to read data from a borrowed HTTP request
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from a borrowed request",
    label = "not a borrowing extractor",
    note = "`with`, `filter` and `map_err` read the request without consuming it, so they take the extractors that borrow: `Query<T>`, `Path<T>`, `ClientIp`, `Dc<T>` (feature `di`) and the header types, but not `Json<T>` or any other body extractor"
)]
pub trait FromRequestRef: Sized {
    /// Extracts data from HTTP request reference
    fn from_request(req: &HttpRequest) -> Result<Self, Error>;

    /// Declares the services this extractor resolves from the DI container.
    ///
    /// [`App::run`](crate::App::run) checks that each of them is registered before it
    /// starts, so that whatever takes this extractor does not answer `500` on every request
    /// instead. The default declares nothing. An extractor of your own that resolves a
    /// service from the container declares it here.
    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(_deps: &mut Dependencies) {}
}

/// Specifies extractors to read data from HTTP request parts
pub trait FromRequestParts: Sized {
    /// Extracts data from HTTP request parts
    fn from_parts(parts: &Parts) -> Result<Self, Error>;

    /// Declares the services this extractor resolves from the DI container.
    ///
    /// [`App::run`](crate::App::run) checks that each of them is registered before it
    /// starts, so that whatever takes this extractor does not answer `500` on every request
    /// instead. The default declares nothing. An extractor of your own that resolves a
    /// service from the container declares it here.
    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(_deps: &mut Dependencies) {}

    /// The numbers of path parameters a fallback can be handed for this extractor to be
    /// read from them, which the prefix a fallback taking it is mapped under is checked
    /// against
    #[doc(hidden)]
    const PATH_ARITY: RangeInclusive<usize> = ANY_PATH_ARITY;
}

/// Specifies extractor to read data from path arguments
///
/// This is what [`Path<T>`](crate::Path) reads its `T` through. It is implemented for a tuple
/// of up to 10 [`FromPathArg`] types, read in the order the route declares its parameters
/// and from a route declaring at least as many, and for a single [`FromPathArg`] type, which
/// reads a route declaring exactly one parameter. A handler taking `Path<T>` on a route
/// declaring any other number panics where it is mapped - see [`ARITY`](Self::ARITY).
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be read from the path arguments",
    label = "not a path",
    note = "`Path<T>` takes a type implementing `FromPathArg` or a tuple of them: `Path<u32>`, `Path<(String, u32)>`",
    note = "to read the parameters into a struct by name, use `NamedPath<T>`"
)]
pub trait FromPathArgs: Sized {
    /// The numbers of path parameters this type can be read from.
    ///
    /// A handler taking [`Path<Self>`](crate::Path) is mapped only on a route declaring one
    /// of these numbers: on any other, mapping it panics, rather than leaving a route that
    /// fails every request it receives. A tuple of `N` reads the first `N` of the parameters,
    /// so it takes `N..=usize::MAX`, and a single [`FromPathArg`] type takes `1..=1`. The
    /// default takes any number, which checks nothing.
    ///
    /// # Example
    /// ```
    /// use std::ops::RangeInclusive;
    /// use volga::{App, Path, error::Error, ok};
    /// use volga::http::endpoints::args::{FromPathArgs, PathArgs};
    ///
    /// struct Span {
    ///     from: u32,
    ///     to: u32,
    /// }
    ///
    /// impl FromPathArgs for Span {
    ///     const ARITY: RangeInclusive<usize> = 2..=2;
    ///
    ///     fn from_path_args(args: &PathArgs) -> Result<Self, Error> {
    ///         let mut args = args.iter();
    ///         match (args.next(), args.next()) {
    ///             (Some(from), Some(to)) => Ok(Span { from: from.parse()?, to: to.parse()? }),
    ///             _ => Err(Error::server_error("a span reads two path parameters")),
    ///         }
    ///     }
    /// }
    ///
    /// let mut app = App::new();
    /// app.map_get("/days/{from}/{to}", |Path(span): Path<Span>| ok!("{}", span.to - span.from));
    /// ```
    const ARITY: RangeInclusive<usize> = 0..=usize::MAX;

    /// Extracts data from path arguments
    fn from_path_args(args: &PathArgs) -> Result<Self, Error>;
}

/// Specifies how a type is read from a single path argument
///
/// A type implementing it can be a handler argument of its own (`|id: OrderId|`), an
/// element of a [`Path<T>`](crate::Path) tuple, or the `T` of a `Path<T>`. It is
/// implemented for the primitives, the `std::net` addresses, `String`, `Box<str>`,
/// `Cow<'static, str>`, `Box<[u8]>`, `CString`, `OsString`, `PathBuf` and, with the `uuid`
/// feature, `uuid::Uuid`.
///
/// The value is percent-decoded by the router before any extractor reads it, and a path
/// that does not decode is answered `400` - see [`PathArg::value`].
/// [`PathArg::parse`] reads it through [`FromStr`](std::str::FromStr) and answers `400` if
/// it does not parse.
///
/// # Security
/// The value is decoded already: `%2E%2E` arrives as `..` and `%2F` as `/`, so a single
/// parameter can carry a separator, and a catch-all parameter (`{*rest}`) carries the
/// separators between its segments as well. A check made on the value, such as refusing
/// `..` or `/` in a file name, sees what the handler will use - do not decode it a second
/// time after checking it.
///
/// # Example
/// ```no_run
/// use volga::{App, HttpResult, error::Error, ok};
/// use volga::http::endpoints::args::{FromPathArg, PathArg};
///
/// struct OrderId(u64);
///
/// impl FromPathArg for OrderId {
///     fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
///         arg.parse().map(OrderId)
///     }
/// }
///
/// let mut app = App::new();
/// app.map_get("/orders/{id}", |id: OrderId| ok!("{}", id.0));
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an extractor and cannot be a handler argument",
    label = "not an extractor",
    note = "a handler takes extractors: `Json<T>`, `Query<T>`, `Path<T>`, `Form<T>`, `Vec<T>` (a JSON array, as `Json<Vec<T>>` reads one), `File`, `ByteStream`, `ClientIp`, `CancellationToken`, `HttpRequest`, `Dc<T>` (feature `di`), `Multipart` (feature `multipart`)",
    note = "`Option<T>` and `Result<T, volga::error::Error>` wrap any of them, and `Valid<E>` wraps one whose payload implements `Validate`",
    note = "a type of your own is read from a path parameter once it implements `FromPathArg`; otherwise it travels inside an extractor - `Json<T>` / `Query<T>` / `Form<T>` deserialize it, `NamedPath<T>` reads the path into it by name, `Header<T>` takes a `FromHeaders` (or `#[http_header]`), `Dc<T>` whatever the container holds"
)]
pub trait FromPathArg: Sized {
    /// Extracts data from a path argument
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error>;

    /// Extracts data from a path argument the caller owns.
    ///
    /// A handler argument owns its path argument, so a type that keeps the value, such as
    /// `String`, takes it without a copy.
    #[doc(hidden)]
    #[inline]
    fn from_owned_path_arg(arg: PathArg) -> Result<Self, Error> {
        Self::from_path_arg(&arg)
    }
}

/// Specifies extractor to read data from an HTTP request
/// depending on payload's [`Source`]
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an extractor and cannot be a handler argument",
    label = "not an extractor",
    note = "a handler takes extractors: `Json<T>`, `Query<T>`, `Path<T>`, `Form<T>`, `Vec<T>` (a JSON array, as `Json<Vec<T>>` reads one), `File`, `ByteStream`, `ClientIp`, `CancellationToken`, `HttpRequest`, `Dc<T>` (feature `di`), `Multipart` (feature `multipart`)",
    note = "`Option<T>` and `Result<T, volga::error::Error>` wrap any of them, and `Valid<E>` wraps one whose payload implements `Validate`",
    note = "`Path<T>` reads by position: `T` is one type implementing `FromPathArg`, or a tuple of them; a struct read field by field goes into `NamedPath<T>`",
    note = "`FromPayload` is internal to volga: a type of your own is read from a path parameter once it implements `FromPathArg`; otherwise it travels inside an extractor - `Json<T>` / `Query<T>` / `Form<T>` deserialize it, `NamedPath<T>` reads the path into it by name, `Header<T>` takes a `FromHeaders` (or `#[http_header]`), `Dc<T>` whatever the container holds"
)]
pub(crate) trait FromPayload: Send + Sized {
    type Future: Future<Output = Result<Self, Error>> + Send;

    /// Represents a [`Source`] where the payload should be extracted from
    const SOURCE: Source = Source::None;

    /// The numbers of path parameters this extractor can be read from.
    ///
    /// A positional one ([`Source::Path`]) counts the parameters left once the positional
    /// extractors ahead of it have read theirs, and any other one counts all the route
    /// declares. An extractor that reads none, or that is read as `None` or an `Err` where
    /// there are too few, takes any number.
    const PATH_ARITY: RangeInclusive<usize> = ANY_PATH_ARITY;

    /// Extracts data from give [`Payload`]
    fn from_payload(payload: Payload<'_>) -> Self::Future;

    /// Describes Open API schema
    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config
    }

    /// Declares the services this extractor resolves from the DI container
    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(_deps: &mut Dependencies) {}
}

impl FromRequest for () {
    #[inline]
    async fn from_request(_: HttpRequest) -> Result<Self, Error> {
        Ok(())
    }
}

impl FromRequestRef for () {
    #[inline]
    fn from_request(_: &HttpRequest) -> Result<Self, Error> {
        Ok(())
    }
}

impl FromRequestParts for () {
    #[inline]
    fn from_parts(_: &Parts) -> Result<Self, Error> {
        Ok(())
    }
}

macro_rules! define_generic_from_request {
    ($($T: ident),*) => {
        impl<$($T: FromRequestRef),+> FromRequestRef for ($($T,)+) {
            #[inline]
            fn from_request(req: &HttpRequest) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_request(req)?,
                    )*
                );
                Ok(tuple)
            }

            #[inline]
            #[cfg(feature = "di")]
            fn dependencies(deps: &mut Dependencies) {
                $( $T::dependencies(deps); )*
            }
        }
        impl<$($T: FromRequestParts),+> FromRequestParts for ($($T,)+) {
            #[inline]
            fn from_parts(parts: &Parts) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_parts(parts)?,
                    )*
                );
                Ok(tuple)
            }

            #[inline]
            #[cfg(feature = "di")]
            fn dependencies(deps: &mut Dependencies) {
                $( $T::dependencies(deps); )*
            }

            // Each of them is handed all the parameters, none by position
            const PATH_ARITY: RangeInclusive<usize> =
                path_arity(&[$((Source::Parts, $T::PATH_ARITY)),+]);
        }
        #[cfg(feature = "di")]
        impl<$($T: FromContainer),+> FromContainer for ($($T,)+) {
            #[inline]
            fn from_container(container: &Container) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_container(container)?,
                    )*
                );
                Ok(tuple)
            }

            #[inline]
            fn dependencies(deps: &mut Dependencies) {
                $( $T::dependencies(deps); )*
            }
        }
        impl<$($T: FromPayload),+> FromRequest for ($($T,)+) {
            #[inline]
            async fn from_request(req: HttpRequest) -> Result<Self, Error> {
                let uses_path = false $(|| matches!($T::SOURCE, Source::Path))*;
                let uses_pathargs = false $(|| matches!($T::SOURCE, Source::PathArgs))*;

                if uses_path && uses_pathargs {
                    return Err(invalid_extractor_combination());
                }

                let (mut parts, body) = req.into_parts();

                // Taken out of the request only for an extractor that reads them: an
                // `HttpRequest` taken instead carries them on, for `path_args` and `extract`
                // to read, and for the handler `map_ws` and `map_msg` wrap
                let params = if uses_path || uses_pathargs {
                    parts
                        .extensions
                        .get_mut::<HttpRequestScope>()
                        .map(|s| std::mem::replace(&mut s.params, PathArgs::new()))
                        .unwrap_or_else(PathArgs::new)
                } else {
                    PathArgs::new()
                };

                let (path_args, cached_query) = params.into_parts();

                let mut parts = Some(parts);
                let mut body = Some(body);

                if uses_pathargs {
                    let params = PathArgs::from_parts(path_args, cached_query);
                    let tuple = (
                        $(
                            {
                                let payload = payload_for_path_args!($T::SOURCE, parts, body, &params);
                                $T::from_payload(payload).await?
                            },
                        )*
                    );
                    return Ok(tuple);
                }

                let mut iter = path_args.into_iter();
                let tuple = (
                    $(
                        {
                            let payload = payload_for_path!($T::SOURCE, parts, body, iter);
                            $T::from_payload(payload).await?
                        },
                    )*
                );
                Ok(tuple)
            }

            #[cfg(feature = "openapi")]
            fn describe_openapi(config: crate::openapi::OpenApiRouteConfig) -> crate::openapi::OpenApiRouteConfig {
                let mut config = config;
                $(
                    config = $T::describe_openapi(config);
                )*
                config
            }

            #[inline]
            #[cfg(feature = "di")]
            fn dependencies(deps: &mut Dependencies) {
                $( $T::dependencies(deps); )*
            }

            const PATH_ARITY: RangeInclusive<usize> =
                path_arity(&[$(($T::SOURCE, $T::PATH_ARITY)),+]);
        }
    }
}

#[cold]
#[inline(never)]
fn invalid_extractor_combination() -> Error {
    Error::client_error(
        "Invalid extractor combination: Cannot mix Path and PathArgs in the same handler",
    )
}

macro_rules! payload_for_path {
    ($src:expr, $parts:expr, $body:expr, $iter:expr) => {{
        match $src {
            Source::Parts => match $parts.as_ref() {
                Some(p) => Payload::Parts(p),
                None => Payload::None,
            },
            Source::Path => match $iter.next() {
                Some(arg) => Payload::Path(arg),
                None => Payload::None,
            },
            Source::Body => match $body.take() {
                Some(b) => Payload::Body(b),
                None => Payload::None,
            },
            Source::Full => match ($parts.as_ref(), $body.take()) {
                (Some(p), Some(b)) => Payload::Full(p, b),
                _ => Payload::None,
            },
            Source::Request => match ($parts.take(), $body.take()) {
                (Some(p), Some(b)) => Payload::Request(Box::new(HttpRequest::from_parts(p, b))),
                _ => Payload::None,
            },
            Source::PathArgs => Payload::None,
            Source::None => Payload::None,
        }
    }};
}

macro_rules! payload_for_path_args {
    ($src:expr, $parts:expr, $body:expr, $params:expr) => {{
        match $src {
            Source::Parts => match $parts.as_ref() {
                Some(p) => Payload::Parts(p),
                None => Payload::None,
            },
            Source::PathArgs => Payload::PathArgs($params),
            Source::Body => match $body.take() {
                Some(b) => Payload::Body(b),
                None => Payload::None,
            },
            Source::Full => match ($parts.as_ref(), $body.take()) {
                (Some(p), Some(b)) => Payload::Full(p, b),
                _ => Payload::None,
            },
            Source::Request => match ($parts.take(), $body.take()) {
                (Some(p), Some(b)) => Payload::Request(Box::new(HttpRequest::from_parts(p, b))),
                _ => Payload::None,
            },
            Source::Path => Payload::None,
            Source::None => Payload::None,
        }
    }};
}

define_generic_from_request! { T1 }
define_generic_from_request! { T1, T2 }
define_generic_from_request! { T1, T2, T3 }
define_generic_from_request! { T1, T2, T3, T4 }
define_generic_from_request! { T1, T2, T3, T4, T5 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8, T9 }
define_generic_from_request! { T1, T2, T3, T4, T5, T6, T7, T8, T9, T10 }

#[cfg(test)]
mod tests {
    use super::{ANY_PATH_ARITY, FromPayload, FromRequest, FromRequestParts, Payload, Source};
    use crate::error::Error;
    use crate::http::Uri;
    use crate::{Json, NamedPath, Path, Query};
    use futures_util::future::{Ready, ok};
    use std::collections::HashMap;

    type Map = HashMap<String, String>;

    struct TestNone;

    impl FromPayload for TestNone {
        type Future = Ready<Result<TestNone, Error>>;

        fn from_payload(_: Payload<'_>) -> Self::Future {
            ok(TestNone)
        }
    }

    #[tokio::test]
    async fn it_reads_none_from_payload() {
        let extraction = TestNone::from_payload(Payload::None).await;
        assert!(extraction.is_ok());
        assert_eq!(TestNone::SOURCE, Source::None);
    }

    #[test]
    fn it_takes_any_number_of_path_params_for_a_handler_reading_none() {
        assert_eq!(<() as FromRequest>::PATH_ARITY, ANY_PATH_ARITY);
        assert_eq!(
            <(Query<Map>, Json<Map>) as FromRequest>::PATH_ARITY,
            ANY_PATH_ARITY
        );
        assert_eq!(
            <(NamedPath<Map>,) as FromRequest>::PATH_ARITY,
            ANY_PATH_ARITY
        );
    }

    #[test]
    fn it_counts_the_positional_path_params_a_handler_reads() {
        assert_eq!(<(u32,) as FromRequest>::PATH_ARITY, 1..=usize::MAX);
        assert_eq!(
            <(Query<Map>, u32, Json<Map>, String) as FromRequest>::PATH_ARITY,
            2..=usize::MAX
        );
    }

    #[test]
    fn it_counts_an_optional_path_param_by_its_position_alone() {
        assert_eq!(<(Option<u32>,) as FromRequest>::PATH_ARITY, ANY_PATH_ARITY);
        assert_eq!(
            <(Result<u32, Error>,) as FromRequest>::PATH_ARITY,
            ANY_PATH_ARITY
        );
        assert_eq!(
            <(u32, Option<u32>) as FromRequest>::PATH_ARITY,
            1..=usize::MAX
        );
        assert_eq!(
            <(Option<u32>, u32) as FromRequest>::PATH_ARITY,
            2..=usize::MAX
        );
    }

    #[test]
    fn it_reads_the_arity_of_a_path() {
        assert_eq!(<(Path<u32>,) as FromRequest>::PATH_ARITY, 1..=1);
        assert_eq!(
            <(Path<(u32, String)>,) as FromRequest>::PATH_ARITY,
            2..=usize::MAX
        );
        assert_eq!(
            <(Option<Path<u32>>,) as FromRequest>::PATH_ARITY,
            ANY_PATH_ARITY
        );
        assert_eq!(
            <(Path<u32>, Path<(u32,)>) as FromRequest>::PATH_ARITY,
            1..=1
        );
    }

    #[test]
    fn it_finds_no_arity_for_paths_that_disagree() {
        assert!(<(Path<u32>, Path<(u32, u32)>) as FromRequest>::PATH_ARITY.is_empty());
    }

    #[test]
    fn it_reads_the_arity_of_a_path_a_fallback_takes() {
        assert_eq!(<(Uri,) as FromRequestParts>::PATH_ARITY, ANY_PATH_ARITY);
        assert_eq!(<(Path<u32>, Uri) as FromRequestParts>::PATH_ARITY, 1..=1);
        assert_eq!(
            <(Uri, Path<(u32, u32)>) as FromRequestParts>::PATH_ARITY,
            2..=usize::MAX
        );
        assert_eq!(
            <(Option<Path<u32>>,) as FromRequestParts>::PATH_ARITY,
            ANY_PATH_ARITY
        );
        assert_eq!(
            <(Result<Path<u32>, Error>,) as FromRequestParts>::PATH_ARITY,
            ANY_PATH_ARITY
        );
    }
}
