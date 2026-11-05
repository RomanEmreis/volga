//! Extractors for HTTP request parts and body

use hyper::http::request::Parts;
use std::{future::Future, ops::RangeInclusive};

use crate::{HttpBody, HttpRequest, error::Error};

pub use self::head::{FromRequestHead, RequestHead};
pub use crate::http::endpoints::route::{PathArg, PathArgs};

#[cfg(feature = "di")]
use crate::di::{Container, Dependencies, FromContainer};

pub mod byte_stream;
pub mod cancellation_token;
pub mod client_ip;
pub mod file;
pub mod form;
mod head;
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
// `Request` is by far the largest, and is not boxed: that would allocate for every request a
// handler taking it receives, and a payload does not outlive the call it is handed to
#[allow(clippy::large_enum_variant)]
pub(crate) enum Payload<'a> {
    None,
    Request(HttpRequest),
    Full(&'a Parts, HttpBody),
    /// The head, with the scope of the request
    Parts(RequestHead<'a>),
    /// A positional parameter that something can read after the extractor it is handed to
    Path(&'a PathArg),
    /// A positional parameter nothing reads after the extractor it is handed to, which can
    /// take its value
    PathMut(&'a mut PathArg),
    Body(HttpBody),
}

/// Describes a data source for extractors to read from
#[derive(Debug, PartialEq)]
pub(crate) enum Source {
    None,
    Request,
    Full,
    Parts,
    /// The head, read by a type implementing [`FromRequestHead`]. It can read the path
    /// parameters as well, so a positional extractor next to it leaves them in place.
    Head,
    Path,
    Body,
    /// The head, read by [`Path<T>`](crate::Path) or [`NamedPath<T>`](crate::NamedPath)
    PathArgs,
}

/// Keeps the traits that appear in volga's signatures as bounds from being implemented
/// outside of it: what a user implements is [`FromRequestHead`]
pub(crate) mod sealed {
    use super::FromRequestHead;

    /// Implemented next to each extractor
    #[diagnostic::on_unimplemented(
        message = "`{Self}` cannot implement this trait: it is a bound on what volga takes, not an extension point",
        label = "implement `FromRequestHead` instead",
        note = "a type of your own becomes a handler and middleware argument by implementing `FromRequestHead`"
    )]
    pub trait Sealed {}

    impl Sealed for () {}
    impl<T: FromRequestHead> Sealed for T {}

    /// Whether an extractor of the head can read the path parameters, which a positional
    /// extractor next to it then leaves in place. Unnameable outside of volga, so only a
    /// built-in extractor, which reads no parameter, says [`PathAccess::Ignored`]: one of
    /// the user's own may read them through [`RequestHead::extract`](super::RequestHead::extract).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PathAccess {
        /// It may read them, so they are copied for the positional extractor
        MayRead,
        /// It reads none, so the positional extractor takes its own
        Ignored,
    }
}

/// Any number of path parameters
pub(crate) const ANY_PATH_ARITY: RangeInclusive<usize> = 0..=usize::MAX;

/// Refuses extractors that cannot read one request together, each of which would otherwise
/// be left with nothing to read once the handler is called.
///
/// Extractors are read in the order a handler takes them, so what a signature costs does not
/// depend on how its arguments are sorted. `HttpRequest` is handed the whole request, which
/// makes it a body extractor, and one that stands last.
///
/// It only runs in the `const` block of the tuple's [`FromRequest`], when a handler is built.
const fn assert_read_together(sources: &[Source]) {
    let (mut bodies, mut positional, mut whole_path) = (0, false, false);
    let mut request_is_last = true;
    let mut i = 0;

    while i < sources.len() {
        match sources[i] {
            Source::Request => {
                bodies += 1;
                request_is_last = i + 1 == sources.len();
            }
            Source::Body | Source::Full => bodies += 1,
            Source::Path => positional = true,
            Source::PathArgs => whole_path = true,
            Source::None | Source::Parts | Source::Head => {}
        }
        i += 1;
    }

    assert!(
        bodies <= 1,
        "a handler takes one extractor of the request body, `HttpRequest` included: the body is a stream that is read once"
    );
    assert!(
        request_is_last,
        "`HttpRequest` takes the whole request, so it stands last among the arguments of a handler"
    );
    assert!(
        !(positional && whole_path),
        "a handler reads path parameters either by position or through `Path<T>` / `NamedPath<T>`, not both"
    );
}

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

