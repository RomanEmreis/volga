﻿use tokio::fs::File;
use volga::{
    App,
    file,
    EndpointsMapping
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    
    app.map_get("/download", || async {
        let file = File::open("examples/files/download.txt").await?;

        file!("download.txt", file)
    });
    
    app.run().await
}