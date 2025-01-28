use crate::{App, error::call_weak_err_handler};
use futures_util::TryFutureExt;
use tracing::{Instrument, trace_span};

const DEFAULT_SPAN_HEADER_NAME: &str = "request-id";

/// Represents a tracing configuration
#[derive(Clone)]
pub struct TracingConfig {
    /// Specifies whether include a span id HTTP header
    /// 
    /// Default: `false`
    include_header: bool,
    
    /// Specifies a span id HTTP header name
    /// 
    /// Default: `request-id`
    span_header_name: &'static str,
}

impl Default for TracingConfig {
    #[inline]
    fn default() -> Self {
        Self { 
            include_header: false,
            span_header_name: DEFAULT_SPAN_HEADER_NAME,
        }
    }
}

impl TracingConfig {
    /// Creates a default tracing configuration
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Configures tracing to include the span id as HTTP header
    /// 
    /// Default: `false`
    pub fn with_header(mut self) -> Self {
        self.include_header = true;
        self
    }

    /// Configures tracing to use a specific HTTP header name if the `include_header` is set to `true`
    ///
    /// Default: `request-id`
    pub fn with_header_name(mut self, name: &'static str) -> Self {
        self.span_header_name = name;
        self
    }
}

impl App {
    /// Configures web server with specifies Tracing configurations
    ///
    /// Defaults:
    /// - include_header: `false`
    /// - span_header_name: `request-id`
    pub fn with_tracing(mut self, config: TracingConfig) -> Self {
        self.tracing_config = Some(config);
        self
    }
    
    /// Adds middleware for wrapping each request into unique [`tracing::Span`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, tracing::TracingConfig};
    ///
    /// let mut app = App::new();
    /// // is equivalent to:
    /// let mut app = App::new().with_tracing(TracingConfig::default());
    ///
    /// // if the tracing middleware is enabled
    /// app.use_tracing(); 
    /// ```
    pub fn use_tracing(&mut self) -> &mut Self {
        let tracing_config = self.tracing_config
            .take()
            .unwrap_or_default();
        
        self.use_middleware(move |ctx, next| {
            let tracing_config = tracing_config.clone();
            async move {
                let method = ctx.request.method();
                let path = ctx.request.uri();
                
                let span = trace_span!("request", %method, %path);
                let span_id = span.id();
                let error_handler = ctx.error_handler.clone();
                
                let http_result = next(ctx)
                    .or_else(|err| async { call_weak_err_handler(error_handler, err).await })
                    .instrument(span)
                    .await;

                if tracing_config.include_header && span_id.is_some() {
                    http_result.map(|mut response| {
                        response.headers_mut().append(
                            tracing_config.span_header_name,
                            span_id.map_or(0, |id| id.into_u64()).into());
                        response
                    })
                } else { 
                    http_result
                } 
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{TracingConfig, DEFAULT_SPAN_HEADER_NAME};
    
    #[test]
    fn it_creates_new() {
        let tracing_config = TracingConfig::new();
        
        assert!(!tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_default() {
        let tracing_config = TracingConfig::default();

        assert!(!tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_with_include_header() {
        let tracing_config = TracingConfig::new().with_header();

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, DEFAULT_SPAN_HEADER_NAME);
    }

    #[test]
    fn it_creates_with_header_name() {
        let tracing_config = TracingConfig::new()
            .with_header()
            .with_header_name("correlation-id");

        assert!(tracing_config.include_header);
        assert_eq!(tracing_config.span_header_name, "correlation-id");
    }
}