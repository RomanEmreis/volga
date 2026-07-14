//! A complete OAuth 2.1 Authorization Code + PKCE flow between two volga
//! applications:
//!
//! * An **authorization server** (port 7979) publishing RFC 8414 metadata
//!   and a JWKS, with toy `/authorize` and `/token` endpoints issuing
//!   RS256-signed access tokens;
//! * A **resource server** (port 7878) that validates those tokens against
//!   the issuer's keys — no shared secret, just
//!   `with_oauth(..)` + `use_oauth()`;
//! * A **client** (this binary) driving the flow with `volga-oauth-client`:
//!   discovery → authorization request → code exchange → protected call.
//!
//! Run with:
//!
//! ```no_rust
//! cargo run -p oauth_flow
//! ```

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use volga::{
    App, Query,
    auth::{AuthClaims, roles},
    ok, status,
};
use volga_oauth_client::{ClientConfig, DiscoveryClient, OAuthClient};

const ISSUER: &str = "http://127.0.0.1:7979";
const RESOURCE: &str = "http://127.0.0.1:7878";
const KEY_ID: &str = "demo-key";

// Throwaway RSA private key generated for this example only — never ship
// a real private key inside a binary.
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

/// Query parameters of the `/authorize` endpoint (RFC 6749 §4.1.1).
#[derive(Deserialize)]
struct AuthorizeParams {
    response_type: String,
    #[allow(dead_code)]
    client_id: String,
    redirect_uri: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

/// Form body of the `/token` endpoint (RFC 6749 §4.1.3).
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: String,
    code_verifier: String,
}

/// Authorization codes waiting to be exchanged, mapped to their PKCE
/// challenge.
type PendingCodes = Arc<Mutex<HashMap<String, String>>>;

fn signing_key() -> jsonwebtoken::EncodingKey {
    jsonwebtoken::EncodingKey::from_rsa_pem(RSA_PRIVATE_PEM).unwrap()
}

/// The toy authorization server: metadata, JWKS, `/authorize`, `/token`.
fn authorization_server() -> App {
    let pending: PendingCodes = Arc::new(Mutex::new(HashMap::new()));

    let mut app = App::new()
        .bind("127.0.0.1:7979")
        .with_oauth_server_metadata(|m| {
            m.with_issuer(ISSUER)
                .with_authorization_endpoint(format!("{ISSUER}/authorize"))
                .with_token_endpoint(format!("{ISSUER}/token"))
                .with_jwks_uri(format!("{ISSUER}/jwks"))
        });

    // GET /.well-known/oauth-authorization-server
    app.use_oauth_server_metadata();

    // The verification keys, published as a JWKS — the resource server
    // fetches them through discovery, no key material is shared manually
    app.map_get("/jwks", || async {
        let jwk = jsonwebtoken::jwk::Jwk::from_encoding_key(
            &signing_key(),
            jsonwebtoken::Algorithm::RS256,
        );
        let mut jwk = jwk.unwrap();
        jwk.common.key_id = Some(KEY_ID.into());
        ok!({ "keys": [jwk] })
    });

    // A wildly simplified authorization endpoint: every request is
    // "approved" instantly — a real server authenticates the user and asks
    // for consent here
    let codes = pending.clone();
    app.map_get("/authorize", move |params: Query<AuthorizeParams>| {
        let codes = codes.clone();
        async move {
            if params.response_type != "code" || params.code_challenge_method != "S256" {
                return status!(400, { "error": "unsupported_response_type" });
            }
            let code = format!("demo-code-{}", codes.lock().unwrap().len());
            codes
                .lock()
                .unwrap()
                .insert(code.clone(), params.code_challenge.clone());

            status!(302; [(
                "Location",
                format!("{}?code={code}&state={}", params.redirect_uri, params.state)
            )])
        }
    });

    // The token endpoint: verifies the PKCE proof and issues an RS256 JWT
    let codes = pending;
    app.map_post("/token", move |form: volga::Form<TokenForm>| {
        let codes = codes.clone();
        async move {
            let challenge = codes.lock().unwrap().remove(&form.code);
            let valid = form.grant_type == "authorization_code"
                && challenge.is_some_and(|challenge| s256(&form.code_verifier) == challenge);
            if !valid {
                return status!(400, { "error": "invalid_grant" });
            }

            let exp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600;
            let claims = Claims {
                sub: "demo-user".into(),
                role: "admin".into(),
                iss: ISSUER.into(),
                exp,
            };
            let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
            header.kid = Some(KEY_ID.into());
            let token = jsonwebtoken::encode(&header, &claims, &signing_key()).unwrap();

            ok!({
                "access_token": token,
                "token_type": "Bearer",
                "expires_in": 3600
            })
        }
    });

    app
}