/// The bound on what a handler takes: the tuple of its arguments.
///
/// This is a bound, not an extension point: it is sealed, and implemented for `()` and for
/// tuples of up to 10 extractors. To make a type of your own a handler argument, implement
/// [`FromRequestHead`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be extracted from a request",
    label = "not an extractor",
    note = "a handler takes extractors: `Json<T>`, `Query<T>`, `Path<T>`, `Form<T>`, `Vec<T>` (a JSON array, as `Json<Vec<T>>` reads one), `File`, `ByteStream`, `ClientIp`, `CancellationToken`, `HttpRequest`, `Dc<T>` (feature `di`), `Multipart` (feature `multipart`)",
    note = "`Option<T>` and `Result<T, volga::error::Error>` wrap any of them, `Valid<E>` wraps one whose payload implements `Validate`, and a tuple of up to 10 is an extractor too"
)]
pub trait FromRequest: Sized + sealed::Sealed {
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

/// The bound on what reads the head of a request without consuming it: what `with`,
/// `filter`, `map_err` and `map_fallback` take, and what [`HttpRequest::extract`] and
/// [`RequestHead::extract`] read.
///
/// This is a bound, not an extension point: it is sealed. To make a type of your own an
/// extractor, implement [`FromRequestHead`], which serves here as well.
pub trait FromRequestRef: Sized + sealed::Sealed {
    /// Extracts data from the head of a request
    fn from_request(head: &RequestHead<'_>) -> Result<Self, Error>;

    /// Declares the services this extractor resolves from the DI container, for
    /// [`App::run`](crate::App::run) to check that each of them is registered
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
/// A type implementing it is the `T` of a [`Path<T>`](crate::Path) or an element of a
/// `Path<T>` tuple, so the call site names where it is read from. It is implemented for the
/// primitives, the `std::net` addresses, `String`, `Box<str>`, `Cow<'static, str>`,
/// `Box<[u8]>`, `CString`, `OsString`, `PathBuf` and, with the `uuid` feature, `uuid::Uuid`,
/// and these are handler arguments of their own as well (`|id: u64|`).
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
/// use volga::{App, HttpResult, Path, error::Error, ok};
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
/// app.map_get("/orders/{id}", |Path(id): Path<OrderId>| ok!("{}", id.0));
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be read from a path parameter",
    label = "not a path parameter",
    note = "a type of your own is read from a path parameter once it implements `FromPathArg`, and is taken as `Path<T>`",
    note = "to read the parameters into a struct by name, use `NamedPath<T>`"
)]
pub trait FromPathArg: Sized {
    /// Extracts data from a path argument
    fn from_path_arg(arg: &PathArg) -> Result<Self, Error>;

