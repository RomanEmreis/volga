//! Tools and utilities for handling static files

use crate::http::endpoints::{
    args::{FromPayload, Payload, Source},
    route::PathArgs,
};
use crate::{
    App, HttpResult,
    app::HostEnv,
    error::Error,
    html, html_file,
    http::{Method, StatusCode},
    routing::RouteGroup,
    status,
};
use futures_util::future::{Ready, ready};
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
use tokio::fs::{File, canonicalize, metadata};

use crate::headers::{
    CACHE_CONTROL, CacheControl, ETAG, HttpHeaders, LAST_MODIFIED, ResponseCaching,
    helpers::validate_preconditions,
};

mod file_listing;

const ACCESS_DENIED_MESSAGE: &str = "Access is denied.";

/// Prefix [`App::map_static_assets`] labels the segments of a nested path with:
/// `path_0`, `path_1`, ...
///
/// The names are labels only. The router keeps at most one dynamic child per
/// node and reuses whichever name got there first, so a name declared here is
/// not necessarily the name a request arrives under - see [`AssetPath`].
const PATH_SEGMENT_PREFIX: &str = "path_";

/// The path of a static asset, rebuilt from the segments the router matched.
struct AssetPath(PathBuf);

#[inline]
async fn index(method: Method, env: HostEnv, headers: HttpHeaders) -> HttpResult {
    if env.show_files_listing() {
        let path = env.content_root().to_path_buf();
        respond_with_folder_impl(path, env.content_root(), true).await
    } else {
        let index_path = env.index_path().to_path_buf();
        respond_with_shell_impl(index_path, &method, &headers, env.shell_cache_control()).await
    }
}

#[inline]
async fn fallback(method: Method, env: HostEnv, headers: HttpHeaders) -> HttpResult {
    match env.fallback_path() {
        None => status!(404),
        Some(path) => {
            let path = path.to_path_buf();
            respond_with_shell_impl(path, &method, &headers, env.shell_cache_control()).await
        }
    }
}

/// Answers with a file addressed by a stable name - the index or the fallback one.
///
/// The shell is served `no-cache` by default, which is a promise that it will be revalidated
/// rather than that it will be re-sent: these two are reached by their own handlers rather
/// than through [`respond_with_file`], so they have to run the request's validators
/// themselves or every reload would pay for a full body.
#[inline]
async fn respond_with_shell_impl(
    path: PathBuf,
    method: &Method,
    headers: &HttpHeaders,
    cache_control: CacheControl,
) -> HttpResult {
    let metadata = metadata(&path).await?;
    let caching = ResponseCaching::try_from(&metadata)?.with_cache_control(cache_control);

    respond_with_file_or_304_impl(path, caching, method, headers).await
}

#[inline]
async fn respond_with_file(
    AssetPath(path): AssetPath,
    method: Method,
    headers: HttpHeaders,
    env: HostEnv,
) -> HttpResult {
    let path = env.content_root().join(path);
    // The index and the fallback file keep their own policy even when they are requested
    // by name, since the name they are addressed by is stable either way.
    let cache_control = if env.is_shell_path(&path) {
        env.shell_cache_control()
    } else {
        env.asset_cache_control()
    };

    let response = respond_with_file_or_dir_impl(
        path,
        &method,
        &headers,
        env.content_root(),
        env.show_files_listing(),
        cache_control,
    )
    .await;
    match response {
        Ok(response) => Ok(response),
        Err(err) if err.status == StatusCode::NOT_FOUND => fallback(method, env, headers).await,
        Err(err) => Err(err),
    }
}

impl FromPayload for AssetPath {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::PathArgs;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::PathArgs(args) = payload else {
            unreachable!()
        };
        ready(assemble_path(args))
    }
}

/// Rebuilds a nested request path from the segments the router matched.
///
/// The segments are joined in the order the router bound them, which is the
/// order they appear in the path. Neither their names nor their count can be
/// relied upon: names are rewritten when another route already owns the node
/// (see [`PATH_SEGMENT_PREFIX`]), and the count varies with the depth of the
/// matched route.
#[inline]
fn assemble_path(args: &PathArgs) -> Result<AssetPath, Error> {
    let mut path = PathBuf::new();
    for arg in args.iter() {
        path.push(percent_decode(arg.value.as_ref())?.as_ref());
    }
    Ok(AssetPath(path))
}

