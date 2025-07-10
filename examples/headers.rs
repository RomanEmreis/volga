//! Run with:
//!
//! ```no_rust
//! cargo run --example headers
//! ```

use volga::{App, ResponseContext, headers, ok};
use volga::headers::{
    Header,
    HttpHeaders,
    Accept
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Read request headers with Headers
    app.map_get("/api-key", |headers: HttpHeaders| async move { 
        let api_key = headers.get("x-api-key")
            .unwrap()
            .to_str()
            .unwrap();
        ok!(api_key)
    });
    
    // Reading header with Header<T>
    app.map_get("/accept", |accept: Header<Accept>| async move { 
        ok!("{accept}")
    });
    
    // Respond with headers
    app.map_get("/hello", || async {
        ok!("Hello World!", [
           ("x-api-key", "some api key"),
           ("Content-Type", "text/plain")
       ])
    });

    // Respond with headers using headers! macro
    app.map_get("/hello-again", || async {
        let headers = headers![
            ("x-api-key", "some api key"),
            ("Content-Type", "text/plain")
        ];

        ResponseContext {
            content: "Hello World!",
            status: 200,
            headers
        }
    });

    app.run().await
}