//! `Config<T>` extractor — provides read access to a pre-deserialized config section.

use crate::{
    HttpRequest,
    config::store::ConfigStore,
    error::Error,
    http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload, Source},
};
use futures_util::future::{Ready, ready};
use hyper::http::request::Parts;
use std::{ops::Deref, sync::Arc};

/// Provides read access to a pre-deserialized config section `T`.
///
/// `T` must be registered via [`ConfigBuilder::bind_section`] before the server starts.
/// On each request, `Config<T>` performs one atomic load + `Arc::clone` — no deserialization.
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
///         .with_config(|cfg| cfg.from_file("app_config.toml").bind_section::<Database>("database"))?;
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
            .get::<Arc<ConfigStore>>()
            .ok_or_else(|| Error::server_error("Config store not found in extensions"))?;
        let arc = store
            .get::<T>()
            .ok_or_else(|| Error::server_error("Config section not registered for this type"))?;
        Ok(Config(arc))
    }
}

impl<T: Send + Sync + 'static> FromRequestParts for Config<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Self::from_extensions(&parts.extensions)
    }
}

impl<T: Send + Sync + 'static> FromRequestRef for Config<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::from_extensions(req.extensions())
    }
}

impl<T: Send + Sync + 'static> FromPayload for Config<T> {
    type Future = Ready<Result<Self, Error>>;
    const SOURCE: Source = Source::Parts;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else {
            unreachable!()
        };
        ready(Self::from_parts(parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::store::{ConfigStore, SectionKind};
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
        ext.insert(Arc::new(store));
        ext
    }

    #[test]
    fn config_deref_reads_field() {
        let ext = make_extensions();
        let mut req = hyper::Request::get("/").body(()).unwrap();
        *req.extensions_mut() = ext;
        let (parts, _) = req.into_parts();
        let cfg = Config::<Db>::from_parts(&parts).unwrap();
        assert_eq!(cfg.url, "postgres://localhost/mydb");
    }

    #[test]
    fn config_missing_store_returns_err() {
        let req = hyper::Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();
        let result = Config::<Db>::from_parts(&parts);
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
        let result = Config::<Other>::from_parts(&parts);
        assert!(result.is_err());
    }
}
