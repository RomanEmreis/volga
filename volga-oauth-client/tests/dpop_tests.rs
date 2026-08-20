//! End-to-end DPoP tests (RFC 9449): a real volga application playing the
//! token endpoint and a DPoP-protected resource, verifying the proofs the
//! way a server would. Same HTTP/1-only gating as the discovery suite (see
//! `discovery_tests.rs`).

#![cfg(all(feature = "http1", feature = "dpop"))]

mod common;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use bytes::Bytes;
use common::{free_port, serve};
use http::{HeaderMap, Method, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header, jwk::ThumbprintHash,
};
use serde::Deserialize;
use volga::{App, headers::HttpHeaders};
use volga_oauth_client::{
    AuthorizationServerMetadata, BearerChallenge, ClientConfig, ClientError, DPOP_HEADER,
    DPOP_NONCE_HEADER, Dpop, OAuthClient, OAuthErrorCode, TokenSet, auth_scheme, grant,
};

/// The claims of a DPoP proof (RFC 9449 Section 4.2), as a server reads them.
#[derive(Deserialize)]
struct ProofClaims {
    jti: String,
    htm: String,
    htu: String,
    #[allow(dead_code)]
    iat: u64,
    ath: Option<String>,
    nonce: Option<String>,
}

/// A verified proof: its claims and the thumbprint of the key that signed
/// it - the `jkt` an authorization server binds the issued token to.
struct VerifiedProof {
    claims: ProofClaims,
    thumbprint: String,
}

/// Verifies a proof the way a server would: `typ`, the signature against
/// the public key the header publishes, and nothing else to go on.
fn verify_proof(proof: &str) -> VerifiedProof {
    let header = decode_header(proof).expect("the proof must have a JOSE header");
    assert_eq!(header.typ.as_deref(), Some("dpop+jwt"));

    let jwk = header.jwk.expect("a proof must publish its public key");
    let thumbprint = jwk
        .thumbprint(ThumbprintHash::SHA256)
        .expect("the published key must be thumbprintable");

    let mut validation = Validation::new(match header.alg {
        Algorithm::ES256 => Algorithm::ES256,
        other => panic!("unexpected proof algorithm: {other:?}"),
    });
    // a proof carries no `exp` and no `aud`; freshness is `iat` and the
    // nonce, which this endpoint judges itself
    validation.required_spec_claims.clear();
    validation.validate_exp = false;

    let claims = decode::<ProofClaims>(proof, &DecodingKey::from_jwk(&jwk).unwrap(), &validation)
        .expect("the proof must verify against the key it publishes")
        .claims;

    VerifiedProof { claims, thumbprint }
}

/// Reads the `DPoP` header of a request, or fails the request the way a
/// server without one would.
fn proof_of(headers: &HttpHeaders) -> Option<String> {
    headers
        .get_raw(DPOP_HEADER)
        .and_then(|proof| proof.to_str().ok())
        .map(ToOwned::to_owned)
}

/// An OAuth client accepting the plaintext test server.
fn plaintext_client(client_id: &str) -> OAuthClient {
    OAuthClient::new(client_id).with_config(ClientConfig::new().require_https(false))
}

fn server_metadata(base: &str) -> AuthorizationServerMetadata {
    let mut metadata = AuthorizationServerMetadata::new(base);
    metadata.token_endpoint = Some(format!("{base}/token"));
    metadata.grant_types_supported = vec![grant::CLIENT_CREDENTIALS.into()];
    metadata.dpop_signing_alg_values_supported = vec!["ES256".into()];
    metadata
}

