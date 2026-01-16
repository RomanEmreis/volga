//! CORS (Cross-Origin Resource Sharing) Middleware
//!
//! Middleware that applies CORS headers for requests

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
        if !self.cors.registered() {
            panic!(
                "CORS error: Missing CORS configuration, you can configure it with `App::new().with_cors(|cors| cors...)`"
            );
        }

        self.cors.is_enabled = true;
        
        let default_cors = self.cors.get_default().cloned();

        self.wrap(move |ctx, next| {
            let default_cors = default_cors.clone();
            async move {
                // Resolve effective policy (Route > Group > Default)
                let Some(cors) = ctx.resolve_cors(default_cors.as_ref()) else {
                    return next(ctx).await;
                };

                let request = ctx.request();
                let method = request.method();

                // Preflight is only possible for OPTIONS
                if method == Method::OPTIONS {
                    let origin = request.headers().get(&ORIGIN);
                    let acrm = request.headers()
                        .get(ACCESS_CONTROL_REQUEST_METHOD)
                        .and_then(|v| Method::from_bytes(v.as_bytes()).ok());

                    if origin.is_some() && acrm.is_some() {
                        let mut response = Response::new(HttpBody::empty());
                        *response.status_mut() = StatusCode::NO_CONTENT;

                        cors.apply_preflight_response(response.headers_mut(), origin.cloned());

                        return Ok(HttpResponse::from_inner(response))
                    }

                    // Not a valid preflight => fall through to user OPTIONS handler
                }

                let origin = request.headers().get(&ORIGIN).cloned();
                let mut response = next(ctx).await?;
                cors.apply_normal_response(response.headers_mut(), origin);

                Ok(response)
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