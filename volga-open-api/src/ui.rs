//! Helpers for OpenAPI UI.

use super::config::{OpenApiSpec, DEFAULT_SPEC_PATH};

const SWAGGER_UI_CDN_BASE: &str = "https://cdn.jsdelivr.net/npm/swagger-ui-dist@5.31.2";

/// Generates OpenAPI UI HTML.
pub fn ui_html(specs: &[OpenApiSpec], ui_title: &str) -> String {
    let config_js = if specs.len() <= 1 {
        let url = specs
            .first()
            .map(|s| s.spec_path.as_str())
            .unwrap_or(DEFAULT_SPEC_PATH);

        format!(r#"url: {},"#, js_string(url))
    } else {
        // urls: [{url, name}, ...] + primaryName
        let urls = specs.iter().map(|s| {
            format!(
                r#"{{ url: {}, name: {} }}"#,
                js_string(&s.spec_path),
                js_string(&s.name),
            )
        }).collect::<Vec<_>>().join(",\n          ");

        let primary = js_string(&specs[0].name);

        format!(r#"urls: [
            {urls}
        ],
        "urls.primaryName": {primary},"#)
    };

    format!(
        r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{}</title>
    <link rel="stylesheet" href="{}/swagger-ui.css" crossorigin="anonymous" referrerpolicy="no-referrer" />
  </head>
  <body>
    <div id="swagger-ui"></div>

    <script src="{}/swagger-ui-bundle.js" crossorigin="anonymous" referrerpolicy="no-referrer"></script>
    <script src="{}/swagger-ui-standalone-preset.js" crossorigin="anonymous" referrerpolicy="no-referrer"></script>

    <script>
      window.onload = function() {{
        SwaggerUIBundle({{
          {config_js}
          dom_id: "#swagger-ui",
          presets: [
            SwaggerUIBundle.presets.apis,
            SwaggerUIStandalonePreset
          ],
          layout: "StandaloneLayout"
        }});
      }};
    </script>
  </body>
</html>"##,
        html_escape(ui_title),
        SWAGGER_UI_CDN_BASE,
        SWAGGER_UI_CDN_BASE,
        SWAGGER_UI_CDN_BASE,
    )
}

#[inline]
fn js_string(s: &str) -> String {
    // returns a valid JS string literal: "\"...\"" with escapes
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

#[inline]
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{SWAGGER_UI_CDN_BASE, ui_html};
    use crate::config::OpenApiSpec;

    #[test]
    fn ui_html_uses_single_url_config_for_one_spec() {
        let html = ui_html(&[OpenApiSpec::new("v1")], "Docs");

        assert!(html.contains("url: \"/v1/openapi.json\""));
        assert!(!html.contains("urls.primaryName"));
        assert!(html.contains("<title>Docs</title>"));
        assert!(html.contains(SWAGGER_UI_CDN_BASE));
        assert!(html.contains("crossorigin=\"anonymous\""));
        assert!(html.contains("referrerpolicy=\"no-referrer\""));
    }

    #[test]
    fn ui_html_uses_urls_config_for_multiple_specs() {
        let html = ui_html(&[OpenApiSpec::new("v1"), OpenApiSpec::new("admin")], "Docs");

        assert!(html.contains("urls: ["));
        assert!(html.contains("{ url: \"/v1/openapi.json\", name: \"v1\" }"));
        assert!(html.contains("{ url: \"/admin/openapi.json\", name: \"admin\" }"));
        assert!(html.contains("\"urls.primaryName\": \"v1\""));
    }

    #[test]
    fn ui_html_falls_back_to_default_path_for_empty_specs() {
        let html = ui_html(&[], "Docs");
        assert!(html.contains("url: \"/openapi.json\""));
    }

    #[test]
    fn ui_html_escapes_title_html() {
        let html = ui_html(&[OpenApiSpec::new("v1")], r#"</title><script>alert(1)</script>"#);

        assert!(html.contains("<title>&lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt;</title>"));
        assert!(!html.contains("</title><script>alert(1)</script>"));
    }

    #[test]
    fn ui_html_pins_exact_swagger_ui_version() {
        let html = ui_html(&[OpenApiSpec::new("v1")], "Docs");

        assert!(html.contains("/npm/swagger-ui-dist@5.31.2/swagger-ui.css"));
        assert!(html.contains("/npm/swagger-ui-dist@5.31.2/swagger-ui-bundle.js"));
        assert!(html.contains("/npm/swagger-ui-dist@5.31.2/swagger-ui-standalone-preset.js"));
        assert!(!html.contains("unpkg.com/swagger-ui-dist@5/"));
    }
}