/// Sends a plain `GET`, returning the status, headers and body - the raw
/// resource request a consumer of this crate makes for itself.
async fn get(url: &str, headers: HeaderMap) -> (StatusCode, HeaderMap, String) {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .build(hyper_util::client::legacy::connect::HttpConnector::new());

    let mut request = http::Request::builder()
        .method(Method::GET)
        .uri(url)
        .body(Full::default())
        .unwrap();
    request.headers_mut().extend(headers);

    let response = client.request(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let body = response.into_body().collect().await.unwrap().to_bytes();

    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn it_binds_a_token_request_to_the_dpop_key() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base}/token");

    let requests = Arc::new(AtomicUsize::new(0));
    let identifiers: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (seen, collected) = (requests.clone(), identifiers.clone());
    let htu = endpoint.clone();

    // the endpoint demands a nonce on the first request and echoes the
    // thumbprint of the proving key back as the access token, so the client
    // side can assert the token is bound to the key it signed with
    let mut app = App::new();
    app.map_post("/token", move |headers: HttpHeaders| {
        let (seen, collected, htu) = (seen.clone(), collected.clone(), htu.clone());
        async move {
            let attempt = seen.fetch_add(1, Ordering::SeqCst);
            let Some(proof) = proof_of(&headers) else {
                return volga::status!(400, { "error": "invalid_dpop_proof" });
            };

            let proof = verify_proof(&proof);
            assert_eq!(proof.claims.htm, "POST");
            assert_eq!(proof.claims.htu, htu);
            collected.lock().unwrap().push(proof.claims.jti.clone());
            // no access token is presented to the token endpoint
            assert!(proof.claims.ath.is_none());

            // the nonce rotates on the way out: the retry is answered with
            // `n-2`, which the client has to pick up from a *successful*
            // response and use next time (RFC 9449 Section 8.2)
            let expected = match attempt {
                0 => None,
                1 => Some("n-1"),
                _ => Some("n-2"),
            };
            assert_eq!(
                proof.claims.nonce.as_deref(),
                expected,
                "request {attempt} carried the wrong nonce"
            );

            match expected {
                None => volga::status!(
                    400,
                    { "error": "use_dpop_nonce",
                      "error_description": "Authorization server requires nonce in DPoP proof" };
                    [("dpop-nonce", "n-1")]
                ),
                Some(_) => volga::ok!(
                    { "access_token": proof.thumbprint,
                      "token_type": "DPoP",
                      "expires_in": 3600 };
                    [("dpop-nonce", "n-2")]
                ),
            }
        }
    });
    let server = serve(port, app).await;

    let dpop = Dpop::generate().unwrap();
    let client = plaintext_client("my-service").with_dpop(dpop.clone());
    let metadata = server_metadata(&base);

    let tokens = client
        .client_credentials(&metadata)
        .send()
        .await
        .expect("the nonce round must be answered transparently");

    // the token came back bound to the key that proved possession...
    assert!(tokens.is_dpop());
    assert_eq!(tokens.access_token, dpop.thumbprint());
    // ...after exactly one retry
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    // ...and the nonce the *retry* was answered with is the one now held,
    // not the one that provoked the retry - a nonce arriving on a success
    // is still a nonce, and dropping it would cost the next request a round
    // trip to be told again. The endpoint asserts that itself: request 2
    // has to carry `n-2`, and it takes a single request to get there. (The
    // token endpoint's nonces are not readable through `Dpop::nonce`, which
    // reports what a *resource* handed out - RFC 9449 keeps the two apart.)
    let second = client.client_credentials(&metadata).send().await.unwrap();
    assert_eq!(second.access_token, dpop.thumbprint());
    assert_eq!(requests.load(Ordering::SeqCst), 3);

    // every request carried a freshly signed proof: a captured one is not
    // replayable (RFC 9449 Section 11.1)
    let identifiers = identifiers.lock().unwrap();
    let unique: std::collections::HashSet<&String> = identifiers.iter().collect();
    assert_eq!(unique.len(), identifiers.len(), "a `jti` was reused");

    server.abort();
}

