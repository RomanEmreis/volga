//! End-to-end tests for issuer-based bearer authentication (`use_oauth`):
//! a volga [`TestServer`] plays the OAuth issuer (RFC 8414 metadata + JWKS)
//! for a second, protected volga application.

#![cfg(all(feature = "oauth-client", feature = "test", feature = "http1"))]

use jsonwebtoken::{Algorithm, EncodingKey, Header, jwk::Jwk};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use volga::{
    App, HttpRequest,
    auth::{AuthClaims, roles},
    test::TestServer,
};

// Throwaway RSA private key generated for these tests only.
const RSA_PRIVATE_PEM: &[u8] = b"-----BEGIN RSA PRIVATE KEY-----
MIIEpAIBAAKCAQEAwm6oskwz03jgyPI0dYWNmkJwaiKLL6jjedSH5VK0A5W9No6J
NTeHMurksTkMfuhBB7jz6OsEuwMQXs+BUijwjcsuj+XDEeeZ8LjshlyvyoXIcV7a
d1AXD5LM3Uw/D64diBn5jn2d3JUJqouQ8hs2hITBK5wRdQMx/Q7qAjIKUjb5Vgdu
SrIFH+CmvvV94AIf5hMZl8J1c1HzDaBQVJKQ7dh2uyB5xiWWhHIylWczR8Q4alXQ
sm5HKvC+ha3+n5sgevT/efmFd14S4QkE81C1NshIfE/KUJKPgMQPYZh3waOxbwnL
zoHuZr8AtwSQufc9K6NGaGhEd0h5NbfIQsoXOwIDAQABAoIBAESLPm2c76hdtOEi
gdvseT8orPi9tNPYdlk806vEvDGHWG0jUruwF7mblYPk2MLkngha66HxOHm1WtAR
10VfqW5TctbH6T0mqN50Uu4LPu3mvAM7rUjisz6KQi7B8nlUqJSSk6foIP7ii8XT
7gVsEowlQPRe0Mivl6/e0iB0A693k9nDz7YtOcO6jscGjjQFGvFmJgy2wTdt5Wf/
KNT6+yoKhSaYbDngvC93cMcgWduAVHT4N1mXyiLSqoUmMM3PVkFTTexyk5PDoXnP
NpwviEVOj1fqkWKy9Z7c7ApIzn8y1PH6DDnNUatWjDJqqxDRnMTBt6V/BzKSTSGp
ZTw3dAECgYEA9m2QcCVPiwxB1S2ESV6jUTo6pC8xZhkiMT+v4tsBpAmVk1xvL23y
Y/K8hUUkXIiKEWSJKs7zACbTsGZ17a6RrRlm83PR6d1sc7UsjdcfDbnBPxEOaS9O
uPe3Au4t2clPZKlVXrRJLBlFp6bffy9vw0XReqjECkhJrrDXMusht6kCgYEAyfwO
/6GEsY5uZFfD8Hzmy/CewiQrf5gRp1LaQf584/jDbdE+6P1JSoAeRct2PZ5RBc/D
pbD2+zKZAc17i8GF3CY8U6ZqRE6EiIHWQXGT0ikYiVjimyHtDaahI2sHxZffEYF2
Cqqafk5yWaxB0Q8s6PwsmxetqPgb3oIlSyzKlkMCgYBSuB6G9o9H3ppuo7PHKSRr
TL+Ig2rymbc3juhMnzViyfDSoXGVGzQFRuLvXXFCOncWNYgxvXwmbeIbUZl+al3u
HBvJ1vP8q94OzR8ikbaT1em/cMtElaO4RTbCng74DzI+WPUWMDBrxCP0jfhx6gt7
IgGaSfJcfT12jVf/eJw92QKBgQC++1z3KrK77E/HAxFav86+gKqsKOUURSZUDswe
YFGYgOvQV2xjgrKdBd0Z41LO2nYDx7pXXad6RxJTmQY7U+WNDn42HgEWyyMXq6R5
xrmdmov/uhKx2nc5VBfC1H3JwFsEQ2Pom/1udiA7V9v3n6C4P1Cx6MakIMzBLE+0
8Aox3wKBgQCyjGt18aZazm8TqgWo7db9ZhNWJ8tQEmxS/MDh1Se5P0Cq0GjhR/AJ
p/2sC9okZ1EKKSYrENGRiq4l/mvBM/lG/wa8SVAAiJZEysbvvPr9E1WkKmzZ+zN8
VIFweTXfx0uqVvsKBxyxdLbwSpwQD/6FAoGKKZ1PHFKjfjiZEsYWPQ==
-----END RSA PRIVATE KEY-----
";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    iss: String,
    exp: u64,
}