/// Decodes the `%XX` escapes of a single path segment.
///
/// Unlike form decoding, `+` is left as it is: in a request target it is a
/// literal plus sign rather than a space (RFC 3986 Section 3.3).
#[inline]
fn percent_decode(segment: &str) -> Result<Cow<'_, str>, Error> {
    if !segment.contains('%') {
        return Ok(Cow::Borrowed(segment));
    }

    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            decoded.push(bytes[i]);
            i += 1;
            continue;
        }

        let escape = bytes.get(i + 1..i + 3).ok_or_else(malformed_escape)?;
        let hi = char::from(escape[0])
            .to_digit(16)
            .ok_or_else(malformed_escape)?;

        let lo = char::from(escape[1])
            .to_digit(16)
            .ok_or_else(malformed_escape)?;

        decoded.push((hi * 16 + lo) as u8);
        i += 3;
    }

    String::from_utf8(decoded)
        .map(Cow::Owned)
        .map_err(|_| malformed_escape())
}

#[inline]
fn malformed_escape() -> Error {
    Error::client_error("Static files error: malformed percent-encoding in the request path")
}

#[inline]
async fn respond_with_file_or_dir_impl(
    path: PathBuf,
    method: &Method,
    headers: &HttpHeaders,
    content_root: &Path,
    show_files_listing: bool,
    cache_control: CacheControl,
) -> HttpResult {
    let (path, content_root) = sanitize_path(path, content_root).await?;
    let metadata = metadata(&path).await?;
    match (metadata.is_dir(), show_files_listing) {
        (true, false) => status!(403, text: ACCESS_DENIED_MESSAGE),
        (true, true) => respond_with_folder_impl(path, &content_root, false).await,
        (false, _) => {
            let caching = ResponseCaching::try_from(&metadata)?.with_cache_control(cache_control);
            respond_with_file_or_304_impl(path, caching, method, headers).await
        }
    }
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
    path: PathBuf,
    caching: ResponseCaching,
    method: &Method,
    headers: &HttpHeaders,
) -> HttpResult {
    if validate_preconditions(method, &caching, headers) {
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
async fn respond_with_folder_impl(path: PathBuf, content_root: &Path, is_root: bool) -> HttpResult {
    let display_path = if is_root {
        "/".to_string()
    } else {
        path.strip_prefix(content_root)
            .unwrap_or(&path)
            .display()
            .to_string()
    };

    let html = file_listing::generate_html(&path, &display_path, is_root).await?;

    html!(html)
}

#[inline]
async fn respond_with_file_impl(path: PathBuf, caching: ResponseCaching) -> HttpResult {
    match File::open(&path).await {
        Err(err) => Err(err.into()),
        Ok(index) => html_file!(path, index; [
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
            "Access is denied.",
        ));
    }
    Ok((path, content_root))
}

/// Calculates max folders depth for the given root
#[inline]
fn max_folder_depth<P: AsRef<Path>>(path: P) -> u32 {
    fn helper(path: &Path, depth: u32) -> u32 {
        let mut max_depth = depth;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    max_depth = max_depth.max(helper(&entry_path, depth + 1));
                }
            }
        }
        max_depth
    }

    helper(path.as_ref(), 1)
}

impl RouteGroup<'_> {
    /// Configures a static asset
    ///
    /// All the `GET`/`HEAD` requests to root `/` will be redirected to `/index.html`
    /// as well as all the `GET`/`HEAD` requests to `/{file_name}`
    /// will respond with the appropriate page
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
    ///     g.map_static_assets();
    /// });
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_static_assets(&mut self) -> &mut Self {
        // Configure routes depending on root folder depth
        let folder_depth = max_folder_depth(self.app.host_env.content_root());
        let mut segment = String::new();
        for i in 0..folder_depth {
            segment.push_str(&format!("/{{{PATH_SEGMENT_PREFIX}{i}}}"));
            self.map_get(&segment, respond_with_file);
        }
        self.map_get("/", index);
        self
    }

    /// Configures a static files server
    ///
    /// This method combines logic [`App::map_static_assets`] and [`App::map_fallback_to_file`].
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
        self.map_static_assets()
    }
}

