//! Run with:
//!
//! ```no_rust
//! cargo run -p multipart
//! ```

use std::path::Path;
use volga::{App, Multipart, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_post("/upload", |files: Multipart| async move {
        files.save_all("examples/multipart/files").await
    });

    app.map_post("/manual-upload", |mut files: Multipart| async move {
        let path = Path::new("examples/multipart/files");
        while let Some(field) = files.next_field().await? {
            field.save(path).await?;
        }
        ok!("Files have been uploaded!")
    });

    app.map_post("/multipart", |mut multipart: Multipart| async move {
        let mut results = Vec::new();
        while let Some(field) = multipart.next_field().await? {
            let text = field.text().await?;
            results.push(text);
        }
        ok!(results)
    });

    app.run().await
}