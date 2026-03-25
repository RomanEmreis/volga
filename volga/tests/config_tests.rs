#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "config"))]

use serde::Deserialize;
use std::io::Write;
use volga::test::TestServer;
use volga::{App, Config, ok};

#[derive(Debug, Deserialize)]
struct Database {
    url: String,
}

#[derive(Debug, Deserialize)]
struct Cache {
    ttl: u64,
}

fn write_temp_toml(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

fn write_temp_json(content: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::with_suffix(".json").unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f
}

#[tokio::test]
async fn config_extractor_reads_toml_section() {
    let file = write_temp_toml("[database]\nurl = \"postgres://localhost/mydb\"\n");
    let path = file.path().to_str().unwrap().to_owned();

    let server = TestServer::builder()
        .configure(move |app: App| {
            app.with_config(|cfg| cfg.from_file(&path).bind_section::<Database>("database"))
        })
        .setup(|app| {
            app.map_get(
                "/db",
                |db: Config<Database>| async move { ok!("{}", db.url) },
            );
        })
        .build()
        .await;

    let res = server.client().get(server.url("/db")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "postgres://localhost/mydb");

    server.shutdown().await;
    drop(file); // keep temp file alive until after the test
}

#[tokio::test]
async fn config_extractor_reads_json_section() {
    let file = write_temp_json(r#"{"database": {"url": "mysql://localhost/test"}}"#);
    let path = file.path().to_str().unwrap().to_owned();

    let server = TestServer::builder()
        .configure(move |app: App| {
            app.with_config(|cfg| cfg.from_file(&path).bind_section::<Database>("database"))
        })
        .setup(|app| {
            app.map_get(
                "/db",
                |db: Config<Database>| async move { ok!("{}", db.url) },
            );
        })
        .build()
        .await;

    let res = server.client().get(server.url("/db")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "mysql://localhost/test");

    server.shutdown().await;
    drop(file);
}

#[tokio::test]
async fn optional_config_absent_section_returns_none() {
    let file = write_temp_toml(""); // no [cache] section
    let path = file.path().to_str().unwrap().to_owned();

    let server = TestServer::builder()
        .configure(move |app: App| {
            app.with_config(|cfg| cfg.from_file(&path).bind_section_optional::<Cache>("cache"))
        })
        .setup(|app| {
            app.map_get("/cache", |cache: Option<Config<Cache>>| async move {
                if cache.is_none() {
                    ok!("none")
                } else {
                    ok!("some")
                }
            });
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/cache"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.text().await.unwrap(), "none");

    server.shutdown().await;
    drop(file);
}

#[tokio::test]
async fn optional_config_present_section_returns_some() {
    let file = write_temp_toml("[cache]\nttl = 300\n");
    let path = file.path().to_str().unwrap().to_owned();

    let server = TestServer::builder()
        .configure(move |app: App| {
            app.with_config(|cfg| cfg.from_file(&path).bind_section_optional::<Cache>("cache"))
        })
        .setup(|app| {
            app.map_get("/cache", |cache: Option<Config<Cache>>| async move {
                match cache {
                    Some(c) => ok!("{}", c.ttl),
                    None => ok!("none"),
                }
            });
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/cache"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.text().await.unwrap(), "300");

    server.shutdown().await;
    drop(file);
}

#[tokio::test]
async fn invalid_server_section_causes_startup_error() {
    let file = write_temp_toml("[server]\nport = \"not_a_number\"\n");
    let path = file.path().to_str().unwrap().to_owned();

    // `run()` calls `process_config()` internally; an invalid built-in section must
    // return an Err instead of silently using defaults.
    let result = App::new()
        .bind("127.0.0.1:0")
        .with_config(|cfg| cfg.from_file(&path))
        .run()
        .await;

    assert!(
        result.is_err(),
        "expected startup error for invalid [server] section"
    );
    drop(file);
}
