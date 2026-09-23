# Volga Open API Integration

OpenAPI 3.0 integration for the **Volga** web framework.

`volga-open-api` generates OpenAPI specifications directly from your routes, extractors and responders - without macros, codegen or runtime reflection.

It is fully optional and designed to stay out of your way.

## Features

* OpenAPI 3.0 spec generation
* Multiple specs (e.g. `v1`, `v2`)
* Swagger UI integration
* Automatic schema inference
* Support for request/response examples
* Per-route and per-group metadata
* Zero required macros
* Zero runtime reflection
* Fully optional via feature flag

## Philosophy

`volga-open-api` follows the same principles as Volga:

* No hidden magic
* No global registries
* No derive macros required
* No reflection
* Explicit > implicit
* Composable configuration

OpenAPI is generated from actual route definitions and extractor/response types.

If you don't expose the spec endpoints, nothing is publicly served.

## Installation

Add the feature to your `Cargo.toml`:

```toml
volga = { version = "...", features = ["openapi"] }
```

Or use the standalone crate:

```toml
volga-open-api = "..."
```

## Basic Usage

```rust
use volga::App;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .with_open_api(|config| config
            .with_title("Example API")
            .with_version("1.0.0")
            .with_ui());

    app.use_open_api();

    app.map_get("/hello", async || "Hello, World!");

    app.run().await
}
```

Spec will be available at:

```
/openapi.json
/openapi
```

## Multiple Specs (API Versioning)

```rust
app.with_open_api(|config| config
    .with_specs(["v1", "v2"])
    .with_ui())
```

Bind routes to specific documents:

```rust
app.map_post("/users", handler)
    .open_api(|cfg| cfg.with_doc("v2"));
```

Routes default to the first spec unless explicitly assigned.

## Groups

Route groups automatically inherit OpenAPI configuration:

```rust
app.group("/users", |api| {
    api.open_api(|cfg| cfg.with_doc("v2"));

    api.map_get("/", list_users);
    api.map_post("/", create_user);
});
```

Group metadata merges with route-level metadata.

## Request / Response Schema Generation

### JSON

```rust
app.map_post("/json", async |payload: Json<User>| payload)
    .open_api(|cfg| cfg.produces_json::<User>());
```

### Form

```rust
app.map_put("/form", async |payload: Form<User>| payload)
    .open_api(|cfg| cfg.produces_form::<User>());
```

### Examples

```rust
app.map_post("/json", async |payload: Json<User>| payload)
    .open_api(|cfg| cfg.produces_json_example(User {
        name: "John".into(),
        age: 30,
    }));
```

## Multipart / Streams / SSE

```rust
cfg.consumes_multipart();
cfg.produces_sse();
cfg.produces_stream();
```

Binary streams use:

```
type: string
format: binary
```

## Automatic Schema Inference

For `Deserialize` types, schemas and examples are inferred automatically.

For `Serialize`-only types, response schema must be specified explicitly.

### Types serde reads as a map

serde reads a struct with a `#[serde(flatten)]` field as a map, so that it can collect the keys the struct does not name for the flattened member - and a map does not say which keys it takes. None of such a struct's fields can be inferred, including the ones declared beside the flattened member: a request body is described as an object without properties (or as any value, when the struct sits inside it, as in a `Vec`), and query parameters are not described at all. Debug builds name each such input at startup.

Describe them by hand instead:

```rust
use volga::openapi::OpenApiSchema;

let schema = OpenApiSchema::object()
    .with_property("name", OpenApiSchema::string())
    .with_property("page", OpenApiSchema::integer())
    .with_required(["name", "page"]);

app.map_post("/search", search)
    .open_api(|cfg| cfg.with_request_schema(schema.clone()));

app.map_get("/search", search_by_query)
    .open_api(|cfg| cfg.with_query_schema(schema));
```

Path parameters are named by the route template whatever the extractor describes; spell their types there - `{id:integer}`.

## Caching

Swagger UI is served with:

* `ETag`
* `Cache-Control`
* `stale-while-revalidate`

Spec JSON endpoints are cache-friendly.

## Security

OpenAPI endpoints are not exposed unless `use_open_api()` is called.

You can:

* Expose only JSON spec
* Serve UI behind auth
* Export spec without exposing endpoints

## Roadmap

* Optional schema deduplication improvements
* Better enum support
* Component reuse optimizations
* Optional Redoc support
* Optional spec export to file

## Why Not Macros?

`volga-open-api` intentionally avoids:

* `#[derive(OpenApi)]`
* Proc macros
* Reflection
* Code generation

Schemas are built using real `serde` behavior and actual route definitions.

## License
Volga is licensed under the MIT License. Contributions welcome!
