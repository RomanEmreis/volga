use volga::{App, ok, Json};

#[derive(serde::Serialize, serde::Deserialize)]
struct User {
    name: String,
    age: i32,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.use_decompression();

    app.map_post("/decompress", |Json(users): Json<Vec<User>>| async move {
        ok!(users)
    });

    app.run().await
}