use std::collections::HashMap;
use std::sync::OnceLock;

use hyper::{Method, Uri};

use crate::headers::{ContentType, Header, HttpHeaders};
use crate::http::endpoints::Endpoints;
use crate::http::endpoints::args::{FromPayload, Payload};
use crate::http::endpoints::handlers::Func;
use crate::{App, Form, HttpBody, Json, Query, ok};

fn build_router() -> Endpoints {
    let mut endpoints = Endpoints::new();
    endpoints.map_route(Method::GET, "/", Func::new(|| async { ok!() }));
    endpoints.map_route(Method::GET, "/a", Func::new(|| async { ok!() }));
    endpoints.map_route(Method::GET, "/a/{id}", Func::new(|| async { ok!() }));
    endpoints.map_route(
        Method::POST,
        "/a/{id}/b/{slug}",
        Func::new(|| async { ok!() }),
    );
    endpoints.map_route(
        Method::DELETE,
        "/files/{name}",
        Func::new(|| async { ok!() }),
    );
    endpoints
}

pub fn fuzz_router_match(method: Method, path: &str, host: Option<&str>) {
    static ROUTER: OnceLock<Endpoints> = OnceLock::new();
    let endpoints = ROUTER.get_or_init(build_router);

    let uri_str = match host {
        Some(h) if !h.is_empty() => format!("http://{h}{path}"),
        _ => path.to_owned(),
    };

    if let Ok(uri) = uri_str.parse::<Uri>() {
        let _ = endpoints.find(
            &method,
            &uri,
            #[cfg(feature = "middleware")]
            false,
            #[cfg(feature = "middleware")]
            &crate::headers::HeaderMap::new(),
        );
    }
}

pub fn fuzz_query_decode(query: &str) {
    let _: Result<HashMap<String, String>, _> = serde_urlencoded::from_str(query);
    let _: Result<Query<HashMap<String, String>>, _> = Query::try_from(query);
}

pub fn fuzz_extractor_typed(headers: &[(String, String)], body: &[u8]) {
    // Build a HeaderMap from the fuzzed headers, skipping any with invalid names or values.
    let mut header_map = hyper::HeaderMap::new();
    for (k, v) in headers.iter().take(16) {
        if let (Ok(name), Ok(value)) = (
            k.parse::<hyper::header::HeaderName>(),
            v.parse::<hyper::header::HeaderValue>(),
        ) {
            header_map.insert(name, value);
        }
    }
    let (mut parts, _) = hyper::Request::new(()).into_parts();
    parts.headers = header_map;

    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let rt = RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("fuzz runtime")
    });

    rt.block_on(async {
        // Header extractors (Source::Parts).
        let _ = HttpHeaders::from_payload(Payload::Parts(&parts)).await;
        let _ = Header::<ContentType>::from_payload(Payload::Parts(&parts)).await;

        // Body extractors (Source::Body).
        let _ =
            Json::<serde_json::Value>::from_payload(Payload::Body(HttpBody::full(body.to_vec())))
                .await;
        let _ = Form::<HashMap<String, String>>::from_payload(Payload::Body(HttpBody::full(
            body.to_vec(),
        )))
        .await;
    });
}

pub fn fuzz_openapi_gen(selector: u16) {
    #[cfg(feature = "openapi")]
    {
        let mut app = App::new().with_open_api(|cfg| cfg.with_title("Fuzz").with_version("1.0.0"));
        app.map_get("/users/{id}", |_id: i32| async { ok!() });
        app.map_post("/users", |_body: Json<serde_json::Value>| async { ok!() });
        app.map_get("/search", |_query: Query<HashMap<String, String>>| async {
            ok!()
        });

        match selector % 3 {
            0 => {
                app.use_open_api();
            }
            1 => {
                app.use_open_api().map_get("/ping", || async { ok!() });
            }
            _ => {}
        }

        if let Some(registry) = app.openapi.registry.clone() {
            for spec in registry.specs() {
                if let Some(doc) = registry.document_by_name(&spec.name) {
                    let _ = serde_json::to_vec(&doc);
                }
            }
        }
    }
}
