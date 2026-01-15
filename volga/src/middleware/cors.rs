//! CORS (Cross-Origin Resource Sharing) Middleware
//!
//! Middleware that applies CORS headers for requests

use std::sync::Arc;
use hyper::Response;
use crate::{
    App,
    http::{StatusCode, HttpBody, Method},
    headers::{
        ACCESS_CONTROL_REQUEST_METHOD,
        ORIGIN,
    },
    HttpResponse
};
use crate::http::CorsConfig;

#[inline]
fn validate_cors_config(cors_config: &mut Option<CorsConfig>) -> CorsConfig {
    let Some(cors_config) = cors_config.take() else {
        panic!(
            "CORS error: Missing CORS configuration, you can configure it with `App::new().with_cors(|cors| cors...)`"
        );
    };

    cors_config.validate();

    cors_config
}

impl App {
    /// Adds CORS middleware to your web server's pipeline to allow cross-domain requests.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new()
    ///     .with_cors(|cors| cors
    ///         .with_any_origin()
    ///         .with_any_method()
    ///         .with_any_header());
    ///
    /// app.use_cors(); 
    /// ```
    ///
    /// # Panics
    /// If CORS hasn't been configured with [`App::set_cors`] or [`App::with_cors`]
    pub fn use_cors(&mut self) -> &mut Self {
        let cors_headers = Arc::new(
            validate_cors_config(&mut self.cors_config).precompute()
        );

        self.cors_enabled = true;

        self.wrap(move |ctx, next| {
            let cors_headers = cors_headers.clone();
            async move {
                let request = ctx.request();
                let method = request.method();
                let origin = request.headers().get(&ORIGIN).cloned();
                let acrm = request.headers()
                    .get(ACCESS_CONTROL_REQUEST_METHOD)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| Method::from_bytes(s.as_bytes()).ok());

                if method == Method::OPTIONS && origin.is_some() && acrm.is_some() {
                    let mut response = Response::new(HttpBody::empty());
                    
                    *response.status_mut() = StatusCode::NO_CONTENT;
                    cors_headers.apply_preflight_response(response.headers_mut(), origin);
            
                    Ok(HttpResponse::from_inner(response))
                } else {
                    let mut response = next(ctx).await?;
                    cors_headers.apply_normal_response(response.headers_mut(), origin);

                    Ok(response)
                }
            }
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::App;
    
    #[test]
    #[should_panic]
    fn it_panics_due_missing_cors_config() {
        let mut app = App::new();
        app.use_cors();
    }

    #[test]
    fn it_validates_cors_config() {
        let mut app = App::new()
            .with_cors(|cors| cors.with_credentials(false));
        app.use_cors();
    }
}