    /// Extracts data from a path argument nothing reads afterwards.
    ///
    /// A type that keeps the value, such as `String`, takes it without a copy.
    #[doc(hidden)]
    #[inline]
    fn take_path_arg(arg: &mut PathArg) -> Result<Self, Error> {
        Self::from_path_arg(arg)
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
    note = "`FromPayload` is internal to volga: a type of your own becomes an extractor by implementing `FromRequestHead`; one read from a path parameter implements `FromPathArg` and is taken as `Path<T>`; otherwise it travels inside an extractor - `Json<T>` / `Query<T>` / `Form<T>` deserialize it, `NamedPath<T>` reads the path into it by name, `Header<T>` takes a `FromHeaders` (or `#[http_header]`), `Dc<T>` whatever the container holds"
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
    fn from_request(_: &RequestHead<'_>) -> Result<Self, Error> {
        Ok(())
    }
}

macro_rules! define_generic_from_request {
    ($($T: ident),*) => {
        impl<$($T),+> sealed::Sealed for ($($T,)+) {}

        impl<$($T: FromRequestRef),+> FromRequestRef for ($($T,)+) {
            #[inline]
            fn from_request(head: &RequestHead<'_>) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::from_request(head)?,
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
            // Not an `async fn`, which keeps a parameter in a slot of the future and moves it
            // to a second one
            #[inline]
            fn from_request(mut req: HttpRequest) -> impl Future<Output = Result<Self, Error>> + Send {
                async move {
                    // A failure here is a compile error only where the handler is built:
                    // `cargo check` does not instantiate it
                    const { assert_read_together(&[$($T::SOURCE),+]) };

                    // An `HttpRequest` carries the path parameters on, for `path_args` and
                    // `extract` to read and for the handler `map_ws` and `map_msg` wrap, and an
                    // extractor of the user's own can read them through the head, so next to
                    // these two a positional extractor leaves its parameter where it is
                    let keeps_params = false
                        $(|| matches!($T::SOURCE, Source::Request | Source::Head))*;
                    let mut position = 0;

                    let tuple = (
                        $(
                            {
                                let payload = next_payload($T::SOURCE, &mut req, &mut position, keeps_params);
                                $T::from_payload(payload).await?
                            },
                        )*
                    );
                    Ok(tuple)
                }
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

/// Hands an extractor what it reads from `req`.
///
/// `req` stays whole until `HttpRequest` takes it, which [`assert_read_together`] keeps last.
#[inline]
fn next_payload<'a>(
    source: Source,
    req: &'a mut HttpRequest,
    position: &mut usize,
    keeps_params: bool,
) -> Payload<'a> {
    match source {
        Source::Parts | Source::Head | Source::PathArgs => {
            Payload::Parts(RequestHead::from_request(req))
        }
        Source::Path => {
            let arg = req.path_arg_mut(*position);
            *position += 1;
            match arg {
                Some(arg) if keeps_params => Payload::Path(arg),
                Some(arg) => Payload::PathMut(arg),
                None => Payload::None,
            }
        }
        Source::Body => Payload::Body(req.take_body()),
        Source::Full => {
            let body = req.take_body();
            Payload::Full(req.parts(), body)
        }
        Source::Request => Payload::Request(req.take()),
        Source::None => Payload::None,
    }
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

/// A request whose scope carries `params`
#[cfg(test)]
pub(crate) fn request_with(params: PathArgs) -> HttpRequest {
    use crate::http::request_scope::HttpRequestScope;

    let scope = HttpRequestScope {
        params,
        ..HttpRequestScope::default()
    };
    let (parts, body) = hyper::Request::get("/")
        .body(crate::HttpBody::empty())
        .unwrap()
        .into_parts();
    HttpRequest::from_parts(parts, body).with_scope(scope)
}

#[cfg(test)]
mod tests {
    use super::{ANY_PATH_ARITY, FromPayload, FromRequest, Payload, Source, assert_read_together};
    use crate::error::Error;
    use crate::http::FromRequestRef;
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
    fn it_reads_extractors_that_share_a_request_together() {
        assert_read_together(&[]);
        assert_read_together(&[Source::Request]);
        assert_read_together(&[Source::Body]);
        assert_read_together(&[Source::Parts, Source::Head, Source::Body, Source::Path]);
        assert_read_together(&[Source::Path, Source::Path, Source::Full, Source::None]);
        assert_read_together(&[Source::PathArgs, Source::PathArgs, Source::Head]);
    }

    #[test]
    fn it_reads_a_request_after_everything_else() {
        assert_read_together(&[Source::Parts, Source::Request]);
        assert_read_together(&[Source::Head, Source::Parts, Source::Request]);
        assert_read_together(&[Source::Path, Source::Path, Source::Request]);
        assert_read_together(&[Source::PathArgs, Source::Request]);
    }

    #[test]
    #[should_panic(expected = "so it stands last among the arguments of a handler")]
    fn it_refuses_a_built_in_extractor_of_the_head_after_a_request() {
        assert_read_together(&[Source::Request, Source::Parts]);
    }

    #[test]
    #[should_panic(expected = "so it stands last among the arguments of a handler")]
    fn it_refuses_a_custom_extractor_after_a_request() {
        assert_read_together(&[Source::Request, Source::Head]);
    }

    #[test]
    #[should_panic(expected = "so it stands last among the arguments of a handler")]
    fn it_refuses_a_positional_param_after_a_request() {
        assert_read_together(&[Source::Request, Source::Path]);
    }

    #[test]
    #[should_panic(expected = "so it stands last among the arguments of a handler")]
    fn it_refuses_a_path_after_a_request() {
        assert_read_together(&[Source::Request, Source::PathArgs]);
    }

    #[test]
    #[should_panic(expected = "a handler takes one extractor of the request body")]
    fn it_refuses_two_extractors_of_the_body() {
        assert_read_together(&[Source::Body, Source::Full]);
    }

    #[test]
    #[should_panic(expected = "a handler takes one extractor of the request body")]
    fn it_refuses_a_request_next_to_a_body_extractor() {
        assert_read_together(&[Source::Body, Source::Parts, Source::Request]);
    }

    #[test]
    #[should_panic(expected = "a handler takes one extractor of the request body")]
    fn it_refuses_two_requests() {
        assert_read_together(&[Source::Request, Source::Request]);
    }

    #[test]
    #[should_panic(expected = "either by position or through")]
    fn it_refuses_positional_params_next_to_a_path() {
        assert_read_together(&[Source::Path, Source::PathArgs]);
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
        assert_eq!(<(Uri,) as FromRequestRef>::PATH_ARITY, ANY_PATH_ARITY);
        assert_eq!(<(Path<u32>, Uri) as FromRequestRef>::PATH_ARITY, 1..=1);
        assert_eq!(
            <(Uri, Path<(u32, u32)>) as FromRequestRef>::PATH_ARITY,
            2..=usize::MAX
        );
        assert_eq!(
            <(Option<Path<u32>>,) as FromRequestRef>::PATH_ARITY,
            ANY_PATH_ARITY
        );
        assert_eq!(
            <(Result<Path<u32>, Error>,) as FromRequestRef>::PATH_ARITY,
            ANY_PATH_ARITY
        );
    }
}