impl AuthClaims for Claims {
    fn role(&self) -> Option<&str> {
        Some(&self.role)
    }
}

fn signing_key() -> EncodingKey {
    EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM).unwrap()
}

fn sign_token(kid: &str, issuer: &str, role: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_owned());
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let claims = Claims {
        sub: "test-user".to_owned(),
        role: role.to_owned(),
        iss: issuer.to_owned(),
        exp,
    };
    jsonwebtoken::encode(&header, &claims, &signing_key()).unwrap()
}

/// The public JWK of the test signing key, under the given `kid`.
fn jwk(kid: &str) -> serde_json::Value {
    let mut jwk = Jwk::from_encoding_key(&signing_key(), Algorithm::RS256).unwrap();
    jwk.common.key_id = Some(kid.to_owned());
    serde_json::to_value(&jwk).unwrap()
}

struct Issuer {
    server: TestServer,
    jwks: Arc<RwLock<serde_json::Value>>,
    jwks_hits: Arc<AtomicUsize>,
}

impl Issuer {
    fn url(&self) -> String {
        self.server.url("")
    }

    fn rotate_to(&self, kid: &str) {
        *self.jwks.write().unwrap() = json!({ "keys": [jwk(kid)] });
    }
}

/// Spawns a volga app playing the authorization server: RFC 8414 metadata
/// (the issuer is derived from the `Host` header, so the dynamically
/// assigned port needs no coordination) and a mutable JWKS endpoint.
async fn spawn_issuer(initial_kid: &str) -> Issuer {
    let jwks = Arc::new(RwLock::new(json!({ "keys": [jwk(initial_kid)] })));
    let jwks_hits = Arc::new(AtomicUsize::new(0));

    let served = jwks.clone();
    let hits = jwks_hits.clone();
    let server = TestServer::spawn(move |app| {
        app.map_get(
            "/.well-known/oauth-authorization-server",
            |req: HttpRequest| async move {
                let host = req.headers().get("host").unwrap().to_str().unwrap();
                let issuer = format!("http://{host}");
                volga::ok!({
                    "issuer": issuer,
                    "jwks_uri": format!("{issuer}/jwks"),
                    "response_types_supported": ["code"]
                })
            },
        );
        app.map_get("/jwks", move || {
            let served = served.clone();
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                let document = served.read().unwrap().clone();
                volga::ok!(document)
            }
        });
    })
    .await;

    Issuer {
        server,
        jwks,
        jwks_hits,
    }
}

