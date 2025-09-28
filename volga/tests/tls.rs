use std::time::Duration;
use volga::App;
use volga::http::StatusCode;
use volga::headers::{STRICT_TRANSPORT_SECURITY, LOCATION};
use volga::tls::TlsConfig;
use reqwest::{Certificate, Identity, redirect::Policy};

#[tokio::test]
async fn it_works_with_tls_with_no_auth() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7921")
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"));
        
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
        let mut app = App::new()
            .bind("127.0.0.1:7922")
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_required_client_auth("tests/tls/ca.pem"));
        
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
        let mut app = App::new()
            .bind("127.0.0.1:7923")
            .with_tls(|tls| tls
                .with_key_path("tests/tls/server.key")
                .with_cert_path("tests/tls/server.pem")
                .with_required_client_auth("tests/tls/ca.pem"));
        
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
        let mut app = App::new()
            .bind("127.0.0.1:7924")
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key")
                .with_optional_client_auth("tests/tls/ca.pem"));
        
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
        let mut app = App::new()
            .bind("127.0.0.1:7925")
            .with_tls(|tls| tls
                .with_cert_path("tests/tls/server.pem")
                .with_key_path("tests/tls/server.key")
                .with_optional_client_auth("tests/tls/ca.pem"));

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

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated_and_https_redirection() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind(([127,0,0,1], 7926))
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_https_redirection()
                .with_http_port(7927))
            .with_hsts(|hsts| hsts            
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"]));
        app.use_hsts();
        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        use tokio::time::{sleep, Duration};
        
        // Giving a little more time for the task spawned above
        sleep(Duration::from_millis(10)).await;
        
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
            .get("http://127.0.0.1:7927/tls")
            .header("host", "localhost:7927")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get(STRICT_TRANSPORT_SECURITY).unwrap(), "max-age=60; includeSubDomains");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_works_with_tls_with_https_redirection() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind(([127,0,0,1], 7940))
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_https_redirection()
                .with_http_port(7941))
            .with_hsts(|hsts| hsts
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"]));
        app.use_hsts();
        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        use tokio::time::{sleep, Duration};

        // Giving a little more time for the task spawned above
        sleep(Duration::from_millis(10)).await;

        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .redirect(Policy::none())
                .http1_only()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .redirect(Policy::none())
                .http2_prior_knowledge()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("http://127.0.0.1:7941/tls")
            .header("host", "localhost:7941")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.headers().get(&LOCATION).unwrap(), "https://localhost:7940/tls");
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
}

#[tokio::test]
async fn it_returns_404_if_no_host() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind(([127,0,0,1], 7942))
            .set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key"))
            .with_tls(|tls| tls
                .with_https_redirection()
                .with_http_port(7943))
            .with_hsts(|hsts| hsts
                .with_preload(false)
                .with_sub_domains(true)
                .with_max_age(Duration::from_secs(60))
                .with_exclude_hosts(&["example.com", "example.net"]));
        app.use_hsts();
        app.map_get("/tls", || async {
            "Pass!"
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        use tokio::time::{sleep, Duration};

        // Giving a little more time for the task spawned above
        sleep(Duration::from_millis(10)).await;

        let ca_cert = include_bytes!("tls/ca.pem");
        let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder()
                .redirect(Policy::none())
                .http1_only()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        } else {
            reqwest::Client::builder()
                .redirect(Policy::none())
                .http2_prior_knowledge()
                .add_root_certificate(ca_certificate)
                .build()
                .unwrap()
        };

        client
            .get("http://127.0.0.1:7943/tls")
            .send()
            .await
            .unwrap()
    }).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}