impl App {
    /// Configures a static files server
    ///
    /// This method combines logic [`App::map_static_assets`] and [`App::map_fallback_to_file`].
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

        self.map_static_assets()
    }

    /// Configures a static asset
    ///
    /// All the `GET`/`HEAD` requests to root `/` will be redirected to `/index.html`
    /// as well as all the `GET`/`HEAD` requests to `/{file_name}`
    /// will respond with the appropriate page
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
    /// app.map_static_assets();
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_static_assets(&mut self) -> &mut Self {
        // Configure routes depending on root folder depth
        let folder_depth = max_folder_depth(self.host_env.content_root());
        let mut segment = String::new();
        for i in 0..folder_depth {
            segment.push_str(&format!("/{{{PATH_SEGMENT_PREFIX}{i}}}"));
            self.map_get(&segment, respond_with_file);
        }
        self.map_get("/", index).app
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
        Cow, assemble_path, fallback, index, max_folder_depth, percent_decode,
        respond_with_file_impl, respond_with_file_or_dir_impl, respond_with_folder_impl,
    };
    use crate::app::HostEnv;
    use crate::headers::{
        CACHE_CONTROL, CacheControl, HeaderMap, HeaderValue, HttpHeaders, IF_MODIFIED_SINCE,
        IF_NONE_MATCH, ResponseCaching,
    };
    use crate::http::Method;
    use crate::http::endpoints::route::{PathArg, PathArgs};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};
    use tokio::fs::metadata;

    fn no_headers() -> HttpHeaders {
        HttpHeaders::from(HeaderMap::new())
    }

    fn if_none_match(etag: &str) -> HttpHeaders {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, etag.try_into().unwrap());
        HttpHeaders::from(headers)
    }

    #[tokio::test]
    async fn it_returns_304_for_an_index_whose_etag_still_matches() {
        let env = HostEnv::new("tests/static");
        let metadata = metadata(env.index_path()).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();

        let response = index(Method::GET, env, if_none_match(caching.etag()))
            .await
            .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_304_for_a_fallback_whose_etag_still_matches() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");
        let metadata = metadata(env.fallback_path().unwrap()).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();

        let response = fallback(Method::GET, env, if_none_match(caching.etag()))
            .await
            .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_the_cache_control_on_a_304() {
        let path = PathBuf::from("tests/static/index.html");
        let metadata = metadata(&path).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();

        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &if_none_match(caching.etag()),
            &path,
            false,
            CacheControl::ASSET,
        )
        .await
        .unwrap();

        assert_eq!(response.status(), 304);
        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "max-age=86400, public, immutable"
        );
    }

    #[tokio::test]
    async fn it_ignores_the_date_when_the_etag_says_the_file_changed() {
        let path = PathBuf::from("tests/static/index.html");
        let metadata = metadata(&path).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();

        // A client holding the shell from a build that has since been rolled back: its
        // `ETag` no longer matches what is on disk, but the date it remembers is newer
        // than the restored file's `mtime`.
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, "\"not-the-tag-on-disk\"".try_into().unwrap());
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(
                caching.last_modified + Duration::from_secs(60),
            ))
            .unwrap(),
        );

        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &HttpHeaders::from(headers),
            &path,
            false,
            CacheControl::ASSET,
        )
        .await
        .unwrap();

        // RFC 9110 Section 13.1.3: `If-Modified-Since` is ignored when `If-None-Match` is
        // there, so the mismatching tag decides and the client is sent the current file.
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn it_still_reads_the_date_when_no_etag_was_sent() {
        let path = PathBuf::from("tests/static/index.html");
        let metadata = metadata(&path).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(
                caching.last_modified + Duration::from_secs(60),
            ))
            .unwrap(),
        );

        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &HttpHeaders::from(headers),
            &path,
            false,
            CacheControl::ASSET,
        )
        .await
        .unwrap();

        assert_eq!(response.status(), 304);
    }

    #[tokio::test]
    async fn it_returns_index() {
        let env = HostEnv::new("tests/static");

        let index_response = index(Method::GET, env, no_headers()).await;

        assert!(index_response.is_ok());
        assert_eq!(
            index_response
                .unwrap()
                .headers()
                .get("Content-Type")
                .unwrap(),
            "text/html"
        );
    }

    #[tokio::test]
    async fn it_returns_root_folder_files_listing() {
        let env = HostEnv::new("tests/static").with_files_listing();

        let index_response = index(Method::GET, env, no_headers()).await;

        assert!(index_response.is_ok());
        assert_eq!(
            index_response
                .unwrap()
                .headers()
                .get("Content-Type")
                .unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_returns_fallback() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");

        let index_response = fallback(Method::GET, env, no_headers()).await;

        assert!(index_response.is_ok());
        assert_eq!(
            index_response
                .unwrap()
                .headers()
                .get("Content-Type")
                .unwrap(),
            "text/html"
        );
    }

    #[tokio::test]
    async fn it_returns_index_with_the_shell_cache_control() {
        let env = HostEnv::new("tests/static");

        let response = index(Method::GET, env, no_headers()).await.unwrap();

        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_returns_index_with_a_configured_cache_control() {
        let env = HostEnv::new("tests/static")
            .with_shell_cache_control(|cc| cc.with_no_store().with_private());

        let response = index(Method::GET, env, no_headers()).await.unwrap();

        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "no-cache, no-store, private"
        );
    }

    #[tokio::test]
    async fn it_returns_fallback_with_the_shell_cache_control() {
        let env = HostEnv::new("tests/static").with_fallback_file("index.html");

        let response = fallback(Method::GET, env, no_headers()).await.unwrap();

        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-cache");
    }

    #[tokio::test]
    async fn it_responds_with_the_given_cache_control() {
        let path = PathBuf::from("tests/static/index.html");
        let headers = HttpHeaders::from(HeaderMap::new());
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            false,
            CacheControl::default().with_max_age(60).with_public(),
        )
        .await
        .unwrap();

        assert_eq!(
            response.headers().get(CACHE_CONTROL).unwrap(),
            "max-age=60, public"
        );
    }

    #[tokio::test]
    async fn it_returns_no_fallback() {
        let env = HostEnv::new("tests/static");

        let index_response = fallback(Method::GET, env, no_headers()).await;

        assert!(index_response.is_ok());
        assert_eq!(index_response.unwrap().status(), 404);
    }

    #[tokio::test]
    async fn it_responds_with_file() {
        let path = PathBuf::from("tests/static/index.html");
        let metadata = metadata(&path).await.unwrap();
        let resp_caching = ResponseCaching::try_from(&metadata).unwrap();
        let index_response = respond_with_file_impl(path, resp_caching).await;

        assert!(index_response.is_ok());
        assert_eq!(
            index_response
                .unwrap()
                .headers()
                .get("Content-Type")
                .unwrap(),
            "text/html"
        );
    }

    #[tokio::test]
    async fn it_responds_with_folder() {
        let path = PathBuf::from("tests/static");
        let index_response = respond_with_folder_impl(path.clone(), &path, true).await;

        assert!(index_response.is_ok());
        assert_eq!(
            index_response
                .unwrap()
                .headers()
                .get("Content-Type")
                .unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_responds_with_directory_listing() {
        let path = PathBuf::from("tests/static");
        let headers = HttpHeaders::from(HeaderMap::new());
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            true,
            CacheControl::ASSET,
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(
            response.unwrap().headers().get("Content-Type").unwrap(),
            "text/html; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn it_responds_with_403_as_shows_files_is_false() {
        let path = PathBuf::from("tests/static");
        let headers = HttpHeaders::from(HeaderMap::new());
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            false,
            CacheControl::ASSET,
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 403);
    }

    #[tokio::test]
    async fn it_responds_with_html_file() {
        let path = PathBuf::from("tests/static/index.html");
        let headers = HeaderMap::new();
        let headers = HttpHeaders::from(headers);
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            false,
            CacheControl::ASSET,
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(
            response.unwrap().headers().get("Content-Type").unwrap(),
            "text/html"
        );
    }

    #[tokio::test]
    async fn it_responds_with_304_as_file_was_not_changed() {
        let path = PathBuf::from("tests/static/index.html");
        let now = SystemTime::now() - Duration::from_secs(10);

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap(),
        );

        let headers = HttpHeaders::from(headers);
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            false,
            CacheControl::ASSET,
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 304);
    }

    #[tokio::test]
    async fn it_responds_with_304_as_file_has_same_etag() {
        let path = PathBuf::from("tests/static/index.html");
        let metadata = metadata(&path).await.unwrap();
        let caching = ResponseCaching::try_from(&metadata).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, caching.etag().try_into().unwrap());

        let headers = HttpHeaders::from(headers);
        let response = respond_with_file_or_dir_impl(
            path.clone(),
            &Method::GET,
            &headers,
            &path,
            false,
            CacheControl::ASSET,
        )
        .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 304);
    }

    #[test]
    fn it_calculates_max_folder_depth() {
        let depth = max_folder_depth("tests");

        assert_eq!(depth, 3);
    }

    fn args<const N: usize>(values: [(&str, &str); N]) -> PathArgs {
        values
            .iter()
            .map(|(name, value)| PathArg {
                name: (*name).into(),
                value: (*value).into(),
            })
            .collect()
    }

    fn assembled<const N: usize>(values: [(&str, &str); N]) -> PathBuf {
        assemble_path(&args(values)).unwrap().0
    }

    #[test]
    fn it_assembles_empty_path_from_no_segments() {
        assert_eq!(assembled([]), PathBuf::new());
    }

    #[test]
    fn it_assembles_single_segment_path() {
        assert_eq!(
            assembled([("path_0", "favicon.svg")]),
            PathBuf::from("favicon.svg")
        );
    }

    #[test]
    fn it_assembles_nested_path_in_match_order() {
        assert_eq!(
            assembled([("path_0", "assets"), ("path_1", "app.css")]),
            ["assets", "app.css"].iter().collect::<PathBuf>()
        );
    }

    #[test]
    fn it_assembles_deeply_nested_path_in_match_order() {
        let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        let args = names
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("path_{i}"), (*v).to_string()))
            .collect::<Vec<_>>();
        let args = args
            .iter()
            .map(|(name, value)| PathArg {
                name: name.as_str().into(),
                value: value.as_str().into(),
            })
            .collect::<PathArgs>();

        assert_eq!(
            assemble_path(&args).unwrap().0,
            names.iter().collect::<PathBuf>()
        );
    }

    #[test]
    fn it_assembles_path_whatever_the_segments_are_named() {
        // The router keeps one dynamic child per node and reuses the name that
        // got there first, so a segment can arrive under any name at all.
        assert_eq!(
            assembled([("lang", "assets"), ("path_1", "app.css")]),
            ["assets", "app.css"].iter().collect::<PathBuf>()
        );
        assert_eq!(
            assembled([("lang", "favicon.svg")]),
            PathBuf::from("favicon.svg")
        );
    }

    #[test]
    fn it_percent_decodes_segments() {
        assert_eq!(
            assembled([("path_0", "my%20file.css")]),
            PathBuf::from("my file.css")
        );
        assert_eq!(
            assembled([("path_0", "%D1%84%D0%B0%D0%B9%D0%BB.txt")]),
            PathBuf::from("\u{0444}\u{0430}\u{0439}\u{043b}.txt")
        );
    }

    #[test]
    fn it_leaves_a_plus_alone_when_decoding() {
        // `+` is a space in a form body, but a literal plus in a request target.
        assert_eq!(
            assembled([("path_0", "my+file.css")]),
            PathBuf::from("my+file.css")
        );
    }

    #[test]
    fn it_borrows_a_segment_that_needs_no_decoding() {
        assert!(matches!(percent_decode("app.css"), Ok(Cow::Borrowed(_))));
    }

    #[test]
    fn it_rejects_malformed_percent_encoding() {
        for segment in ["%", "%2", "%zz", "%2z", "app%.css"] {
            assert!(
                percent_decode(segment).is_err(),
                "expected `{segment}` to be rejected"
            );
        }
    }

    #[test]
    fn it_rejects_percent_encoding_that_is_not_utf8() {
        assert!(percent_decode("%FF%FE").is_err());
    }
}
