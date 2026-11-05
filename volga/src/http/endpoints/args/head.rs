//! An extractor of your own, read from the head of a request

use futures_util::future::{Ready, ready};
use hyper::http::request::Parts;
use std::fmt;

use crate::{
    HttpRequest,
    error::Error,
    headers::HeaderMap,
    http::{
        Method, Uri,
        endpoints::args::{FromPayload, FromRequestRef, Payload, Source, sealed::PathAccess},
        request_scope::HttpRequestScope,
    },
};

#[cfg(feature = "di")]
use crate::di::Dependencies;

/// The head of a request - everything but the body - as a [`FromRequestHead`] reads it.
#[derive(Clone, Copy)]
pub struct RequestHead<'a> {
    parts: &'a Parts,
    /// The scope of the request, a field of [`HttpRequest`] that the head lends with it
    scope: Option<&'a HttpRequestScope>,
}

impl fmt::Debug for RequestHead<'_> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestHead(..)")
    }
}

impl<'a> RequestHead<'a> {
    /// The head of a request, with its scope
    #[inline]
    pub(crate) fn from_request(req: &'a HttpRequest) -> Self {
        Self {
            parts: req.parts(),
            scope: req.scope(),
        }
    }

    /// The head of a request that has no scope, as one that was not routed has none
    #[inline]
    #[cfg(any(test, feature = "__fuzzing"))]
    pub(crate) fn from_parts(parts: &'a Parts) -> Self {
        Self { parts, scope: None }
    }

    /// The head of a request with the scope given
    #[inline]
    #[cfg(test)]
    pub(crate) fn with_scope(parts: &'a Parts, scope: &'a HttpRequestScope) -> Self {
        Self {
            parts,
            scope: Some(scope),
        }
    }

    /// Returns a reference to the HTTP headers of the request.
    #[inline]
    pub fn headers(&self) -> &'a HeaderMap {
        &self.parts.headers
    }

    /// The scope of the request, for the built-in extractors, `None` when it was not routed
    #[inline]
    pub(crate) fn scope(&self) -> Option<&'a HttpRequestScope> {
        self.scope
    }

    /// The whole head, for the WebSocket handshake, which reads its version and extensions
    #[inline]
    #[cfg(feature = "ws")]
    pub(crate) fn parts(&self) -> &'a Parts {
        self.parts
    }

    /// The URI of the request, for the built-in extractors
    #[inline]
    pub(crate) fn uri(&self) -> &'a Uri {
        &self.parts.uri
    }

    /// The method of the request, for the built-in extractors
    #[inline]
    pub(crate) fn method(&self) -> &'a Method {
        &self.parts.method
    }

    /// The extensions of the request, for the built-in extractors of what is still kept there:
    /// the container, `HostEnv` and the claims of `Authenticated<T>`. They stay crate-private,
    /// see #272
    #[inline]
    #[cfg(any(
        feature = "di",
        feature = "jwt-auth",
        feature = "static-files",
        feature = "signed-cookie",
        feature = "private-cookie"
    ))]
    pub(crate) fn extensions(&self) -> &'a crate::http::Extensions {
        &self.parts.extensions
    }

    /// Reads any extractor of the head: a built-in one or another [`FromRequestHead`].
    ///
    /// # Example
    /// ```no_run
    /// use serde::Deserialize;
    /// use volga::{Query, error::Error, http::{FromRequestHead, RequestHead}};
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     lang: Option<String>,
    /// }
    ///
    /// struct Locale(String);
    ///
    /// impl FromRequestHead for Locale {
    ///     fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
    ///         let params: Query<Params> = head.extract()?;
    ///         let lang = params.into_inner().lang;
    ///         Ok(Locale(lang.unwrap_or_else(|| "en".to_owned())))
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        T::from_request(self)
    }
}

