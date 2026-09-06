//! Tools and utilities for handling static files
//!
//! # Where the files are served from
//!
//! The static file server is middleware, not routing. [`App::use_static_files`] registers a
//! handler in the request pipeline that resolves the request target against the content
//! root, answers with the file when one is there, and declines otherwise - the router knows
//! nothing about static content, so nothing about a directory tree reaches it.
//!
//! What follows from that:
//!
//! * The content root is read when a request asks for something, not walked at startup, so a
//!   directory created while the server is running is served like any other.
//! * The request target is used as it arrived rather than taken apart into route parameters
//!   and put back together, at any depth.
//! * Nothing is registered in the router, so no route is shadowed by static content, none of
//!   it shows up in the route listing, and none of it has to be described in an OpenAPI spec.
//! * A file answers before any route does. The mount is what a request under it reaches
//!   first, so a file that exists is served even where a route was mapped for the same path.
//!   Mount the files under a group prefix to keep them to one part of the URL space.
//! * Where the mount sits among the other middleware is where it was registered. Register it
//!   after compression to have the files compressed, after CORS to have the headers on them,
//!   and before whatever should not run for a file that is served from disk.
//!
//! A request nothing under the content root answers goes on to routing, so
//! [`App::map_fallback_to_file`] still answers it - an SPA shell is served exactly as before.

use crate::{
    App, HttpResult,
    app::{HostEnv, warn},
    error::Error,
    html, html_file,
    http::{
        IntoResponse, Method, StatusCode,
        endpoints::route::{Layer, RoutePipeline, join_path},
    },
    middleware::{HttpContext, Middleware, MiddlewareFn, NextFn},
    routing::RouteGroup,
    status,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs::{File, canonicalize, metadata};

use crate::headers::{
    CACHE_CONTROL, CacheControl, ETAG, HeaderMap, HttpHeaders, LAST_MODIFIED, ResponseCaching,
    helpers::validate_preconditions,
};

mod file_listing;
pub(crate) mod path;

use path::{Target, resolve};

const ACCESS_DENIED_MESSAGE: &str = "Access is denied.";

/// A static file mount: everything under the content root, answered under a path prefix.
///
/// A mount is middleware, and it carries the pipeline a route carries - the group's
/// middleware ahead of the layer that answers with the file. It is created when
/// `use_static_assets` is called, takes the middleware of every scope around it while those
/// scopes close, and is composed and registered by [`mount`](Self::mount) at the end. The
/// two states are the two states of [`RoutePipeline`] and need no second type here, as they
/// need none for a route.
pub(crate) struct StaticMount {
    /// The prefix this mount answers under, without a trailing slash. Empty for a mount
    /// that answers the whole application.
    prefix: Box<str>,

    /// The pipeline that answers with the file, headed by the middleware of the scope this
    /// mount belongs to.
    pipeline: RoutePipeline,
}

impl Middleware for StaticMount {
    #[inline]
    fn call(
        &self,
        ctx: HttpContext,
        next: NextFn,
    ) -> impl Future<Output = HttpResult> + Send + 'static {
        // Whether this mount answers a request at all is decided here, before anything is
        // awaited: a request for something else pays for a method check and a prefix
        // comparison, and never reaches the filesystem.
        let target = if is_retrieval(ctx.request().method()) {
            resolve(ctx.request().uri().path(), &self.prefix)
        } else {
            Ok(None)
        };

        let pipeline = self.pipeline.clone();

        async move {
            let mut ctx = ctx;
            let target = match target {
                Ok(Some(target)) => target,
                Ok(None) => return next(ctx).await,
                Err(err) => return err.into_response(),
            };

            let serving = match ctx.request().extensions().get::<HostEnv>() {
                Some(env) => probe(env, target).await,
                None => None,
            };

            let Some(serving) = serving else {
                return next(ctx).await;
            };

            // The pipeline is composed once, at startup, and is type-erased - so what this
            // request resolved to reaches its last layer the way everything else a handler
            // needs reaches one.
            ctx.request_mut().extensions_mut().insert(Arc::new(serving));
            pipeline.call(ctx).await
        }
    }
}