#[tokio::test]
async fn it_retries_a_nonce_refusal_exactly_once() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // a server that refuses forever, handing out a *fresh* nonce every
    // time: the retry must be bounded by the attempt, not by the nonce
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();

    let mut app = App::new();
    app.map_post("/token", move |headers: HttpHeaders| {
        let seen = seen.clone();
        async move {
            let attempt = seen.fetch_add(1, Ordering::SeqCst);
            assert!(proof_of(&headers).is_some(), "the request carried no proof");
            volga::status!(
                400,
                { "error": "use_dpop_nonce" };
                [("dpop-nonce", format!("n-{attempt}"))]
            )
        }
    });
    let server = serve(port, app).await;

    let client = plaintext_client("my-service").with_dpop(Dpop::generate().unwrap());
    let err = client
        .client_credentials(&server_metadata(&base))
        .send()
        .await
        .unwrap_err();

    assert!(
        matches!(&err, ClientError::Protocol(err) if err.error == OAuthErrorCode::UseDpopNonce),
        "got: {err}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    server.abort();
}

#[tokio::test]
async fn it_retries_when_the_nonce_it_sent_was_not_the_one_demanded() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base}/token");

    let dpop = Dpop::generate().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let (seen, sibling, target) = (requests.clone(), dpop.clone(), endpoint.clone());

    // Two concurrent token requests, neither carrying a nonce, challenged
    // with the *same* one: whichever answer is processed first stores it,
    // and the second then finds the shared state already holding the nonce
    // it is being told to use. That is the interleaving reproduced here -
    // the endpoint stores the nonce into the client's shared state before
    // replying, exactly as a sibling response landing first would.
    let mut app = App::new();
    app.map_post("/token", move |headers: HttpHeaders| {
        let (seen, sibling, target) = (seen.clone(), sibling.clone(), target.clone());
        async move {
            let attempt = seen.fetch_add(1, Ordering::SeqCst);
            let proof = verify_proof(&proof_of(&headers).expect("no proof"));

            match proof.claims.nonce.as_deref() {
                None => {
                    assert_eq!(attempt, 0, "the nonce was demanded once already");
                    // the sibling's answer, landing first
                    sibling.remember_nonce(&target, "n-1");
                    volga::status!(
                        400,
                        { "error": "use_dpop_nonce" };
                        [("dpop-nonce", "n-1")]
                    )
                }
                Some("n-1") => volga::ok!({ "access_token": "at", "token_type": "DPoP" }),
                Some(other) => panic!("unexpected nonce: {other}"),
            }
        }
    });
    let server = serve(port, app).await;

    let client = plaintext_client("my-service").with_dpop(dpop.clone());
    let tokens = client
        .client_credentials(&server_metadata(&base))
        .send()
        .await
        .expect("a request refused for a nonce it did not carry must be retried");

    // the shared state having already learned the nonce says nothing about
    // what *this* proof contained - the request was refused for a nonce it
    // did not send, so it is repeated with the one demanded
    assert!(tokens.is_dpop());
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    server.abort();
}

/// A throwaway P-256 key pair: the client signs assertions with the
/// private half, the test token endpoint verifies with the public one.
#[cfg(feature = "private-key-jwt")]
const CLIENT_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgIeRig+AlqV2rBdgt
BzEQ28UAk8/d5l2+4PDfsspynmShRANCAATP07xL4i2PpomWJmZSZZMbQqj4Ybbd
aLozept2OHnD6J7pNTHm12NdaEJ4knzrCkp6pho2EFIQh5cKnqHm+hQw
-----END PRIVATE KEY-----";

#[cfg(feature = "private-key-jwt")]
const CLIENT_PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEz9O8S+Itj6aJliZmUmWTG0Ko+GG2
3Wi6M3qbdjh5w+ie6TUx5tdjXWhCeJJ86wpKeqYaNhBSEIeXCp6h5voUMA==
-----END PUBLIC KEY-----";

#[cfg(feature = "private-key-jwt")]
#[derive(Deserialize)]
struct AssertionForm {
    client_assertion: Option<String>,
}

#[cfg(feature = "private-key-jwt")]
#[derive(Deserialize)]
struct AssertionClaims {
    jti: String,
}

/// Verifies a client assertion the way an authorization server would and
/// returns its `jti` - the identifier a replay-protecting server records.
#[cfg(feature = "private-key-jwt")]
fn assertion_jti(assertion: &str, client_id: &str, issuer: &str) -> String {
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_issuer(&[client_id]);
    validation.set_audience(&[issuer]);

    let key = DecodingKey::from_ec_pem(CLIENT_PUBLIC_KEY_PEM).unwrap();
    decode::<AssertionClaims>(assertion, &key, &validation)
        .expect("the client assertion must verify")
        .claims
        .jti
}

