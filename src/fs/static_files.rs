﻿use tokio::fs::{File, metadata};
use std::{
    collections::HashMap,
    path::{Path, PathBuf}
};
use crate::{
    App,
    Path as RoutePath,
    HttpResult,
    app::HostEnv,
    http::StatusCode,
    routing::RouteGroup,
    html_file,
    html,
    status
};

use crate::headers::{
    ResponseCaching,
    HttpHeaders,
    helpers::{validate_etag, validate_last_modified},
    CACHE_CONTROL, LAST_MODIFIED, ETAG
};

mod file_listing;

#[inline]
async fn index(env: HostEnv) -> HttpResult {
    if env.show_files_listing() {
        let path = env.content_root().to_path_buf();
        respond_with_folder_impl(path, true).await
    } else {
        let index_path = env.index_path().to_path_buf();
        let metadata = metadata(&index_path).await?;
        let caching = ResponseCaching::try_from(&metadata)?;
        
        respond_with_file_impl(index_path, caching).await
    }
}

#[inline]
async fn fallback(env: HostEnv) -> HttpResult {
    match env.fallback_path() {
        None => status!(404),
        Some(path) => {
            let path = path.to_path_buf();
            let metadata = metadata(&path).await?;
            let caching = ResponseCaching::try_from(&metadata)?;
            
            respond_with_file_impl(path, caching).await
        }
    }
}

#[inline]
async fn respond_with_file(
    path: RoutePath<HashMap<String, String>>,
    headers: HttpHeaders,
    env: HostEnv
) -> HttpResult {
    let path = path.values()
        .fold(PathBuf::new(), |mut acc, v| {
            acc.push(v);
            acc
        });
    let path = env.content_root().join(&path);
    match respond_with_file_or_dir_impl(path, headers, env.show_files_listing()).await {
        Ok(response) => Ok(response),
        Err(err) if err.status == StatusCode::NOT_FOUND => fallback(env).await,
        Err(err) => Err(err),
    }
}

#[inline]
async fn respond_with_file_or_dir_impl(
    path: PathBuf,
    headers: HttpHeaders,
    show_files_listing: bool
) -> HttpResult {
    let metadata = metadata(&path).await?;
    match (metadata.is_dir(), show_files_listing) {
        (true, false) => status!(403, "Access is denied."),
        (true, true) => respond_with_folder_impl(path, false).await,
        (false, _) => {
            let caching = ResponseCaching::try_from(&metadata)?;
            if validate_etag(&caching.etag, &headers) ||
                validate_last_modified(caching.last_modified, &headers) {
                status!(304, [
                    (ETAG, caching.etag()),
                    (LAST_MODIFIED, caching.last_modified())
                ])
            } else {
                respond_with_file_impl(path, caching).await
            }
        },
    }
}

#[inline]
async fn respond_with_folder_impl(path: PathBuf, is_root: bool) -> HttpResult {
    let html = file_listing::generate_html(&path, is_root).await?;
    html!(html)
}

#[inline]
async fn respond_with_file_impl(path: PathBuf, caching: ResponseCaching) -> HttpResult {
    match File::open(&path).await {
        Err(err) => Err(err.into()),
        Ok(index) => html_file!(path, index, [
            (ETAG, caching.etag()),
            (LAST_MODIFIED, caching.last_modified()),
            (CACHE_CONTROL, caching.cache_control()),
        ])
    }
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
    /// app.map_group("/static")
    ///     .map_static_assets();
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_static_assets(mut self) -> Self {
        // Configure routes depending on root folder depth
        let folder_depth = max_folder_depth(self.app.host_env.content_root());
        let mut segment = String::new();
        for i in 0..folder_depth {
            segment.push_str(&format!("/{{path_{i}}}"));
            self = self.map_get(&segment, respond_with_file);
        }
        self.map_get("/", index)
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
    /// app
    ///     .map_group("/static")
    ///     .use_static_files();
    /// # app.run().await
    /// # }
    /// ```
    pub fn use_static_files(self) -> Self {
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
            segment.push_str(&format!("/{{path_{i}}}"));
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
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};
    use tokio::fs::metadata;
    use crate::app::HostEnv;
    use crate::headers::{HeaderMap, HeaderValue, HttpHeaders, ResponseCaching, IF_MODIFIED_SINCE, IF_NONE_MATCH};
    use super::{
        index, fallback, respond_with_folder_impl, respond_with_file_impl,
        respond_with_file_or_dir_impl, max_folder_depth
    };
    
    #[tokio::test]
    async fn it_returns_index() {
        let env = HostEnv::new("tests/static");
        
        let index_response = index(env).await;
        
        assert!(index_response.is_ok());
        assert_eq!(index_response.unwrap().headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_returns_root_folder_files_listing() {
        let env = HostEnv::new("tests/static")
            .with_files_listing();

        let index_response = index(env).await;

        assert!(index_response.is_ok());
        assert_eq!(index_response.unwrap().headers().get("Content-Type").unwrap(), "text/html; charset=utf-8");
    }

    #[tokio::test]
    async fn it_returns_fallback() {
        let env = HostEnv::new("tests/static")
            .with_fallback_file("index.html");

        let index_response = fallback(env).await;

        assert!(index_response.is_ok());
        assert_eq!(index_response.unwrap().headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_returns_no_fallback() {
        let env = HostEnv::new("tests/static");

        let index_response = fallback(env).await;

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
        assert_eq!(index_response.unwrap().headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_responds_with_folder() {
        let path = PathBuf::from("tests/static");
        let index_response = respond_with_folder_impl(path, true).await;

        assert!(index_response.is_ok());
        assert_eq!(index_response.unwrap().headers().get("Content-Type").unwrap(), "text/html; charset=utf-8");
    }
    
    #[tokio::test]
    async fn it_responds_with_directory_listing() {
        let path = PathBuf::from("tests/static");
        let headers = HttpHeaders::from(HeaderMap::new());
        let response = respond_with_file_or_dir_impl(path, headers, true).await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().headers().get("Content-Type").unwrap(), "text/html; charset=utf-8");
    }

    #[tokio::test]
    async fn it_responds_with_403_as_shows_files_is_false() {
        let path = PathBuf::from("tests/static");
        let headers = HttpHeaders::from(HeaderMap::new());
        let response = respond_with_file_or_dir_impl(path, headers, false).await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 403);
    }

    #[tokio::test]
    async fn it_responds_with_html_file() {
        let path = PathBuf::from("tests/static/index.html");
        let headers = HeaderMap::new();
        let headers = HttpHeaders::from(headers);
        let response = respond_with_file_or_dir_impl(path, headers, false).await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().headers().get("Content-Type").unwrap(), "text/html");
    }

    #[tokio::test]
    async fn it_responds_with_304_as_file_was_not_changed() {
        let path = PathBuf::from("tests/static/index.html");
        let now = SystemTime::now() - Duration::from_secs(10);

        let mut headers = HeaderMap::new();
        headers.insert(IF_MODIFIED_SINCE, HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap());

        let headers = HttpHeaders::from(headers);
        let response = respond_with_file_or_dir_impl(path, headers, false).await;

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
        let response = respond_with_file_or_dir_impl(path, headers, false).await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 304);
    }
    
    #[test]
    fn it_calculates_max_folder_depth() {
        let depth = max_folder_depth("tests");
        
        assert_eq!(depth, 2);
    }
}