/// Spells a mount's prefix for a message to a human, where the empty prefix of an
/// application-wide mount would read as nothing at all.
#[inline]
fn mount_name(prefix: &str) -> &str {
    if prefix.is_empty() { "/" } else { prefix }
}

/// The layer that answers with the file, and the tail of every mount's pipeline.
///
/// It reads back what [`probe`] decided, so that the layers a group put in front of it - its
/// `filter`, its `authorize`, its `map_ok` - run first, exactly as they do for a route.
#[inline]
fn serve_layer() -> MiddlewareFn {
    Arc::new(|ctx: HttpContext, _| {
        Box::pin(async move {
            let request = ctx.request();
            match request.extensions().get::<Arc<Serving>>() {
                Some(serving) => respond(serving, request.method(), request.headers()).await,
                // The mount puts one there on the way in, and nothing else calls this
                // pipeline, so no request arrives here without one.
                None => status!(500),
            }
        })
    })
}

impl StaticMount {
    /// Creates a mount that answers under `prefix`.
    #[inline]
    pub(crate) fn new(prefix: &str) -> Self {
        // Spelled the way the router spells a route registered under the same prefix, so
        // that a mount and a route in one group agree on where the group is - and then
        // stripped of the trailing slash, which a request target is compared up to rather
        // than against. What is left of `app.group("/", ..)` is the empty prefix: the whole
        // application, which is what that group is.
        let prefix = join_path(prefix, "");

        Self {
            prefix: prefix.trim_end_matches('/').into(),
            pipeline: RoutePipeline::from(Layer::from(serve_layer())),
        }
    }

    /// Puts the middleware of an enclosing scope in front of this mount, where a route
    /// takes the same middleware from the same group.
    #[inline]
    pub(crate) fn prepend(&mut self, layers: &[MiddlewareFn]) {
        self.pipeline.prepend(layers);
    }

    /// Composes this mount's pipeline and registers it in the application's.
    ///
    /// A prefix already answered by a mount is left to that mount: a second one there would
    /// answer nothing the first did not, and the middleware it carries would never run, so
    /// registering it would only cost every request a second look at the filesystem while
    /// looking like a second policy applies.
    #[inline]
    pub(crate) fn mount(mut self, app: &mut App) {
        if !app.pipeline.claim_static_mount(&self.prefix) {
            warn(&format!(
                "Static files are already served under '{}'; this registration does nothing. \
                 Remove it, or move what it carries to the one that answers.",
                mount_name(&self.prefix)
            ));
            return;
        }

        self.pipeline.compose();
        app.attach(self);
    }
}

/// What a request that a mount answers is answered with.
enum Serving {
    /// A file, and the caching policy the name it is addressed by earns it.
    File {
        path: PathBuf,
        caching: ResponseCaching,
    },

    /// A listing of a directory's contents.
    Listing {
        path: PathBuf,
        content_root: PathBuf,
        is_root: bool,
    },

    /// A directory with listing disabled, or a path that resolved outside the content root.
    Denied,
}

/// A file is a representation to retrieve: `GET` asks for it and `HEAD` asks for the headers
/// it would come with. Every other method is left to routing, whatever is on disk under the
/// name it was aimed at.
#[inline]
fn is_retrieval(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD)
}

/// Decides what, if anything, under the content root answers `target`.
///
/// `None` is a request the mount declines: nothing is there under that name, so routing has
/// its turn and the application's fallback - [`App::map_fallback_to_file`] among them -
/// answers it. That is the common answer for a request aimed at a route rather than a file,
/// and it costs a single failed `metadata` call.
#[inline]
async fn probe(env: &HostEnv, target: Target) -> Option<Serving> {
    match target {
        Target::Root => probe_root(env).await,
        Target::Relative(relative) => probe_asset(env, relative).await,
    }
}

