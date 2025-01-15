use volga::App;
use volga::tls::TlsConfig;
use reqwest::{Certificate, Identity};

#[tokio::test]
async fn it_works_with_tls_with_no_auth() {
    tokio::spawn(async {
        let config = TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key"
        );
        let mut app = App::new()
            .bind("127.0.0.1:7921")
            .bind_tls(config);
        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();
        
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().add_root_certificate(ca_certificate).build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().add_root_certificate(ca_certificate).build().unwrap()
        };

        client
            .get("https://localhost:7921/tls")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated() {
    tokio::spawn(async {
        let config = TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key")
            .with_required_client_auth("tests/tls/ca.pem");
        
        let mut app = App::new()
            .bind("127.0.0.1:7922")
            .bind_tls(config);
        
        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let cert = include_bytes!("tls/client.pem");
        let key = include_bytes!("tls/client.key");
        
        let identity = Identity::from_pkcs8_pem(cert, key).unwrap();

        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();
        
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .http1_only()
                .identity(identity)
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .http2_prior_knowledge()
                .identity(identity)
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("https://localhost:7922/tls")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_unauthenticated() {
    tokio::spawn(async {
        let config = TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key")
            .with_required_client_auth("tests/tls/ca.pem");

        let mut app = App::new()
            .bind("127.0.0.1:7923")
            .bind_tls(config);

        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .http1_only()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .http2_prior_knowledge()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("https://localhost:7923/tls")
            .send()
            .await
            .unwrap()
    }).await;

    assert!(response.is_err());
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_authenticated() {
    tokio::spawn(async {
        let config = TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key")
            .with_optional_client_auth("tests/tls/ca.pem");

        let mut app = App::new()
            .bind("127.0.0.1:7924")
            .bind_tls(config);

        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let cert = include_bytes!("tls/client.pem");
        let key = include_bytes!("tls/client.key");

        let identity = Identity::from_pkcs8_pem(cert, key).unwrap();

        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .http1_only()
                .identity(identity)
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .http2_prior_knowledge()
                .identity(identity)
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("https://localhost:7924/tls")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_unauthenticated() {
    tokio::spawn(async {
        let config = TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key")
            .with_optional_client_auth("tests/tls/ca.pem");

        let mut app = App::new()
            .bind("127.0.0.1:7925")
            .bind_tls(config);

        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();
        
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .http1_only()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .http2_prior_knowledge()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("https://localhost:7925/tls")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");
}