﻿use volga::{
    App, HttpContext, Next, HttpResult,
    headers::{Header, custom_headers},
    Inject, Dc, ok, not_found
};
use hyper::http::HeaderValue;
use uuid::Uuid;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex}
};

trait RequestIdGenerator: Send + Sync {
    fn generate_id(&self) -> String;
}

trait Cache: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: String, value: String);
}

#[derive(Clone, Default)]
struct InMemoryCache {
    inner: Arc<Mutex<HashMap<String, String>>>
}

#[derive(Clone, Default)]
struct RequestLog {
    inner: Arc<Mutex<Vec<String>>>
}

#[derive(Clone, Default)]
struct UuidGenerator;

custom_headers! {
    (RequestId, "x-req-id")
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    
    // Register a singleton service that is available globally 
    let global_cache = InMemoryCache::default();
    app.register_singleton(global_cache);

    // Register a scoped service that will be available during the request lifetime
    app.register_scoped::<RequestLog>();
    
    // Register a scoped service that will be available during the request lifetime
    app.register_transient::<UuidGenerator>();
    
    // Each request uses a simple scoped logger to write some logs ("Request started", "Requests ended")
    // Each request has an ID generated by RequestIdGenerator and passed further as a request and response header
    // When the request ends all the logs are stored in the cache with a key in this format: "log_<request_id>" and can be read by GET
    app.use_middleware(log_request::<UuidGenerator>);
    
    app.map_get("/{id}", get_value::<InMemoryCache>);
    app.map_post("/{id}/{value}", set_value::<InMemoryCache>);
    
    app.run().await
}

async fn log_request<T: RequestIdGenerator + Inject>(mut ctx: HttpContext, next: Next) -> HttpResult {
    let log: RequestLog = ctx.resolve()?;
    let id_gen: UuidGenerator = ctx.resolve()?;
    let cache: InMemoryCache = ctx.resolve()?;
    
    let req_id = id_gen.generate_id();
    ctx.request
        .headers_mut()
        .insert("x-req-id", HeaderValue::from_str(req_id.as_str()).unwrap());
    
    log.write(format!("Request: {req_id} started"));
    let response = next(ctx).await;
    log.write(format!("Request: {req_id} ended"));

    cache.set(
        format!("log_{req_id}"),
        log.to_string()
    );
    
    response
}

async fn get_value<T: Cache + Inject>(
    id: String,
    req_id: Header<RequestId>,
    cache: Dc<T>
) -> HttpResult {
    let item = cache.get(&id);
    match item { 
        Some(value) => ok!(value, [("x-req-id", req_id.to_string())]),
        None => not_found!([("x-req-id", req_id.to_string())])
    }
}

async fn set_value<T: Cache + Inject>(
    id: String, 
    value: String,
    req_id: Header<RequestId>,
    cache: Dc<T>
) -> HttpResult {
    cache.set(id, value);
    ok!([("x-req-id", req_id.to_string())])
}

impl Cache for InMemoryCache {
    fn get(&self, key: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .get(key)
            .cloned()
    }

    fn set(&self, key: String, value: String) {
        self.inner
            .lock()
            .unwrap()
            .insert(key, value);
    }
}

impl RequestLog {
    fn write(&self, log_entry: String) {
        self.inner
            .lock()
            .unwrap()
            .push(log_entry);
    }

    #[allow(clippy::inherent_to_string)]
    fn to_string(&self) -> String {
        self.inner.lock().unwrap().join("\n")
    }
}

impl RequestIdGenerator for UuidGenerator {
    fn generate_id(&self) -> String {
        Uuid::new_v4().to_string()
    }
}