/// Answers the mount point itself with the index file, or with a listing of the content root
/// when [`HostEnv::show_files_listing`] is on.
#[inline]
async fn probe_root(env: &HostEnv) -> Option<Serving> {
    let content_root = env.content_root();
    if env.show_files_listing() {
        metadata(content_root).await.ok().filter(|m| m.is_dir())?;
        return Some(Serving::Listing {
            path: content_root.to_path_buf(),
            content_root: content_root.to_path_buf(),
            is_root: true,
        });
    }

    let path = env.index_path();
    let metadata = metadata(path).await.ok().filter(|m| m.is_file())?;
    let caching = ResponseCaching::try_from(&metadata)
        .ok()?
        .with_cache_control(env.shell_cache_control());

    Some(Serving::File {
        path: path.to_path_buf(),
        caching,
    })
}

/// Answers a path under the content root with the file it names, or with the listing of the
/// directory it names.
#[inline]
async fn probe_asset(env: &HostEnv, relative: PathBuf) -> Option<Serving> {
    let path = env.content_root().join(relative);

    // The index and the fallback file keep their own policy even when they are requested
    // by name, since the name they are addressed by is stable either way.
    let cache_control = if env.is_shell_path(&path) {
        env.shell_cache_control()
    } else {
        env.asset_cache_control()
    };

    // Asked first, and answered from a single `stat`: a request nothing is there for is
    // declined here, before the two `canonicalize` calls below.
    let metadata = metadata(&path).await.ok()?;

    // Defence in depth. A request target is built from ordinary path components alone, so
    // it cannot climb out of the content root on its own - a symlink under the root can.
    let (path, content_root) = match sanitize_path(path, env.content_root()).await {
        Ok(paths) => paths,
        Err(err) if err.status == StatusCode::FORBIDDEN => return Some(Serving::Denied),
        // It was there a syscall ago and cannot be resolved now: nothing to serve.
        Err(_) => return None,
    };

    if metadata.is_dir() {
        let serving = if env.show_files_listing() {
            Serving::Listing {
                path,
                content_root,
                is_root: false,
            }
        } else {
            Serving::Denied
        };

        return Some(serving);
    }

    let caching = ResponseCaching::try_from(&metadata)
        .ok()?
        .with_cache_control(cache_control);

    Some(Serving::File { path, caching })
}

/// Answers the request with what [`probe`] decided on.
#[inline]
async fn respond(serving: &Serving, method: &Method, headers: &HeaderMap) -> HttpResult {
    match serving {
        Serving::Denied => status!(403, text: ACCESS_DENIED_MESSAGE),
        Serving::Listing {
            path,
            content_root,
            is_root,
        } => respond_with_folder_impl(path, content_root, *is_root).await,
        Serving::File { path, caching } => {
            respond_with_file_or_304_impl(path, caching, method, headers).await
        }
    }
}

/// Answers a request no route was found for with the fallback file.
#[inline]
async fn fallback(method: Method, env: HostEnv, headers: HttpHeaders) -> HttpResult {
    let cache_control = env.shell_cache_control();
    match env.fallback_path() {
        None => status!(404),
        Some(path) => respond_with_shell_impl(path, &method, headers.as_map(), cache_control).await,
    }
}

/// Answers with a file addressed by a stable name - the fallback one.
///
/// The shell is served `no-cache` by default, which is a promise that it will be revalidated
/// rather than that it will be re-sent: the fallback file is reached by its own handler rather
/// than through the static file mount, so it has to run the request's validators itself or
/// every reload would pay for a full body.
#[inline]
async fn respond_with_shell_impl(
    path: &Path,
    method: &Method,
    headers: &HeaderMap,
    cache_control: CacheControl,
) -> HttpResult {
    let metadata = metadata(path).await?;
    let caching = ResponseCaching::try_from(&metadata)?.with_cache_control(cache_control);

    respond_with_file_or_304_impl(path, &caching, method, headers).await
}

