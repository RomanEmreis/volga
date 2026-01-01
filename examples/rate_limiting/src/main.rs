//! Run with:
//!
//! ```no_rust
//! JWT_SECRET=secret cargo run -p rate_limiting
//! ```

use std::time::Duration;
use serde::Deserialize;
use volga::{
    auth::{Claims, roles, DecodingKey}, 
    rate_limiting::{FixedWindow, SlidingWindow, RateLimitKeyExt, by},
    App
};

fn main() {
    let secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set");
    
    let mut app = App::new()
        .with_bearer_auth(|auth| auth
            .validate_exp(false)
            .set_decoding_key(DecodingKey::from_secret(secret.as_bytes())))
        .with_fixed_window(FixedWindow::new(100, Duration::from_secs(30)))
        .with_sliding_window(SlidingWindow::new(100, Duration::from_secs(15)))
        .with_sliding_window(SlidingWindow::new(100, Duration::from_secs(30))
            .with_name("burst"));

    app.use_fixed_window(by::ip());
    
    app.map_get("/fixed", async || "Hello from fixed window!")
        .sliding_window(by::header("x-api-key"));

    app.map_get("/sliding", async || "Hello from sliding window!")
        .authorize::<Claims>(roles(["admin", "user"]))
        .sliding_window(by::user(|c: &Claims| c.sub.as_str()).using("burst"));

    app.run_blocking();
}

#[derive(Claims, Clone, Deserialize)]
struct Claims {
    sub: String,
    role: String,
}