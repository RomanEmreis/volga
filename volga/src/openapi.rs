//! OpenAPI registry and configuration.

use crate::{App, headers::{Header, HttpHeaders, CacheControl, ETag}};
use volga_open_api::ui_html;

pub use volga_open_api::{
    OpenApiConfig, 
    OpenApiRegistry, 
    OpenApiSpec, 
    OpenApiRouteConfig,
    OpenApiDocument
};

pub(super) const OPEN_API_NOT_EXPOSED_WARN: &str = "OpenAPI configured but endpoints not exposed; call app.use_open_api() to serve spec/UI.";

impl App {
    /// Configures OpenAPI registry with custom settings.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_open_api(|config| config
    ///         .with_title("Example API")
    ///         .with_version("1.0.0"));
    /// ```
    pub fn with_open_api<T>(mut self, config: T) -> Self
    where
        T: FnOnce(OpenApiConfig) -> OpenApiConfig,
    {
        let config = config(self.openapi_config.unwrap_or_default());
        let registry = OpenApiRegistry::new(config.clone());

        self.openapi_config = Some(config);
        self.openapi = Some(registry);
        self
    }

    /// Sets OpenAPI registry with the provided configuration.
    pub fn set_open_api(mut self, config: OpenApiConfig) -> Self {
        self.openapi = Some(OpenApiRegistry::new(config.clone()));
        self.openapi_config = Some(config);
        self
    }

    /// Registers the OpenAPI JSON endpoint.
    pub fn use_open_api(&mut self) -> &mut Self {
        let (Some(registry), Some(config)) = (self.openapi.clone(), &mut self.openapi_config) else {
            panic!(
                "OpenAPI is not configured. Use `App::with_open_api` or `App::set_open_api` to configure it."
            );
        };
        
        config.exposed = true;
        
        let config = config.clone();
        let cache_control = create_spec_cache_control();
        for spec in registry.specs().to_vec() {
            let registry = registry.clone();
            let cache_control = cache_control.clone();
            
            self.map_get(&spec.spec_path, move || {
                let spec_name = spec.name.clone();
                let registry = registry.clone();
                let cache_control = cache_control.clone();
                
                async move {
                    let Some(doc) = registry.document_by_name(&spec_name) else {
                        return crate::status!(404);
                    };
                    
                    crate::ok!(doc; [cache_control])
                }
            });
        }

        if config.ui_enabled() {
            let html = ui_html(registry.specs(), config.title());
            let etag = create_etag(html.as_bytes());
            let cache_control = create_ui_cache_control();
            
            self.map_get(config.ui_path(), move |headers: HttpHeaders| {
                let etag = etag.clone();
                let cache_control = cache_control.clone();
                let html = html.clone();
                
                async move {
                    if crate::headers::helpers::validate_etag(&etag, &headers) {
                        return crate::status!(304; [Header::<ETag>::try_from(etag)?]);
                    }
                    
                    crate::html!(html; [
                        cache_control, 
                        Header::<ETag>::try_from(etag)?
                    ])
                }
            });
        }

        self
    }
}

fn create_spec_cache_control() -> Header<CacheControl> {
    Header::try_from(
        CacheControl::default()
            .with_public()
            .with_max_age(60)
            .with_stale_while_revalidate(600))
        .expect("invalid cache control header")
}

fn create_ui_cache_control() -> Header<CacheControl> {
    Header::try_from(
        CacheControl::default()
            .with_public()
            .with_max_age(3600)
            .with_stale_while_revalidate(86400))
        .expect("invalid cache control header")
}

fn create_etag(bytes: &[u8]) -> ETag {
    use sha1::{Sha1, Digest};
    
    let mut hasher = Sha1::new();
    hasher.update(bytes);

    let tag = format!("{:x}", hasher.finalize());
    ETag::weak(tag)
}
