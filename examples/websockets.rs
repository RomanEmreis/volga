use volga::{App, HttpRequest, HttpResult, error::Error, status};
use hyper::header::{UPGRADE, CONNECTION, SEC_WEBSOCKET_ACCEPT};
use hyper_util::rt::TokioIo;
use hyper::upgrade::Upgraded;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new()
        .with_websockets()
        .with_content_root("examples/ws");
    
    app.use_static_files();

    app.map_get("/ws", handle_ws);

    app.run().await
}

async fn handle_ws(req: HttpRequest) -> HttpResult {
    if !req.headers().contains_key(UPGRADE) {
        tracing::debug!("no UPGRADE header, responding with 400");
        return status!(404);
    }

    let mut req = req.into_inner();
    tokio::task::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgraded) => {
                if let Err(e) = server_upgraded_io(upgraded).await {
                    tracing::error!("server websocket io error: {}", e)
                };
            }
            Err(e) => tracing::error!("upgrade error: {}", e),
        }
    });
    
    status!(101, [
        (UPGRADE, "websocket"),
        (CONNECTION, "upgrade"),
        //(SEC_WEBSOCKET_ACCEPT, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
    ])
}

async fn server_upgraded_io(upgraded: Upgraded) -> Result<(), Error> {
    let mut upgraded = TokioIo::new(upgraded);

    let mut vec = Vec::new();
    upgraded.read(&mut vec).await?;

    tracing::debug!("server[websocket] recv: {:?}", std::str::from_utf8(&vec));

    upgraded.write_all(b"bar=foo").await?;

    tracing::debug!("server[websocket] sent");

    Ok(())
}