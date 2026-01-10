//! CORS (Cross-Origin Resource Sharing) Middleware
//!
//! Middleware that applies CORS headers for requests

use hyper::Response;
use crate::{App, http::{StatusCode, HttpBody, Method}, headers::{
    HeaderMap,
    HeaderValue,
    CONTENT_LENGTH,
    ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_CREDENTIALS,
    ACCESS_CONTROL_MAX_AGE,
    ACCESS_CONTROL_EXPOSE_HEADERS,
    ORIGIN,
    VARY
}, HttpResponse};
use crate::http::CorsConfig;

fn validate_cors_config(cors_config: &Option<CorsConfig>) {
    assert!(
        cors_config.is_some(), 
        "CORS error: Missing CORS configuration, you can configure it with `App::new().with_cors(|cors| cors...)`"
    );
    
    if let Some(ref cors_config) = *cors_config {
        cors_config.validate()
    }
}

impl App {
    /// Adds a CORS middleware to your web server's pipeline to allow cross domain requests.
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
    pub fn use_cors(&mut self) -> &mut Self {
        validate_cors_config(&self.cors_config);

        let cors_config = self.cors_config.clone().unwrap();
        self.wrap(move |ctx, next| {
            let cors_config = cors_config.clone();
            async move {
                let origin = ctx.request().headers().get(&ORIGIN);
                let method = ctx.request().method();

                let mut headers = HeaderMap::new();

                if let Some(allow_credentials) = cors_config.allow_credentials() {
                    headers.insert(ACCESS_CONTROL_ALLOW_CREDENTIALS, allow_credentials);
                }
                if let Some(vary_header) = cors_config.vary_header() {
                    headers.insert(VARY, vary_header);
                }
                if let Some(allow_origin) = cors_config.allow_origin(origin) {
                    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, allow_origin);
                }

                if method == Method::OPTIONS {
                    if let Some(allow_methods) = cors_config.allow_methods() {
                        headers.insert(ACCESS_CONTROL_ALLOW_METHODS, allow_methods);
                    }
                    if let Some(allow_headers) = cors_config.allow_headers() {
                        headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, allow_headers);
                    }
                    if let Some(max_age) = cors_config.max_age() {
                        headers.insert(ACCESS_CONTROL_MAX_AGE, max_age);
                    };

                    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));
                    
                    let mut response = Response::new(HttpBody::empty());
                    
                    *response.status_mut() = StatusCode::NO_CONTENT;
                    *response.headers_mut() = headers;
                    
                    Ok(HttpResponse::from_inner(response))
                } else {
                    if let Some(expose_headers) = cors_config.expose_headers() {
                        headers.insert(ACCESS_CONTROL_EXPOSE_HEADERS, expose_headers);
                    }

                    let response = next(ctx).await;
                    match response {
                        Err(err) => Err(err),
                        Ok(mut response) => {
                            let response_headers = response.headers_mut();
                            if let Some(vary_header) = headers.remove(&VARY) {
                                response_headers.append(VARY, vary_header);
                            }
                            response_headers.extend(headers.drain());

                            Ok(response)
                        }
                    }
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