/// Makes a type of your own an extractor, read from the head of a request.
///
/// A type implementing it is taken wherever a built-in extractor of the head is: as a handler
/// argument, in `with`, `filter` and `map_err`, as `Option<T>` or `Result<T, Error>`, and
/// through [`RequestHead::extract`]. It leaves the body alone, so a handler can take a body
/// extractor next to it. Next to a path extractor it can still read the path parameters.
///
/// Reading the head is synchronous. Asynchronous work, such as loading a session row, goes to
/// a middleware, which hands its result over through a scoped service: with the `di` feature,
/// one instance per request that the middleware fills through `Dc<T>` and this trait reads
/// the same way.
///
/// # Example
/// ```no_run
/// use volga::{App, HttpResult, Json, error::Error, ok};
/// use volga::http::{FromRequestHead, RequestHead, StatusCode};
///
/// struct CurrentUser(String);
///
/// impl FromRequestHead for CurrentUser {
///     fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
///         let id = head.headers().get("x-user-id").and_then(|id| id.to_str().ok());
///         id.map(|id| CurrentUser(id.to_owned()))
///             .ok_or_else(|| (StatusCode::UNAUTHORIZED, "who are you?").into())
///     }
/// }
///
/// let mut app = App::new();
///
/// app.map_post("/notes", async |me: CurrentUser, note: Json<String>| -> HttpResult {
///     ok!("{} wrote {}", me.0, note.0)
/// });
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an extractor this handler or middleware can take",
    label = "not an extractor",
    note = "a handler takes extractors: `Json<T>`, `Query<T>`, `Path<T>`, `Form<T>`, `Vec<T>` (a JSON array, as `Json<Vec<T>>` reads one), `File`, `ByteStream`, `ClientIp`, `CancellationToken`, `HttpRequest`, `Dc<T>` (feature `di`), `Multipart` (feature `multipart`)",
    note = "`Option<T>` and `Result<T, volga::error::Error>` wrap any of them, and `Valid<E>` wraps one whose payload implements `Validate`",
    note = "`with`, `filter` and `map_err` read the request without consuming it, so they take the extractors of its head, but not `Json<T>` or any other body extractor",
    note = "a type of your own becomes an extractor by implementing `FromRequestHead`; one read from a path parameter implements `FromPathArg` and is taken as `Path<T>`; otherwise it travels inside an extractor - `Json<T>` / `Query<T>` / `Form<T>` deserialize it, `NamedPath<T>` reads the path into it by name, `Header<T>` takes a `FromHeaders` (or `#[http_header]`), `Dc<T>` whatever the container holds"
)]
pub trait FromRequestHead: Sized + Send {
    /// Reads the extractor from the head of a request
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error>;

    /// Declares the services this extractor resolves from the DI container.
    ///
    /// [`App::run`](crate::App::run) checks that each of them is registered before it
    /// starts, so that whatever takes this extractor does not answer `500` on every request
    /// instead. The default declares nothing. A `Dc<T>` read through
    /// [`RequestHead::extract`] is declared here, since nothing else can see it.
    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(_deps: &mut Dependencies) {}

    /// Whether this extractor can read the path parameters. Its type cannot be named outside
    /// of volga, so a type of your own keeps the default
    #[doc(hidden)]
    const PATH_ACCESS: PathAccess = PathAccess::MayRead;
}

impl<T: FromRequestHead> FromPayload for T {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = match T::PATH_ACCESS {
        PathAccess::MayRead => Source::Head,
        PathAccess::Ignored => Source::Parts,
    };

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(head) = payload else {
            unreachable!()
        };
        ready(T::from_head(&head))
    }

    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(deps: &mut Dependencies) {
        <T as FromRequestHead>::dependencies(deps);
    }
}

impl<T: FromRequestHead> FromRequestRef for T {
    #[inline]
    fn from_request(head: &RequestHead<'_>) -> Result<Self, Error> {
        T::from_head(head)
    }

    #[inline]
    #[cfg(feature = "di")]
    fn dependencies(deps: &mut Dependencies) {
        <T as FromRequestHead>::dependencies(deps);
    }
}