/// The nonce round repeats the token request, and a repeat has to be a new
/// request: `private_key_jwt` authenticates with a one-shot assertion, and
/// RFC 7523 Section 3 invites the server to remember its `jti`. Resending
/// the first request's bytes would present that assertion twice.
#[cfg(feature = "private-key-jwt")]
#[tokio::test]
async fn it_mints_a_fresh_client_assertion_for_the_nonce_retry() {
    use volga::Form;
    use volga_oauth_client::{JwsAlgorithm, PrivateKeyJwt, client_auth};

    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let issuer = base.clone();

    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded = seen.clone();

    let mut app = App::new();
    app.map_post(
        "/token",
        move |headers: HttpHeaders, form: Form<AssertionForm>| {
            let (recorded, issuer) = (recorded.clone(), issuer.clone());
            async move {
                let proof = verify_proof(&proof_of(&headers).expect("no proof"));
                let assertion = form
                    .client_assertion
                    .as_deref()
                    .expect("the request carried no client assertion");
                let jti = assertion_jti(assertion, "my-service", &issuer);

                // a replay-protecting server refuses an assertion it has
                // already consumed
                if !recorded.lock().unwrap().insert_unique(jti) {
                    return volga::status!(401, { "error": "invalid_client" });
                }

                match proof.claims.nonce.as_deref() {
                    None => volga::status!(
                        400,
                        { "error": "use_dpop_nonce" };
                        [("dpop-nonce", "n-1")]
                    ),
                    Some("n-1") => {
                        volga::ok!({ "access_token": "at", "token_type": "DPoP" })
                    }
                    Some(other) => panic!("unexpected nonce: {other}"),
                }
            }
        },
    );
    let server = serve(port, app).await;

    let mut metadata = server_metadata(&base);
    metadata.token_endpoint_auth_methods_supported = vec![client_auth::PRIVATE_KEY_JWT.into()];

    let key = PrivateKeyJwt::from_pem(CLIENT_KEY_PEM, JwsAlgorithm::ES256).unwrap();
    let client = plaintext_client("my-service")
        .with_private_key_jwt(key)
        .with_dpop(Dpop::generate().unwrap());

    let tokens = client
        .client_credentials(&metadata)
        .send()
        .await
        .expect("the retry must present a freshly signed assertion");

    assert!(tokens.is_dpop());
    // both attempts authenticated, each with an assertion of its own
    assert_eq!(seen.lock().unwrap().len(), 2);

    server.abort();
}

/// `Vec::push` that reports whether the value was new.
#[cfg(feature = "private-key-jwt")]
trait InsertUnique {
    fn insert_unique(&mut self, value: String) -> bool;
}

#[cfg(feature = "private-key-jwt")]
impl InsertUnique for Vec<String> {
    fn insert_unique(&mut self, value: String) -> bool {
        if self.contains(&value) {
            return false;
        }
        self.push(value);
        true
    }
}

#[tokio::test]
async fn it_refuses_an_algorithm_the_server_does_not_accept() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let requests = Arc::new(AtomicUsize::new(0));
    let seen = requests.clone();

    let mut app = App::new();
    app.map_post("/token", move |_headers: HttpHeaders| {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            volga::ok!({ "access_token": "at", "token_type": "DPoP" })
        }
    });
    let server = serve(port, app).await;

    let mut metadata = server_metadata(&base);
    metadata.dpop_signing_alg_values_supported = vec!["RS256".into()];

    let client = plaintext_client("my-service").with_dpop(Dpop::generate().unwrap());
    let err = client
        .client_credentials(&metadata)
        .send()
        .await
        .unwrap_err();

    assert!(matches!(&err, ClientError::Validation(reason) if reason.contains("ES256")));
    // the server advertised what it accepts; there was nothing to try
    assert_eq!(requests.load(Ordering::SeqCst), 0);

    server.abort();
}