/// Answers with a `304` when the request's validators still match the file, and with the
/// file itself otherwise.
///
/// The `304` carries the `Cache-Control` as well: RFC 9111 Section 4.3.4 has a cache update
/// the stored response from the headers of the `304`, so leaving it out would let a cache
/// keep serving a file under the policy it was first stored with - which is exactly what a
/// change to the [`HostEnv`] policy is meant to replace.
#[inline]
async fn respond_with_file_or_304_impl(
    path: &Path,
    caching: &ResponseCaching,
    method: &Method,
    headers: &HeaderMap,
) -> HttpResult {
    if validate_preconditions(method, caching, headers) {
        status!(304; [
            (ETAG, caching.etag()),
            (LAST_MODIFIED, caching.last_modified()),
            (CACHE_CONTROL, caching.cache_control()),
        ])
    } else {
        respond_with_file_impl(path, caching).await
    }
}

#[inline]
async fn respond_with_folder_impl(path: &Path, content_root: &Path, is_root: bool) -> HttpResult {
    let display_path = if is_root {
        "/".to_string()
    } else {
        path.strip_prefix(content_root)
            .unwrap_or(path)
            .display()
            .to_string()
    };

    let html = file_listing::generate_html(path, &display_path, is_root).await?;

    html!(html)
}

#[inline]
async fn respond_with_file_impl(path: &Path, caching: &ResponseCaching) -> HttpResult {
    match File::open(path).await {
        Err(err) => Err(err.into()),
        Ok(file) => html_file!(path, file; [
            (ETAG, caching.etag()),
            (LAST_MODIFIED, caching.last_modified()),
            (CACHE_CONTROL, caching.cache_control()),
        ]),
    }
}

#[inline]
async fn sanitize_path(path: PathBuf, content_root: &Path) -> Result<(PathBuf, PathBuf), Error> {
    let content_root = canonicalize(content_root).await?;
    let path = canonicalize(&path).await?;
    if !path.starts_with(&content_root) {
        return Err(Error::from_parts(
            StatusCode::FORBIDDEN,
            None,
            ACCESS_DENIED_MESSAGE,
        ));
    }
    Ok((path, content_root))
}

impl RouteGroup<'_> {
    /// Serves the static files of the hosting environment under this group's prefix.
    ///
    /// The files are answered by middleware rather than by routes, so nothing is registered
    /// in the router; see the [module documentation](crate::fs::static_files) for what that
    /// means. The group's own middleware wraps the files this mount serves, as it wraps the
    /// routes the group registered.
    ///
    /// > **Note:** a group's CORS policy is bound to the routes the group mapped, and a file
    /// > is served without going through one - so the policy that reaches these files is the
    /// > application's, configured with [`App::with_cors`](crate::App::with_cors).
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, app::HostEnv};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// // Enables static file server
    /// app.group("/static", |g| {
    ///     g.use_static_assets();
    /// });
    /// # app.run().await
    /// # }
    /// ```
    pub fn use_static_assets(&mut self) -> &mut Self {
        self.mounts.push(StaticMount::new(&self.prefix));
        self
    }

    /// Configures a static files server under this group's prefix
    ///
    /// This method combines logic [`RouteGroup::use_static_assets`] and [`App::map_fallback_to_file`].
    /// The last one is called if the `fallback_path` is explicitly provided in [`HostEnv`].
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, app::HostEnv};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// // Enables static file server
    /// app.group("/static", |g| {
    ///     g.use_static_files();
    /// });
    /// # app.run().await
    /// # }
    /// ```
    pub fn use_static_files(&mut self) -> &mut Self {
        // Enable fallback to file if it's provided
        if self.app.host_env.fallback_path().is_some() {
            self.app.map_fallback_to_file();
        }
        self.use_static_assets()
    }
}

