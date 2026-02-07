//! Run with:
//!
//! ```no_rust
//! cargo run -p open_api
//! ```

use volga::{
    App, 
    Json, 
    Path, 
    NamedPath, 
    Form, 
    Query, 
    File, 
    Multipart,
    http::sse::Message,
    ok, 
    sse_stream
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .with_open_api(|config| config
            .with_title("Open API Demo")
            .with_description("Demonstration of Open API with Volga")
            .with_version("1.0.0")
            .with_ui());

    app.use_open_api();

    app.map_get("/hello", async || "Hello, World!");

    app.map_get("/{name}", async |name: String| ok!(fmt: "Hello {name}"));
    app.map_get("/path/{name}/{age}", async |Path((_name, _age)): Path<(String, u32)>| {});
    app.map_get("/named-path/{name}/{age}", async |path: NamedPath<Payload>| ok!(Payload {
        name: path.name.clone(),
        age: path.age
    }))
        .open_api(|config| config.produces_json::<Payload>());
    
    app.map_get("/query", async |q: Query<Payload>| ok!(fmt: "Hello {}", q.name));
    
    app.map_put("/form", async |payload: Form<Payload>| payload)
        .open_api(|config| config
            .produces_form::<Payload>());
    
    app.map_post("/json", async |payload: Json<Payload>| payload)
        .open_api(|config| config
            .produces_json_example(Payload {
                name: "John".into(),
                age: 30
            })
        );
    
    app.map_post("/file", async |file: File| file.into_byte_stream());
    app.map_post("/multipart", async |mut files: Multipart| sse_stream! {
        while let Ok(Some(field)) = files.next_field().await {
            yield Message::new().data(field.name().unwrap());
        }
    });

    app.run().await
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Payload {
    name: String,
    age: u64
}