#[tokio::test]
async fn it_protects_a_resource_request_with_a_proof() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let url = format!("{base}/orders");

    let dpop = Dpop::generate().unwrap();
    let tokens = TokenSet {
        access_token: "at-1".into(),
        token_type: auth_scheme::DPOP.into(),
        refresh_token: None,
        scope: None,
        id_token: None,
        expires_at: None,
        dpop_jkt: Some(dpop.thumbprint().to_owned()),
    };

    // a resource server that binds the token to the key (RFC 9449
    // Section 6), demands `ath` and asks for a nonce once (Section 9)
    let jkt = dpop.thumbprint().to_owned();
    let htu = url.clone();
    let ath = {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        URL_SAFE_NO_PAD.encode(
            aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, tokens.access_token.as_bytes())
                .as_ref(),
        )
    };

    let mut app = App::new();
    app.map_get("/orders", move |headers: HttpHeaders| {
        let (jkt, htu, ath) = (jkt.clone(), htu.clone(), ath.clone());
        async move {
            let credential = headers
                .get_raw(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            // a DPoP-bound token is never accepted as a bearer token
            assert_eq!(credential, "DPoP at-1");

            let proof = verify_proof(&proof_of(&headers).expect("no proof"));
            assert_eq!(proof.claims.htm, "GET");
            // the query string is not part of the proven target
            assert_eq!(proof.claims.htu, htu);
            // the proof is bound to the token presented alongside it...
            assert_eq!(proof.claims.ath.as_deref(), Some(ath.as_str()));
            // ...and the token is bound to the key that signed it
            assert_eq!(proof.thumbprint, jkt);

            match proof.claims.nonce.as_deref() {
                Some("rs-1") => volga::ok!({ "orders": [] }),
                _ => volga::status!(
                    401,
                    { "error": "use_dpop_nonce" };
                    [
                        ("www-authenticate", r#"DPoP error="use_dpop_nonce", error_description="Resource server requires nonce""#),
                        ("dpop-nonce", "rs-1")
                    ]
                ),
            }
        }
    });
    let server = serve(port, app).await;

    // this is the consumer's half: mint the proof, send the request, and
    // repeat it once when the resource asks for a nonce
    let target = format!("{url}?page=2");
    let mut headers = HeaderMap::new();
    dpop.authorize(&mut headers, &Method::GET, &target, &tokens)
        .unwrap();

    let (status, response_headers, _) = get(&target, headers).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let challenge = BearerChallenge::parse_scheme(
        response_headers[http::header::WWW_AUTHENTICATE]
            .to_str()
            .unwrap(),
        auth_scheme::DPOP,
    )
    .unwrap();
    assert_eq!(challenge.error(), Some(&OAuthErrorCode::UseDpopNonce));

    // the nonce the challenge came with is what makes the retry worth
    // making - and the proof picks it up on its own
    let demanded = dpop
        .accept_nonce(&target, &response_headers)
        .expect("the challenge must carry the nonce to retry with");
    assert_eq!(demanded, "rs-1");

    // the retry answers with the nonce *that* response demanded, not with
    // whatever the shared state holds by now
    let mut headers = HeaderMap::new();
    dpop.authorize_with_nonce(&mut headers, &Method::GET, &target, &tokens, &demanded)
        .unwrap();
    let (status, _, body) = get(&target, headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"orders":[]}"#);

    server.abort();
}

#[tokio::test]
async fn it_refuses_a_token_the_server_did_not_bind() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // a server with no DPoP support ignores the header it does not know and
    // answers with an ordinary bearer token
    let mut app = App::new();
    app.map_post("/token", |headers: HttpHeaders| async move {
        assert!(proof_of(&headers).is_some());
        volga::ok!({ "access_token": "at", "token_type": "Bearer", "expires_in": 3600 })
    });
    let server = serve(port, app).await;

    // ...and its metadata says nothing either way, so nothing could have
    // been caught before the request
    let mut metadata = server_metadata(&base);
    metadata.dpop_signing_alg_values_supported.clear();

    let store = Arc::new(volga_oauth_client::InMemoryTokenStore::new());
    let client = plaintext_client("my-service")
        .with_dpop(Dpop::generate().unwrap())
        .with_token_store(store.clone());

    let err = client
        .client_credentials(&metadata)
        .token("service")
        .await
        .expect_err("an unbound token must not be taken for a bound one");

    assert!(
        matches!(&err, ClientError::Validation(reason) if reason.contains("Bearer")),
        "got: {err}"
    );
    // ...and nothing unbound reached the store
    assert!(
        volga_oauth_client::TokenStore::get(store.as_ref(), "service").is_none(),
        "an unbound credential was cached"
    );

    server.abort();
}

