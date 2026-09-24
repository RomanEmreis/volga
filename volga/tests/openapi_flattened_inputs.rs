#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "openapi"))]

use serde::Deserialize;
use serde_json::{Value, json};
use volga::openapi::OpenApiSchema;
use volga::{Json, Query, ok, test::TestServer};

#[derive(Deserialize)]
struct Paging {
    cursor: String,
}

/// Read by serde as a map, because of the flattened field - so none of its fields, `name`
/// included, can be inferred
#[derive(Deserialize)]
struct Search {
    #[serde(flatten)]
    paging: Paging,
    name: String,
}

fn search_schema() -> OpenApiSchema {
    OpenApiSchema::object()
        .with_property("cursor", OpenApiSchema::string())
        .with_property("name", OpenApiSchema::string())
        .with_required(["cursor", "name"])
}

async fn spec(server: &TestServer) -> Value {
    server
        .client()
        .get(server.url("/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn it_describes_a_flattened_body_as_an_object_without_an_example() {
    let server = TestServer::builder()
        .configure(|app| app.with_open_api(|config| config))
        .setup(|app| {
            app.map_post("/search", async |search: Json<Search>| {
                ok!("{} {}", search.name, search.paging.cursor)
            });
            app.use_open_api();
        })
        .build()
        .await;

    let spec = spec(&server).await;
    let body = &spec["paths"]["/search"]["post"]["requestBody"]["content"]["application/json"];

    assert_eq!(
        body["schema"],
        json!({ "type": "object", "title": "Search" })
    );
    assert!(body.get("example").is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_publishes_a_flattened_input_described_by_hand() {
    let server = TestServer::builder()
        .configure(|app| app.with_open_api(|config| config))
        .setup(|app| {
            app.map_post("/search", async |search: Json<Search>| {
                ok!("{} {}", search.name, search.paging.cursor)
            })
            .open_api(|cfg| cfg.with_request_schema(search_schema()));

            app.map_get("/search", async |search: Query<Search>| {
                ok!("{} {}", search.name, search.paging.cursor)
            })
            .open_api(|cfg| cfg.with_query_schema(search_schema()));

            app.use_open_api();
        })
        .build()
        .await;

    let spec = spec(&server).await;

    let body = &spec["paths"]["/search"]["post"]["requestBody"]["content"]["application/json"];
    assert_eq!(body["schema"]["properties"]["cursor"]["type"], "string");
    assert_eq!(body["schema"]["properties"]["name"]["type"], "string");

    let parameters = spec["paths"]["/search"]["get"]["parameters"]
        .as_array()
        .expect("query parameters")
        .iter()
        .map(|p| (p["name"].clone(), p["in"].clone(), p["required"].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        parameters,
        [
            (json!("cursor"), json!("query"), json!(true)),
            (json!("name"), json!("query"), json!(true)),
        ]
    );

    // The type itself is read as it always was
    let res = server
        .client()
        .get(server.url("/search?cursor=abc&name=volga"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.text().await.unwrap(), "volga abc");

    server.shutdown().await;
}