/// Spawns the protected resource app validating tokens against `issuer`.
async fn spawn_resource(issuer: String, cooldown: Duration) -> TestServer {
    TestServer::builder()
        .configure(move |app| {
            app.with_oauth(|oauth| {
                oauth
                    .with_issuer(&issuer)
                    .with_client_config(|client| client.require_https(false))
                    .with_refresh_cooldown(cooldown)
            })
        })
        .setup(|app: &mut App| {
            app.use_oauth();
            app.map_get("/protected", || async { volga::ok!("secret") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await
}

#[tokio::test]
async fn it_authorizes_with_issuer_published_keys() {
    let issuer = spawn_issuer("key-1").await;
    let resource = spawn_resource(issuer.url(), Duration::from_secs(60)).await;

    // a token signed by the issuer's key passes
    let token = sign_token("key-1", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "secret");

    // the right key but the wrong role is rejected by the authorizer
    let token = sign_token("key-1", &issuer.url(), "guest");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    // a token claiming a different issuer fails validation
    let token = sign_token("key-1", "https://evil.example.com", "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let challenge = res.headers()["www-authenticate"].to_str().unwrap();
    assert!(challenge.contains("invalid_token"), "was: {challenge}");

    resource.shutdown().await;
    issuer.server.shutdown().await;
}

#[tokio::test]
async fn it_refreshes_keys_on_rotation() {
    let issuer = spawn_issuer("old-key").await;
    // zero cooldown: rotation is picked up immediately
    let resource = spawn_resource(issuer.url(), Duration::ZERO).await;

    let token = sign_token("old-key", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 1);

    // the issuer rotates its signing key; a token under the new kid
    // triggers a refresh and passes without a restart
    issuer.rotate_to("new-key");
    let token = sign_token("new-key", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 2);

    resource.shutdown().await;
    issuer.server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_unknown_kids_as_invalid_tokens() {
    let issuer = spawn_issuer("key-1").await;
    let resource = spawn_resource(issuer.url(), Duration::ZERO).await;

    let token = sign_token("ghost-key", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let challenge = res.headers()["www-authenticate"].to_str().unwrap();
    assert!(challenge.contains("invalid_token"), "was: {challenge}");

    resource.shutdown().await;
    issuer.server.shutdown().await;
}

#[tokio::test]
async fn it_respects_the_refresh_cooldown() {
    let issuer = spawn_issuer("key-1").await;
    let resource = spawn_resource(issuer.url(), Duration::from_secs(3600)).await;

    // the first request loads the keys
    let token = sign_token("key-1", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 1);

    // a flood of unknown kids inside the cooldown must not hammer the
    // issuer: no extra JWKS fetches, tokens are rejected as invalid
    for _ in 0..3 {
        let token = sign_token("ghost-key", &issuer.url(), "admin");
        let res = resource
            .client()
            .get(resource.url("/protected"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    }
    assert_eq!(issuer.jwks_hits.load(Ordering::SeqCst), 1);

    resource.shutdown().await;
    issuer.server.shutdown().await;
}

#[tokio::test]
async fn it_answers_503_while_the_issuer_is_unreachable() {
    // nothing listens on this issuer
    let resource = spawn_resource("http://127.0.0.1:9".to_owned(), Duration::ZERO).await;

    let token = sign_token("key-1", "http://127.0.0.1:9", "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
    // an unreachable issuer is not the client's fault — no challenge
    assert!(res.headers().get("www-authenticate").is_none());

    resource.shutdown().await;
}

#[tokio::test]
async fn it_challenges_missing_credentials_with_401() {
    let issuer = spawn_issuer("key-1").await;
    let resource = spawn_resource(issuer.url(), Duration::from_secs(60)).await;

    // RFC 6750 §3: no credentials — 401 with a bare Bearer challenge
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    let challenge = res.headers()["www-authenticate"].to_str().unwrap();
    assert!(challenge.starts_with("Bearer"), "was: {challenge}");
    assert!(!challenge.contains("error"), "was: {challenge}");

    resource.shutdown().await;
    issuer.server.shutdown().await;
}

#[cfg(feature = "config")]
#[tokio::test]
async fn it_configures_the_issuer_from_a_config_file() {
    use std::io::Write;

    let issuer = spawn_issuer("key-1").await;

    // the issuer is described in the config file; activation stays in code
    let mut file = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
    write!(
        file,
        "[oauth.client]\n\
         issuer = \"{}\"\n\
         require_https = false\n",
        issuer.url()
    )
    .unwrap();
    let path = file.path().to_str().unwrap().to_owned();

    let resource = TestServer::builder()
        .configure(move |app| app.with_config(|cfg| cfg.with_file(&path)))
        .setup(|app: &mut App| {
            app.use_oauth();
            app.map_get("/protected", || async { volga::ok!("secret") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let token = sign_token("key-1", &issuer.url(), "admin");
    let res = resource
        .client()
        .get(resource.url("/protected"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "secret");

    resource.shutdown().await;
    issuer.server.shutdown().await;
}
