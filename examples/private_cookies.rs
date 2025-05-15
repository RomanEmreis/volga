//! Run with:
//!
//! ```no_rust
//! cargo run --example private_cookies --features cookie,private-cookie,di
//! ```

use uuid::Uuid;
use volga::{
    App, HttpResult,
    error::Error,
    http::{PrivateKey, PrivateCookies},
    headers::{Header, Authorization},
    status, ok, see_other
};

async fn login(cookies: PrivateCookies, auth: Header<Authorization>) -> Result<(HttpResult, PrivateCookies), Error> {
    let session_id = authorize(auth)?;
    Ok((see_other!("/me"), cookies.add(("session-id", session_id))))
}

async fn me(cookies: PrivateCookies) -> HttpResult {
    if let Some(_session_id) = cookies.get("session-id") {
        ok!("Success")
    } else {
        status!(401, "Unauthorized")
    }
}

fn authorize(_auth: Header<Authorization>) -> Result<String, Error> {
    Ok(Uuid::new_v4().to_string())

    // authorize the user and create a session
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.add_singleton(PrivateKey::generate());

    app.map_post("/login", login);
    app.map_get("/me", me);

    app.run().await
}