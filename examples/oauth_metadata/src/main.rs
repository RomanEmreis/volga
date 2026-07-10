//! Serving OAuth 2.0 metadata documents (RFC 8414 / RFC 9728).
//!
//! Run with:
//!
//! ```no_rust
//! cargo run -p oauth_metadata
//! ```
//!
//! Then:
//!
//! ```no_rust
//! curl http://127.0.0.1:7878/.well-known/oauth-protected-resource
//! curl http://127.0.0.1:7878/.well-known/oauth-authorization-server
//! curl http://127.0.0.1:7878/.well-known/openid-configuration
//! curl -i http://127.0.0.1:7878/protected   # note the WWW-Authenticate header
//! ```

use serde::Deserialize;
use volga::{
    App,
    auth::{AuthClaims, DecodingKey, roles},
    ok,
};

#[derive(Clone, Deserialize)]
struct Claims {
    role: String,
}

impl AuthClaims for Claims {
    fn role(&self) -> Option<&str> {
        Some(&self.role)
    }
}

fn main() {
    let mut app = App::new()
        // The derived metadata URL is advertised automatically as
        // `resource_metadata` in WWW-Authenticate challenges (RFC 9728 §5.1)
        .with_bearer_auth(|auth| {
            auth.set_decoding_key(DecodingKey::from_secret(b"secret"))
                .require_https(false)
        })
        // Protected Resource Metadata (RFC 9728)
        .with_oauth_resource_metadata(|metadata| {
            metadata
                .with_resource("http://127.0.0.1:7878")
                .with_authorization_servers(["https://auth.example.com"])
                .with_scopes(["read", "write"])
                .with_bearer_methods(["header"])
        })
        // Authorization Server Metadata (RFC 8414) — only when the
        // application is also an authorization server; the identifier
        // string alone configures the minimal document. Both documents can
        // also come from the `[oauth.resource]`/`[oauth.server]` sections
        // of the config file (`config` feature).
        .set_oauth_server_metadata("http://127.0.0.1:7878");

    // GET /.well-known/oauth-protected-resource
    app.use_oauth_resource_metadata();

    // The same server document is commonly published at both discovery paths:
    // GET /.well-known/oauth-authorization-server
    // GET /.well-known/openid-configuration
    app.use_oauth_server_metadata().use_oidc_metadata();

    app.map_get("/protected", || async { ok!("protected") })
        .authorize::<Claims>(roles(["admin"]));

    app.run_blocking()
}
