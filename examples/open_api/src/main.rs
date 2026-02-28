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
            .with_specs(["v1", "v2"])
            .with_ui());

    app.use_open_api();

    app.group("/path", |api| {
        api.open_api(|cfg| cfg.with_docs(["v1", "v2"]));
        
        api.map_get("/hello", async || "Hello, World!");
        api.map_get("/{name}", async |name: String| ok!(fmt: "Hello {name}"));
        api.map_get("/{name}/{age:integer}", async |Path((_name, _age)): Path<(String, u32)>| {});
        api.map_get("/named/{name}/{age}", async |path: NamedPath<Payload>| ok!(path.into_inner()))
            .open_api(|cfg| cfg.produces_json::<Payload>(200u16));
    });
    
    app.group("/file", |api| {
        api.open_api(|cfg| cfg.with_docs(["v1", "v2"]));
        
        api.map_post("/file", async |file: File| file.into_byte_stream());
        api.map_post("/multipart", async |mut files: Multipart| sse_stream! {
            while let Ok(Some(field)) = files.next_field().await {
                yield Message::new().data(field.name().unwrap());
            }
        });
    });
    
    app.map_head("/head", async || {})
        .open_api(|cfg| cfg.with_docs(["v1"]));

    app.map_get("/query", async |q: Query<Payload>| ok!(fmt: "Hello {}", q.name))
        .open_api(|cfg| cfg.with_docs(["v1", "v2"]));
    
    app.map_put("/form", async |payload: Form<Payload>| payload)
        .open_api(|cfg| cfg
            .with_doc("v2")
            .produces_form::<Payload>(200u16));

    app.map_post("/json", async |payload: Json<Payload>| payload)
        .open_api(|cfg| cfg
            .with_doc("v2")
            .produces_json_example(200u16, Payload { name: "John".into(), age: 30 }));

    app.run().await
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
struct Payload {
    name: String,
    age: u64
}