impl App {
    /// Configures a static files server
    ///
    /// This method combines logic [`App::use_static_assets`] and [`App::map_fallback_to_file`].
    /// The last one is called if the `fallback_path` is explicitly provided in [`HostEnv`].
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, app::HostEnv};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// // Enables static file server
    /// app.use_static_files();
    /// # app.run().await
    /// # }
    /// ```
    pub fn use_static_files(&mut self) -> &mut Self {
        // Enable fallback to file if it's provided
        if self.host_env.fallback_path().is_some() {
            self.map_fallback_to_file();
        }

        self.use_static_assets()
    }

    /// Serves the static files of the hosting environment.
    ///
    /// A `GET` or `HEAD` request for `/` is answered with the index file - or with a listing
    /// of the content root when [`HostEnv::with_files_listing`] is on - and a request for a
    /// path under it with the file of that name, at any depth.
    ///
    /// The files are answered by middleware rather than by routes, so nothing is registered
    /// in the router and a request nothing answers goes on to routing; see the
    /// [module documentation](crate::fs::static_files) for what follows from that, and for
    /// where this call belongs among the rest of the pipeline.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, app::HostEnv};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// // Enables static file server
    /// app.use_static_assets();
    /// # app.run().await
    /// # }
    /// ```
    pub fn use_static_assets(&mut self) -> &mut Self {
        StaticMount::new("").mount(self);
        self
    }

    /// Adds a special fallback handler that redirects to a specified file
    /// when unregistered resource is requested
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, app::HostEnv};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// // Specifies a file that will be fault back to
    /// let mut app = App::new()
    ///     .with_host_env(|env| env.with_fallback_file("not_found.html"));
    ///
    /// // Enables the special handler that will fall back
    /// // to the specified file
    /// app.map_fallback_to_file();
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_fallback_to_file(&mut self) -> &mut Self {
        self.map_fallback(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Serving, StaticMount, fallback, is_retrieval, probe, resolve, respond,
        respond_with_file_impl, respond_with_folder_impl, sanitize_path,
    };
    use crate::app::HostEnv;
    use crate::headers::{
        CACHE_CONTROL, CacheControl, HeaderMap, HeaderValue, HttpHeaders, IF_MODIFIED_SINCE,
        IF_NONE_MATCH, ResponseCaching,
    };
    use crate::http::{Method, StatusCode};
    use crate::{App, HttpResult};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};
    use tokio::fs::metadata;

    /// Answers `path` the way the mount does, or `None` when the mount declines it.
    async fn serve(
        env: &HostEnv,
        path: &str,
        method: Method,
        headers: HeaderMap,
    ) -> Option<HttpResult> {
        let target = resolve(path, "").unwrap()?;
        let serving = probe(env, target).await?;

        Some(respond(&serving, &method, &headers).await)
    }

    fn no_headers() -> HeaderMap {
        HeaderMap::new()
    }

    fn if_none_match(etag: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, etag.try_into().unwrap());
        headers
    }

    fn if_modified_since(time: SystemTime) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(time)).unwrap(),
        );
        headers
    }

    async fn caching_of(path: &str) -> ResponseCaching {
        let metadata = metadata(path).await.unwrap();
        ResponseCaching::try_from(&metadata).unwrap()
    }

    #[test]
    fn it_answers_retrievals_only() {
        assert!(is_retrieval(&Method::GET));
        assert!(is_retrieval(&Method::HEAD));

        for method in [Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS] {
            assert!(!is_retrieval(&method), "expected `{method}` to be declined");
        }
    }

    #[tokio::test]
    async fn it_declines_a_path_nothing_is_there_for() {
        let env = HostEnv::new("tests/static");

        assert!(
            serve(&env, "/nothing.css", Method::GET, no_headers())
                .await
                .is_none()
        );
        assert!(
            serve(&env, "/deep/unknown", Method::GET, no_headers())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn it_declines_a_traversal_out_of_the_content_root() {
        let env = HostEnv::new("tests/static");

        assert!(
            serve(&env, "/../static/index.html", Method::GET, no_headers())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn it_serves_a_path_of_any_depth() {
        // Nothing is registered per level, so the depth of the content root at startup -
        // which used to be the ceiling - has nothing to do with what is served.
        let env = HostEnv::new("tests/static");

        let response = serve(&env, "/assets/app.css", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), 200);
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");
    }

    #[tokio::test]
    async fn it_returns_304_for_an_index_whose_etag_still_matches() {
        let env = HostEnv::new("tests/static");
        let caching = caching_of(env.index_path().to_str().unwrap()).await;

        let response = serve(&env, "/", Method::GET, if_none_match(caching.etag()))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_304_for_a_fallback_whose_etag_still_matches() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");
        let caching = caching_of(env.fallback_path().unwrap().to_str().unwrap()).await;

        let response = fallback(
            Method::GET,
            env,
            HttpHeaders::from(if_none_match(caching.etag())),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_the_cache_control_on_a_304() {
        let env = HostEnv::new("tests/static");
        let caching = caching_of("tests/static/assets/app.css").await;

        let response = serve(
            &env,
            "/assets/app.css",
            Method::GET,
            if_none_match(caching.etag()),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "max-age=86400, public, immutable"
        );
    }

    #[tokio::test]
    async fn it_ignores_the_date_when_the_etag_says_the_file_changed() {
        let env = HostEnv::new("tests/static");
        let caching = caching_of("tests/static/assets/app.css").await;

        // A client holding an asset from a build that has since been rolled back: its
        // `ETag` no longer matches what is on disk, but the date it remembers is newer
        // than the restored file's `mtime`.
        let mut headers = if_modified_since(caching.last_modified + Duration::from_secs(60));
        headers.insert(IF_NONE_MATCH, "\"not-the-tag-on-disk\"".try_into().unwrap());

        let response = serve(&env, "/assets/app.css", Method::GET, headers)
            .await
            .unwrap()
            .unwrap();

        // RFC 9110 Section 13.1.3: `If-Modified-Since` is ignored when `If-None-Match` is
        // there, so the mismatching tag decides and the client is sent the current file.
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_still_reads_the_date_when_no_etag_was_sent() {
        let env = HostEnv::new("tests/static");
        let caching = caching_of("tests/static/assets/app.css").await;

        let response = serve(
            &env,
            "/assets/app.css",
            Method::GET,
            if_modified_since(caching.last_modified + Duration::from_secs(60)),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(response.status(), 304);
    }

    #[tokio::test]
    async fn it_returns_index() {
        let env = HostEnv::new("tests/static");

        let response = serve(&env, "/", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_returns_root_folder_files_listing() {
        let env = HostEnv::new("tests/static").with_files_listing();

        let response = serve(&env, "/", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_returns_fallback() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");

        let response = fallback(Method::GET, env, HttpHeaders::from(no_headers()))
            .await
            .unwrap();

        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_returns_index_with_the_shell_cache_control() {
        let env = HostEnv::new("tests/static");

        let response = serve(&env, "/", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_index_with_a_configured_cache_control() {
        let env = HostEnv::new("tests/static")
            .with_shell_cache_control(|cc| cc.with_no_store().with_private());

        let response = serve(&env, "/", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, no-store, private"
        );
    }

    #[tokio::test]
    async fn it_returns_the_index_requested_by_name_with_the_shell_cache_control() {
        let env = HostEnv::new("tests/static");

        let response = serve(&env, "/index.html", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_fallback_with_the_shell_cache_control() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");

        let response = fallback(Method::GET, env, HttpHeaders::from(no_headers()))
            .await
            .unwrap();

        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_responds_with_the_configured_asset_cache_control() {
        let env = HostEnv::new("tests/static")
            .with_asset_cache_control(|_| CacheControl::EMPTY.with_max_age(60).with_public());

        let response = serve(&env, "/assets/app.css", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "max-age=60, public"
        );
    }

    #[tokio::test]
    async fn it_returns_no_fallback() {
        let env = HostEnv::new("tests/static");

        let response = fallback(Method::GET, env, HttpHeaders::from(no_headers()))
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn it_responds_with_file() {
        let path = PathBuf::from("tests/static/index.html");
        let caching = caching_of("tests/static/index.html").await;

        let response = respond_with_file_impl(&path, &caching).await.unwrap();

        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_responds_with_folder() {
        let path = PathBuf::from("tests/static");

        let response = respond_with_folder_impl(&path, &path, true).await.unwrap();

        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_responds_with_a_nested_directory_listing() {
        let env = HostEnv::new("tests/static").with_files_listing();

        let response = serve(&env, "/assets", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            response.headers().get("Content-Type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_responds_with_403_as_shows_files_is_false() {
        let env = HostEnv::new("tests/static");

        let response = serve(&env, "/assets", Method::GET, no_headers())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), 403);
    }

    #[tokio::test]
    async fn it_responds_with_403_for_a_path_that_escaped_the_content_root() {
        // The resolver keeps a request target inside the content root on its own; a symlink
        // is what the check after it is for, and this is that check.
        let escaped = sanitize_path(PathBuf::from("Cargo.toml"), Path::new("tests/static"))
            .await
            .unwrap_err();

        assert_eq!(escaped.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn it_responds_with_304_as_file_was_not_changed() {
        let env = HostEnv::new("tests/static");
        let now = SystemTime::now() - Duration::from_secs(10);

        let response = serve(&env, "/index.html", Method::GET, if_modified_since(now))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), 304);
    }

    #[tokio::test]
    async fn it_responds_with_304_as_file_has_same_etag() {
        let env = HostEnv::new("tests/static");
        let caching = caching_of("tests/static/index.html").await;

        let response = serve(
            &env,
            "/index.html",
            Method::GET,
            if_none_match(caching.etag()),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(response.status(), 304);
    }

    #[tokio::test]
    async fn it_declines_the_root_when_there_is_no_index_file() {
        // Nothing under the content root answers `/`, so routing has its turn - the
        // application's own `/` route, or its fallback, answers it.
        let env = HostEnv::new("tests").with_index_file("no-such-index.html");

        assert!(serve(&env, "/", Method::GET, no_headers()).await.is_none());
    }

    #[tokio::test]
    async fn it_declines_everything_when_the_content_root_is_missing() {
        let env = HostEnv::new("tests/no-such-directory");

        assert!(serve(&env, "/", Method::GET, no_headers()).await.is_none());
        assert!(
            serve(&env, "/app.css", Method::GET, no_headers())
                .await
                .is_none()
        );
    }

    /// The number of middlewares registered in the application pipeline. Nothing but the
    /// mounts registers one in these tests, so this counts them.
    fn registered(app: &mut App) -> usize {
        app.pipeline.middlewares_mut().pipeline.len()
    }

    #[test]
    fn it_registers_one_mount_per_prefix() {
        let mut app = App::new();
        app.use_static_assets();
        app.use_static_assets();

        assert_eq!(registered(&mut app), 1);
    }

    #[test]
    fn it_registers_one_mount_per_prefix_asked_for_by_a_group() {
        let mut app = App::new();
        app.group("/static", |g| {
            g.use_static_assets();
            g.use_static_assets();
        });
        // A second group over the same prefix asks for the mount that is already there.
        app.group("/static", |g| {
            g.use_static_assets();
        });

        assert_eq!(registered(&mut app), 1);
    }

    #[test]
    fn it_registers_a_mount_for_every_prefix_that_has_none() {
        let mut app = App::new();
        app.use_static_assets();
        app.group("/static", |g| {
            g.use_static_assets();
            g.group("/inner", |inner| {
                inner.use_static_assets();
            });
        });

        assert_eq!(registered(&mut app), 3);
    }

    #[test]
    fn it_spells_a_mount_prefix_the_way_a_route_is_spelled() {
        for prefix in ["/static", "static", "/static/", "//static"] {
            assert_eq!(
                StaticMount::new(prefix).prefix.as_ref(),
                "/static",
                "{prefix}"
            );
        }

        // A group over the whole application answers everything, as the application-wide
        // mount does.
        for prefix in ["", "/"] {
            assert_eq!(StaticMount::new(prefix).prefix.as_ref(), "", "{prefix}");
        }
    }

    #[tokio::test]
    async fn it_denies_a_directory_when_listing_is_off() {
        let env = HostEnv::new("tests/static");
        let target = resolve("/assets", "").unwrap().unwrap();

        assert!(matches!(probe(&env, target).await, Some(Serving::Denied)));
    }
}
