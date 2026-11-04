//! `Config<T>` extractor - provides read access to a pre-deserialized config section.

use crate::http::{FromRequestHead, RequestHead, endpoints::args::sealed::PathAccess};
use crate::{error::Error, http::request_scope::HttpRequestScope};
use std::{ops::Deref, sync::Arc};

/// Provides read access to a pre-deserialized config section `T`.
///
/// `T` must be registered via [`ConfigBuilder::bind_section`](crate::ConfigBuilder::bind_section) before the server starts.
/// On each request, `Config<T>` performs one atomic load + `Arc::clone` - no deserialization.
///
/// # Example
/// ```no_run
/// use volga::{App, Config, ok};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Database { url: String }
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let mut app = App::new()
///         .with_config(|cfg| cfg.with_file("app_config.toml").bind_section::<Database>("database"));
///     // app.map_get("/db", |db: Config<Database>| async move { ok!(db.url.as_str()) });
///     app.run().await
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Config<T: Send + Sync>(Arc<T>);

impl<T: Send + Sync> Deref for Config<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Send + Sync + 'static> Config<T> {
    fn from_extensions(ext: &hyper::http::Extensions) -> Result<Self, Error> {
        let store = ext
            .get::<HttpRequestScope>()
            .and_then(|s| s.config.as_ref())
            .ok_or_else(|| Error::server_error("Config store not found in extensions"))?;
        let arc = store
            .get::<T>()
            .ok_or_else(|| Error::server_error("Config section not registered for this type"))?;
        Ok(Config(arc))
    }
}

impl<T: Send + Sync + 'static> FromRequestHead for Config<T> {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        Self::from_extensions(head.extensions())
    }

    // Reads no path parameter, so a positional extractor next to it takes its own
    const PATH_ACCESS: PathAccess = PathAccess::Ignored;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::store::{ConfigStore, SectionKind};
    use crate::http::FromRequestRef;
    use crate::http::request_scope::HttpRequestScope;
    use hyper::http::Extensions;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Db {
        url: String,
    }

    fn make_extensions() -> Extensions {
        let json = serde_json::json!({ "db": { "url": "postgres://localhost/mydb" } });
        let mut store = ConfigStore::new();
        store
            .register::<Db>("db", SectionKind::Required, &json)
            .unwrap();
        let mut ext = Extensions::new();
        ext.insert(HttpRequestScope {
            config: Some(Arc::new(store)),
            ..HttpRequestScope::default()
        });
        ext
    }

    #[test]
    fn config_deref_reads_field() {
        let ext = make_extensions();
        let mut req = hyper::Request::get("/").body(()).unwrap();
        *req.extensions_mut() = ext;
        let (parts, _) = req.into_parts();
        let cfg = Config::<Db>::from_request(&RequestHead::from_parts(&parts)).unwrap();
        assert_eq!(cfg.url, "postgres://localhost/mydb");
    }

    #[test]
    fn config_missing_store_returns_err() {
        let req = hyper::Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let result = Config::<Db>::from_request(&RequestHead::from_parts(&parts));
        assert!(result.is_err());
    }

    #[test]
    fn config_unregistered_type_returns_err() {
        #[derive(Debug, Deserialize)]
        struct Other {
            _x: u32,
        }
        let ext = make_extensions();
        let mut req = hyper::Request::get("/").body(()).unwrap();
        *req.extensions_mut() = ext;
        let (parts, _) = req.into_parts();
        let result = Config::<Other>::from_request(&RequestHead::from_parts(&parts));
        assert!(result.is_err());
    }
}