/// The protected resource server: token validation is wired to the
/// issuer's published keys — no secret is configured anywhere.
fn resource_server() -> App {
    let mut app = App::new()
        .bind("127.0.0.1:7878")
        // where the tokens come from; the plain-HTTP opt-out is for this
        // local demo only
        .with_oauth(|oauth| {
            oauth
                .with_issuer(ISSUER)
                .with_client_config(|client| client.require_https(false))
        })
        // advertised in WWW-Authenticate challenges (RFC 9728)
        .with_oauth_resource_metadata(|m| {
            m.with_resource(RESOURCE)
                .with_authorization_servers([ISSUER])
        });

    // both are explicit opt-ins
    app.use_oauth();
    app.use_oauth_resource_metadata();

    app.map_get("/protected", || async {
        ok!("Hello from the protected route!")
    })
    .authorize::<Claims>(roles(["admin"]));

    app
}

/// PKCE S256: BASE64URL-ENCODE(SHA256(verifier)) (RFC 7636 §4.2).
fn s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // both applications run inside this one process for the demo
    tokio::spawn(authorization_server().run());
    tokio::spawn(resource_server().run());
    tokio::time::sleep(Duration::from_millis(300)).await;

    // ---- the client side ----------------------------------------------

    // 1. an unauthenticated probe: the resource answers 401 with a
    //    challenge pointing at its metadata (RFC 6750 §3 / RFC 9728 §5.1)
    let (status, headers, _) = http_get(&format!("{RESOURCE}/protected"), None).await?;
    let challenge = header(&headers, "www-authenticate").unwrap_or_default();
    println!("without a token   : {status} / WWW-Authenticate: {challenge}");

    // 2. discover the authorization server
    let discovery = DiscoveryClient::with_config(ClientConfig::new().require_https(false));
    let metadata = discovery.fetch_server_metadata(ISSUER).await?;
    println!("discovered issuer : {}", metadata.issuer);

    // 3. build the authorization request (PKCE and state are generated)
    let client = OAuthClient::new("demo-client")
        .with_config(ClientConfig::new().require_https(false))
        .with_redirect_uri(format!("{RESOURCE}/callback"));

    let request = client
        .authorization_request(&metadata)
        .with_scopes(["read"])
        .build()?;

    // 4. the "browser" hop: follow the authorization URL and catch the
    //    redirect carrying the code
    let (_, headers, _) = http_get(&request.url, None).await?;
    let location = header(&headers, "location").expect("authorize must redirect");
    let code = query_param(&location, "code").expect("redirect carries the code");
    let state = query_param(&location, "state").expect("redirect carries the state");
    assert!(
        request.matches_state(&state),
        "state mismatch — possible CSRF"
    );
    println!("authorization code: {code}");

    // 5. exchange the code for tokens (the PKCE verifier goes along)
    let tokens = client.exchange_code(&metadata, &code, &request).await?;
    println!("access token      : {}...", &tokens.access_token[..24]);

    // 6. call the protected route with the token
    let (status, _, body) =
        http_get(&format!("{RESOURCE}/protected"), Some(&tokens.access_token)).await?;
    println!("with the token    : {status} / {body}");

    Ok(())
}

// ---- tiny plaintext HTTP/1.1 helpers, just enough for the demo ---------

/// Issues a GET and returns `(status, headers, body)`.
async fn http_get(
    url: &str,
    bearer: Option<&str>,
) -> Result<(u16, Vec<(String, String)>, String), Box<dyn std::error::Error>> {
    let rest = url.strip_prefix("http://").expect("demo URLs are http");
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let authorization = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();

    let mut stream = TcpStream::connect(host).await?;
    stream
        .write_all(
            format!(
                "GET /{path} HTTP/1.1\r\nHost: {host}\r\n{authorization}Connection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_owned()))
        .collect();
    // ignore transfer-encoding details — the body is only printed
    Ok((status, headers, body.trim().to_owned()))
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header, _)| header == name)
        .map(|(_, value)| value.as_str())
}

fn query_param(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(param, _)| *param == name)
        .map(|(_, value)| value.to_owned())
}
