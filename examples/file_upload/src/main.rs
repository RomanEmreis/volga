//! Run with:
//!
//! ```no_rust
//! cargo run -p file_upload
//! ```

use volga::{App, File};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_post("/upload", |file: File| async move {
        file.save_as("examples/files/upload_test.txt").await
    });

    app.run().await
}