//! Request Body Limit

use crate::{
    Limit,
    routing::{Route, RouteGroup},
};

const DEFAULT_BODY_SIZE: usize = 5 * 1024 * 1024; // 5 MB

/// Represents whether a request body has a configured limit of not
///
/// Default: 5 MB
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) enum RequestBodyLimit {
    /// Body limit completely disabled
    Disabled,
    /// Configured body limit with a specific size
    Enabled(usize),
}

impl Default for RequestBodyLimit {
    #[inline]
    fn default() -> Self {
        Self::Enabled(DEFAULT_BODY_SIZE)
    }
}

impl From<Limit<usize>> for RequestBodyLimit {
    #[inline]
    fn from(limit: Limit<usize>) -> Self {
        match limit {
            Limit::Limited(limit) => Self::Enabled(limit),
            Limit::Unlimited => Self::Disabled,
            Limit::Default => Self::default(),
        }
    }
}

impl<'a> Route<'a> {
    /// Sets the request body limit (in bytes) for this route
    ///
    /// It replaces the limit of the route group the route belongs to, if any, and the
    /// application's, set with [`App::with_body_limit`](crate::App::with_body_limit) - and
    /// it can raise one as well as lower it. A request whose body goes over the limit is
    /// answered `413 Content Too Large` as the handler reads it, and one whose
    /// `Content-Length` already says it won't fit is answered before any of it is read.
    ///
    /// # Parameters
    /// - `Limit::Default` - the framework default (5 MB), not the group's or the application's
    /// - `Limit::Limited(n)` - an explicit limit
    /// - `Limit::Unlimited` - no limit at all, as [`Route::without_body_limit`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, Limit, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new().with_body_limit(Limit::Limited(64 * 1024));
    ///
    /// // The one route that takes a file
    /// app.map_post("/attachments", |req: HttpRequest| async move {
    ///     // stream `req.into_body()` somewhere
    ///     ok!()
    /// })
    /// .with_body_limit(Limit::Limited(20 * 1024 * 1024));
    ///# app.run().await
    ///# }
    /// ```
    pub fn with_body_limit(self, limit: Limit<usize>) -> Self {
        self.body_limit_override(limit.into())
    }

    /// Disables the request body limit for this route
    ///
    /// For a handler that streams the body and enforces a limit of its own. The limit of
    /// the route group the route belongs to and the application's no longer apply to it.
    pub fn without_body_limit(self) -> Self {
        self.body_limit_override(RequestBodyLimit::Disabled)
    }

    #[inline]
    fn body_limit_override(self, limit: RequestBodyLimit) -> Self {
        self.app.pipeline.endpoints_mut().bind_body_limit(
            &self.method,
            self.pattern.as_ref(),
            limit,
        );
        self
    }
}

impl<'a> RouteGroup<'a> {
    /// Sets the request body limit (in bytes) for every route of this group
    ///
    /// It replaces the application's limit, set with
    /// [`App::with_body_limit`](crate::App::with_body_limit), for these routes and for the
    /// group's fallback - and it can raise it as well as lower it. Applies to the routes
    /// mapped before this call as well as after it, and the most specific limit wins: a route
    /// or a nested group that set a limit of its own keeps it.
    ///
    /// A request whose body goes over the limit is answered `413 Content Too Large` as the
    /// handler reads it, and one whose `Content-Length` already says it won't fit is answered
    /// before any of it is read.
    ///
    /// # Parameters
    /// - `Limit::Default` - the framework default (5 MB), not the application's
    /// - `Limit::Limited(n)` - an explicit limit
    /// - `Limit::Unlimited` - no limit at all, as [`RouteGroup::without_body_limit`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, Json, Limit, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/api", |api| {
    ///     // What a JSON API needs
    ///     api.with_body_limit(Limit::Limited(64 * 1024));
    ///
    ///     api.map_post("/chat", |Json(message): Json<String>| async move {
    ///         ok!(message)
    ///     });
    ///
    ///     // The one route that takes a file
    ///     api.map_post("/attachments", |req: HttpRequest| async move {
    ///         // stream `req.into_body()` somewhere
    ///         ok!()
    ///     })
    ///     .with_body_limit(Limit::Limited(20 * 1024 * 1024));
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn with_body_limit(&mut self, limit: Limit<usize>) -> &mut Self {
        self.body_limit = Some(limit.into());
        self
    }

    /// Disables the request body limit for every route of this group
    ///
    /// For routes that stream the body and enforce a limit of their own. Applies the way
    /// [`RouteGroup::with_body_limit`] does: a route or a nested group that set a limit of
    /// its own keeps it.
    pub fn without_body_limit(&mut self) -> &mut Self {
        self.body_limit = Some(RequestBodyLimit::Disabled);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_BODY_SIZE, RequestBodyLimit};
    use crate::Limit;

    #[test]
    fn it_creates_default_body_limit() {
        let limit = RequestBodyLimit::default();
        let RequestBodyLimit::Enabled(limit) = limit else {
            unreachable!()
        };

        assert_eq!(limit, DEFAULT_BODY_SIZE)
    }

    #[test]
    fn it_converts_from_limited() {
        let limit = Limit::Limited(10);

        let body_limit = RequestBodyLimit::from(limit);
        let RequestBodyLimit::Enabled(limit) = body_limit else {
            unreachable!()
        };

        assert_eq!(limit, 10)
    }

    #[test]
    fn it_converts_from_default_limit() {
        let limit = Limit::Default;

        let body_limit = RequestBodyLimit::from(limit);
        let RequestBodyLimit::Enabled(limit) = body_limit else {
            unreachable!()
        };

        assert_eq!(limit, DEFAULT_BODY_SIZE)
    }

    #[test]
    fn it_converts_from_unlimited() {
        let limit = Limit::Unlimited;

        let body_limit = RequestBodyLimit::from(limit);

        assert_eq!(body_limit, RequestBodyLimit::Disabled)
    }
}
