#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "tls"))]

use std::{sync::Once, time::Duration};
use volga::http::StatusCode;
use volga::headers::{STRICT_TRANSPORT_SECURITY, LOCATION};
use volga::tls::TlsConfig;
use volga::test::TestServer;
use reqwest::{Certificate, Identity, redirect::Policy};

static INIT: Once = Once::new();

fn init_crypto() {
    INIT.call_once(|| {
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install crypto provider");
    });
}

#[tokio::test]
async fn it_works_with_tls_with_no_auth() {
    init_crypto();
        
    let server = TestServer::builder()
        .with_https()
        .configure(|app| app
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key")))
        .setup(|app| {
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();
    
    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| app
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_required_client_auth("tests/tls/ca.pem")))
        .setup(|app| {
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_unauthenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| app
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_required_client_auth("tests/tls/ca.pem")))
        .setup(|app| {
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await;

    assert!(response.is_err());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_authenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| app
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_optional_client_auth("tests/tls/ca.pem")))
        .setup(|app| {
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_unauthenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| app
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_optional_client_auth("tests/tls/ca.pem")))
        .setup(|app| {
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated_and_https_redirection() {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| app
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_required_client_auth("tests/tls/ca.pem")
                .with_https_redirection()
                .with_http_port(http_port))
            .with_hsts(|hsts| hsts
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"])))
        .setup(|app| {
            app.use_hsts();
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .header("host", "localhost")
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.headers().get(STRICT_TRANSPORT_SECURITY).unwrap(), "max-age=60; includeSubDomains");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_https_redirection() {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| app
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_https_redirection()
                .with_http_port(http_port))
            .with_hsts(|hsts| hsts
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"])))
        .setup(|app| {
            app.use_hsts();
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .redirect(Policy::none())
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .header("host", "localhost")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(&LOCATION).unwrap(), 
        format!("https://localhost:{}/tls", server.port).as_str()
    );
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_404_if_no_host() {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| app
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_https_redirection()
                .with_http_port(http_port))
            .with_hsts(|hsts| hsts
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"])))
        .setup(|app| {
            app.use_hsts();
            app.map_get("/tls", || async {
                "Pass!"
            });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server.client_builder()
        .add_root_certificate(ca_certificate)
        .redirect(Policy::none())
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}