#[tokio::test]
async fn it_discards_a_stored_token_it_cannot_present() {
    use volga_oauth_client::{InMemoryTokenStore, TokenStore};

    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let issued = Arc::new(AtomicUsize::new(0));
    let seen = issued.clone();

    let mut app = App::new();
    app.map_post("/token", move |headers: HttpHeaders| {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            let proof = verify_proof(&proof_of(&headers).expect("no proof"));
            volga::ok!({
                "access_token": proof.thumbprint,
                "token_type": "DPoP",
                "expires_in": 3600
            })
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let store = Arc::new(InMemoryTokenStore::new());

    // an entry left by an earlier run: unexpired, but bound to a key this
    // process does not hold - a persistent store outlives a generated key
    let stale = TokenSet {
        access_token: "at-from-a-previous-run".into(),
        token_type: auth_scheme::DPOP.into(),
        refresh_token: None,
        scope: None,
        id_token: None,
        expires_at: Some(std::time::SystemTime::now() + std::time::Duration::from_secs(3600)),
        dpop_jkt: Some("a-thumbprint-of-some-other-key".into()),
    };
    store.put("service", &stale);

    let dpop = Dpop::generate().unwrap();
    let client = plaintext_client("my-service")
        .with_dpop(dpop.clone())
        .with_token_store(store.clone());

    // nothing this key can prove possession of, so the grant runs again
    let tokens = client
        .client_credentials(&metadata)
        .token("service")
        .await
        .unwrap();

    assert_eq!(issued.load(Ordering::SeqCst), 1);
    assert_eq!(tokens.access_token, dpop.thumbprint());
    assert_eq!(tokens.dpop_jkt.as_deref(), Some(dpop.thumbprint()));

    // ...and the entry now in the store is the one this key can present, so
    // the next call is served from it
    let again = client
        .client_credentials(&metadata)
        .token("service")
        .await
        .unwrap();
    assert_eq!(issued.load(Ordering::SeqCst), 1);
    assert_eq!(again.access_token, tokens.access_token);

    // a bearer entry cached before DPoP was configured is likewise not a
    // token this client has - it would walk past the downgrade check
    store.put(
        "service",
        &TokenSet {
            token_type: "Bearer".into(),
            dpop_jkt: None,
            ..tokens.clone()
        },
    );
    client
        .client_credentials(&metadata)
        .token("service")
        .await
        .unwrap();
    assert_eq!(issued.load(Ordering::SeqCst), 2);

    server.abort();
}

#[tokio::test]
async fn it_leaves_a_bearer_client_untouched() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // no DPoP configured: nothing about the request changes
    let mut app = App::new();
    app.map_post("/token", |headers: HttpHeaders| async move {
        assert!(headers.get_raw(DPOP_HEADER).is_none());
        volga::ok!({ "access_token": "at", "token_type": "Bearer" })
    });
    let server = serve(port, app).await;

    let client = plaintext_client("my-service");
    let tokens = client
        .client_credentials(&server_metadata(&base))
        .send()
        .await
        .unwrap();

    assert!(!tokens.is_dpop());
    assert!(client.dpop().is_none());
    // ...and a nonce header on a response nobody asked for is ignored
    assert!(HeaderMap::new().get(DPOP_NONCE_HEADER).is_none());

    server.abort();
}
