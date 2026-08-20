//! Demonstrating Proof of Possession (DPoP) - sender-constrained tokens ([RFC 9449](https://www.rfc-editor.org/rfc/rfc9449))
//!
//! A bearer token is a password: whoever holds it may use it. DPoP binds
//! the token to a key the client holds instead, and every request carries a
//! freshly signed *proof* that the sender possesses it. A stolen token is
//! then worth nothing without the key.
//!
//! [`Dpop`] is that key plus the nonce state of the servers it talks to.
//! Attach it with [`OAuthClient::with_dpop`](crate::OAuthClient::with_dpop)
//! and every token request carries a proof, including the nonce round a
//! server may demand (RFC 9449 Section 8). Use it directly to protect a
//! request of your own:
//!
//! ```no_run
//! # use http::{HeaderMap, Method};
//! # use volga_oauth_client::{ClientError, Dpop, TokenSet};
//! # fn run(dpop: &Dpop, tokens: &TokenSet) -> Result<(), ClientError> {
//! let mut headers = HeaderMap::new();
//! dpop.authorize(
//!     &mut headers,
//!     &Method::GET,
//!     "https://api.example.com/orders?page=2",
//!     tokens,
//! )?;
//! // headers now carry `Authorization: DPoP <token>` and `DPoP: <proof>`
//! # Ok(())
//! # }
//! ```
//!
//! Sending the request is the caller's business - this crate mints proofs
//! and owns the nonce state, it is not an HTTP client for resource
//! requests. When a resource server answers `use_dpop_nonce`, hand the
//! response headers to [`Dpop::accept_nonce`] and repeat the request once.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use aws_lc_rs::{
    digest::{SHA256, digest},
    signature::{
        ECDSA_P256_SHA256_FIXED_SIGNING, ECDSA_P384_SHA384_FIXED_SIGNING, EcdsaKeyPair,
        EcdsaSigningAlgorithm, Ed25519KeyPair, KeyPair,
    },
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri, header::AUTHORIZATION};
use jsonwebtoken::{Algorithm, EncodingKey, crypto::sign};
use serde::Serialize;
use volga_oauth_core::{
    AuthorizationServerMetadata, PublicJwk,
    auth_scheme::DPOP,
    jwk::{EcCurve, OkpCurve, PublicKey},
};

use crate::{ClientError, JwsAlgorithm, TokenSet, jws, pkce::random_urlsafe};

/// The `DPoP` request header carrying the proof JWT (RFC 9449 Section 4)
pub const DPOP_HEADER: HeaderName = HeaderName::from_static("dpop");

/// The `DPoP-Nonce` header a server supplies the nonce to use in
/// (RFC 9449 Section 8)
pub const DPOP_NONCE_HEADER: HeaderName = HeaderName::from_static("dpop-nonce");

/// The `typ` header of a DPoP proof JWT (RFC 9449 Section 4.2)
pub const PROOF_TYPE: &str = "dpop+jwt";

/// The key a client binds its tokens to, and the nonces the servers it
/// talks to have handed out
///
/// Cloning shares both: a clone signs with the same key and sees the same
/// nonces, so an [`OAuthClient`](crate::OAuthClient) and the code making
/// resource requests with its tokens stay in step.
///
/// The usual lifetime is one per session, not one per process:
/// [`generate`](Self::generate) mints a throwaway key, and losing it costs
/// nothing beyond the tokens bound to it - which cannot be used without it
/// anyway.
///
/// # Example
/// ```no_run
/// # use volga_oauth_client::{ClientError, Dpop, OAuthClient};
/// # fn run() -> Result<(), ClientError> {
/// let dpop = Dpop::generate()?;
/// let client = OAuthClient::new("my-client").with_dpop(dpop.clone());
///
/// // the same key protects the resource requests made with its tokens
/// let jkt = dpop.thumbprint();
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Dpop {
    inner: Arc<Inner>,
}

struct Inner {
    key: EncodingKey,
    alg: Algorithm,
    algorithm: JwsAlgorithm,
    jwk: PublicJwk,
    /// The RFC 7638 thumbprint of `jwk` - the `jkt` an authorization
    /// server binds the issued token to
    thumbprint: String,
    /// The proof's JOSE header, base64url-encoded once: it is the same for
    /// every proof this key signs
    header: String,
    /// The last nonce each server handed out, by origin (RFC 9449
    /// Section 8 scopes a nonce to the server that issued it)
    nonces: Mutex<HashMap<String, String>>,
}

impl Dpop {
    /// Generates a throwaway `ES256` key
    ///
    /// The algorithm every DPoP implementation supports, and the one to
    /// use unless a server says otherwise.
    pub fn generate() -> Result<Self, ClientError> {
        Self::generate_with(JwsAlgorithm::ES256)
    }

    /// Generates a throwaway key for `algorithm`
    ///
    /// `ES256`, `ES384` and `EdDSA` can be generated. Anything else -
    /// symmetric algorithms, which prove nothing about who signed, and the
    /// RSA family, whose key generation is far too slow to do per session -
    /// has to be loaded with [`from_pem`](Self::from_pem) instead.
    pub fn generate_with(algorithm: JwsAlgorithm) -> Result<Self, ClientError> {
        let generated = match algorithm {
            JwsAlgorithm::ES256 => generate_ec(&ECDSA_P256_SHA256_FIXED_SIGNING, EcCurve::P256),
            JwsAlgorithm::ES384 => generate_ec(&ECDSA_P384_SHA384_FIXED_SIGNING, EcCurve::P384),
            JwsAlgorithm::EdDSA => generate_ed(),
            other => {
                return Err(ClientError::signing(format!(
                    "{other} keys are not generated here; supply one with Dpop::from_pem"
                )));
            }
        }?;

        let (der, material) = generated;
        let (key, alg) = jws::from_der(&der, algorithm)?;
        Self::new(key, alg, algorithm, PublicJwk::new(material))
    }

    /// Adopts a PEM-encoded private key, with `public_jwk` holding its
    /// public half
    ///
    /// For a key that outlives the process - one whose thumbprint a
    /// resource has been told about, say. The public half is supplied
    /// rather than derived: this crate signs, it does not recover public
    /// keys from private ones, and [`PublicJwk`] cannot carry private
    /// material by accident.
    ///
    /// Fails with [`ClientError::Signing`] when the key does not parse, or
    /// when `public_jwk` is not the public half of it - a proof whose
    /// header advertises a key that did not sign it verifies nowhere, and
    /// the pairing is checked here rather than left to fail remotely on
    /// every request as an `invalid_dpop_proof`.
    pub fn from_pem(
        pem: &[u8],
        algorithm: JwsAlgorithm,
        public_jwk: PublicJwk,
    ) -> Result<Self, ClientError> {
        let (key, alg) = jws::from_pem(pem, algorithm)?;
        Self::new(key, alg, algorithm, public_jwk)
    }

    /// Reads the PEM file at `path` as the signing key, otherwise like
    /// [`from_pem`](Self::from_pem)
    pub fn from_pem_file(
        path: impl AsRef<std::path::Path>,
        algorithm: JwsAlgorithm,
        public_jwk: PublicJwk,
    ) -> Result<Self, ClientError> {
        Self::from_pem(&jws::read_pem_file(path.as_ref())?, algorithm, public_jwk)
    }

    fn new(
        key: EncodingKey,
        alg: Algorithm,
        algorithm: JwsAlgorithm,
        jwk: PublicJwk,
    ) -> Result<Self, ClientError> {
        if !jwk.key().supports(algorithm) {
            return Err(ClientError::signing(format!(
                "the public JWK is {} key material, which cannot carry the {algorithm} the \
                 proofs are signed with",
                jwk.key().key_type()
            )));
        }

        // a document a caller may publish should say what it signs; the
        // check above is exactly the one `with_algorithm` performs, so this
        // cannot fail
        let jwk = jwk
            .with_algorithm(algorithm)
            .expect("the JWK was checked against this algorithm");

        // the thumbprint input is the canonical public JWK (RFC 7638
        // Section 3.1): the same rendering serves as the proof header's
        // `jwk` and as the `jkt` the token is bound to, so the two agree by
        // construction
        let canonical = jwk.thumbprint_input();
        ensure_halves_match(&key, alg, &canonical)?;

        let thumbprint = base64url_sha256(canonical.as_bytes());

        // every proof this key signs carries the same JOSE header, so it is
        // encoded once here rather than per request
        let header = URL_SAFE_NO_PAD.encode(format!(
            r#"{{"typ":"{PROOF_TYPE}","alg":"{algorithm}","jwk":{canonical}}}"#
        ));

        Ok(Self {
            inner: Arc::new(Inner {
                key,
                alg,
                algorithm,
                jwk,
                thumbprint,
                header,
                nonces: Mutex::new(HashMap::new()),
            }),
        })
    }

    /// Returns the algorithm proofs are signed with
    #[inline]
    pub fn algorithm(&self) -> JwsAlgorithm {
        self.inner.algorithm
    }

    /// Returns the public key every proof carries in its `jwk` header
    #[inline]
    pub fn public_jwk(&self) -> &PublicJwk {
        &self.inner.jwk
    }

    /// Returns the RFC 7638 thumbprint of the public key - the `jkt` an
    /// authorization server binds the issued access token to
    /// (RFC 9449 Section 6)
    ///
    /// A resource server compares it against the `cnf.jkt` of the token it
    /// was presented; a client that has to name its key out of band (a
    /// pre-registered thumbprint, a log line tying a token to a session)
    /// names it with this.
    #[inline]
    pub fn thumbprint(&self) -> &str {
        &self.inner.thumbprint
    }

    /// Starts a proof for `method` on `url`
    ///
    /// The proof carries the nonce last remembered for that server, if
    /// any. Add the access token being presented with
    /// [`with_access_token`](DpopProof::with_access_token) - RFC 9449
    /// Section 4.2 requires the `ath` claim on every request that presents
    /// one - and [`sign`](DpopProof::sign) to render it.
    ///
    /// ```no_run
    /// # use http::Method;
    /// # use volga_oauth_client::{ClientError, Dpop};
    /// # fn run(dpop: &Dpop, access_token: &str) -> Result<(), ClientError> {
    /// let proof = dpop
    ///     .proof(&Method::POST, "https://api.example.com/orders")
    ///     .with_access_token(access_token)
    ///     .sign()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn proof<'a>(&'a self, method: &'a Method, url: &'a str) -> DpopProof<'a> {
        DpopProof {
            dpop: self,
            method,
            url,
            access_token: None,
            nonce: None,
        }
    }

    /// Fills in the headers presenting `tokens` at `url`: the
    /// `Authorization` credential and the `DPoP` proof covering it
    ///
    /// The caller sends the request; this only prepares it. Existing
    /// values for either header are replaced.
    ///
    /// The proof carries the nonce last remembered for that server; use
    /// [`authorize_with_nonce`](Self::authorize_with_nonce) to answer a
    /// refusal with the nonce it demanded.
    ///
    /// Fails with [`ClientError::Validation`] when `tokens` is not
    /// DPoP-bound - a `Bearer` token presented under this scheme is
    /// refused by any server, and presenting it as `Bearer` instead would
    /// silently give up the binding this key exists for.
    pub fn authorize(
        &self,
        headers: &mut HeaderMap,
        method: &Method,
        url: &str,
        tokens: &TokenSet,
    ) -> Result<(), ClientError> {
        self.authorize_inner(headers, method, url, tokens, None)
    }

    /// [`authorize`](Self::authorize) for the retry of a request a server
    /// refused with `use_dpop_nonce`: the proof carries exactly `nonce`
    ///
    /// The nonce a retry must answer with is the one *that* response
    /// demanded (see [`accept_nonce`](Self::accept_nonce)), which is not
    /// necessarily what the shared state holds by the time the proof is
    /// signed - a concurrent request to the same origin may have been
    /// handed a different one in between.
    pub fn authorize_with_nonce(
        &self,
        headers: &mut HeaderMap,
        method: &Method,
        url: &str,
        tokens: &TokenSet,
        nonce: &str,
    ) -> Result<(), ClientError> {
        self.authorize_inner(headers, method, url, tokens, Some(nonce))
    }

    fn authorize_inner(
        &self,
        headers: &mut HeaderMap,
        method: &Method,
        url: &str,
        tokens: &TokenSet,
        nonce: Option<&str>,
    ) -> Result<(), ClientError> {
        if !tokens.is_dpop() {
            return Err(ClientError::validation(format!(
                "the access token is of type '{}', not DPoP; it is not bound to this key",
                tokens.token_type
            )));
        }

        let credential = HeaderValue::from_str(&format!("{DPOP} {}", tokens.access_token))
            .map_err(|_| ClientError::validation("the access token is not a valid header value"))?;

        let mut proof = self
            .proof(method, url)
            .with_access_token(&tokens.access_token);
        if let Some(nonce) = nonce {
            proof = proof.with_nonce(nonce);
        }
        let proof = proof.sign()?;

        headers.insert(AUTHORIZATION, credential);
        headers.insert(DPOP_HEADER, proof_header(proof)?);
        Ok(())
    }

    /// Returns the nonce last remembered for the server serving `url`
    pub fn nonce(&self, url: &str) -> Option<String> {
        let origin = origin_of(url);
        self.lock().get(&origin).cloned()
    }

    /// Remembers `nonce` as the one to use for the server serving `url`
    ///
    /// Returns whether it differs from the one already held. That is a fact
    /// about this shared state, not about any one request - see
    /// [`accept_nonce`](Self::accept_nonce) for what does decide a retry.
    pub fn remember_nonce(&self, url: &str, nonce: impl Into<String>) -> bool {
        let (origin, nonce) = (origin_of(url), nonce.into());
        match self.lock().insert(origin, nonce.clone()) {
            Some(previous) => previous != nonce,
            None => true,
        }
    }

    /// Adopts the `DPoP-Nonce` of a response from the server serving
    /// `url`, if it carries one (RFC 9449 Section 8)
    ///
    /// A server may supply a nonce with any response, not only with the
    /// `use_dpop_nonce` refusal that demands a retry, so this is worth
    /// calling on every response - including the answer to a retry, which
    /// otherwise costs the next request a round trip to be told again.
    ///
    /// Returns the nonce the response carried, if any - what the server
    /// demanded of *this* request, which is what a retry has to answer
    /// with. It is deliberately not "whether the stored nonce changed":
    /// under concurrency a request to the same origin may have stored this
    /// very nonce first, and treating that as "nothing new" would abandon a
    /// request the server was willing to serve.
    ///
    /// Repeat a refused request once, when the nonce it demands is not the
    /// one that request carried - read [`nonce`](Self::nonce) before
    /// signing to know what that was, and hand the demanded one to
    /// [`authorize_with_nonce`](Self::authorize_with_nonce) so the retry
    /// carries it whatever the shared state has moved on to:
    ///
    /// ```no_run
    /// # use http::{HeaderMap, Method};
    /// # use volga_oauth_client::{ClientError, Dpop, TokenSet};
    /// # fn run(dpop: &Dpop, url: &str, tokens: &TokenSet) -> Result<(), ClientError> {
    /// let sent = dpop.nonce(url);
    /// let mut headers = HeaderMap::new();
    /// dpop.authorize(&mut headers, &Method::GET, url, tokens)?;
    ///
    /// // ...send the request; then, given a `use_dpop_nonce` refusal:
    /// # let response_headers = HeaderMap::new();
    /// if let Some(demanded) = dpop.accept_nonce(url, &response_headers)
    ///     && Some(demanded.as_str()) != sent.as_deref()
    /// {
    ///     dpop.authorize_with_nonce(&mut headers, &Method::GET, url, tokens, &demanded)?;
    ///     // ...and send it once more
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn accept_nonce(&self, url: &str, headers: &HeaderMap) -> Option<String> {
        let nonce = headers
            .get(DPOP_NONCE_HEADER)
            .and_then(|nonce| nonce.to_str().ok())
            .filter(|nonce| !nonce.is_empty())?
            .to_owned();

        self.remember_nonce(url, nonce.clone());
        Some(nonce)
    }

    /// Refuses an algorithm the authorization server does not accept for
    /// proofs (RFC 9449 Section 5.1).
    ///
    /// A server that advertises nothing is not second-guessed - the field
    /// is optional, and an omitted one says nothing about what is
    /// accepted.
    pub(crate) fn ensure_supported(
        &self,
        metadata: &AuthorizationServerMetadata,
    ) -> Result<(), ClientError> {
        let advertised = &metadata.dpop_signing_alg_values_supported;
        if advertised.is_empty()
            || advertised
                .iter()
                .any(|alg| alg == self.algorithm().as_str())
        {
            return Ok(());
        }

        Err(ClientError::validation(format!(
            "authorization server does not accept {} for DPoP proofs; it advertises \
             {advertised:?}",
            self.algorithm()
        )))
    }

    /// The nonce map, recovered from a poisoned lock.
    ///
    /// A panic while holding it leaves a `HashMap` of nonces - public
    /// values a server handed out, not an invariant that can be broken -
    /// so the map is kept rather than propagating the panic to every
    /// subsequent request.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.inner
            .nonces
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }
}

impl std::fmt::Debug for Dpop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the private key is a credential; the public half and the
        // thumbprint are meant to be seen, and a nonce is not a secret but
        // is noise
        f.debug_struct("Dpop")
            .field("key", &"[redacted]")
            .field("algorithm", &self.inner.algorithm)
            .field("jwk", &self.inner.jwk)
            .field("thumbprint", &self.inner.thumbprint)
            .finish_non_exhaustive()
    }
}

/// A proof under construction, created by [`Dpop::proof`]
#[must_use = "a proof does nothing until `sign` is called"]
pub struct DpopProof<'a> {
    dpop: &'a Dpop,
    method: &'a Method,
    url: &'a str,
    access_token: Option<&'a str>,
    nonce: Option<&'a str>,
}

impl<'a> DpopProof<'a> {
    /// Binds the proof to the access token the request presents, through
    /// the `ath` claim (RFC 9449 Section 4.2)
    ///
    /// Required on every request that carries a token: without it the
    /// proof covers the method and URL alone, and could be replayed
    /// alongside a different token by whoever intercepts it.
    pub fn with_access_token(mut self, access_token: &'a str) -> Self {
        self.access_token = Some(access_token);
        self
    }

    /// Overrides the nonce, which otherwise comes from what the target
    /// server last handed out (see [`Dpop::accept_nonce`])
    pub fn with_nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Signs the proof
    ///
    /// Fails with [`ClientError::Validation`] when `url` is not an
    /// absolute URL - `htu` identifies the target and cannot be resolved
    /// against anything here - and with [`ClientError::Signing`] when the
    /// key cannot produce the signature.
    pub fn sign(self) -> Result<String, ClientError> {
        let inner = &self.dpop.inner;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ClientError::validation("system clock is set before the Unix epoch"))?
            .as_secs();

        let held;
        let nonce = match self.nonce {
            Some(nonce) => Some(nonce),
            None => {
                held = self.dpop.nonce(self.url);
                held.as_deref()
            }
        };

        let claims = ProofClaims {
            jti: random_urlsafe(16),
            htm: self.method.as_str(),
            htu: &http_target_uri(self.url)?,
            iat: now,
            ath: self
                .access_token
                .map(|token| base64url_sha256(token.as_bytes())),
            nonce,
        };

        let claims = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);

        // header and claims are already encoded; the signing input is the
        // proof itself up to the second separator, so it is built once and
        // completed in place
        let mut proof = String::with_capacity(inner.header.len() + claims.len() + 130);
        proof.push_str(&inner.header);
        proof.push('.');
        proof.push_str(&claims);

        let signature =
            sign(proof.as_bytes(), &inner.key, inner.alg).map_err(ClientError::signing)?;
        proof.push('.');
        proof.push_str(&signature);

        Ok(proof)
    }
}

impl std::fmt::Debug for DpopProof<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the access token and the nonce are not shown - only whether the
        // proof carries them
        f.debug_struct("DpopProof")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("access_token", &self.access_token.map(|_| "[redacted]"))
            .field("nonce", &self.nonce.map(|_| "[set]"))
            .finish_non_exhaustive()
    }
}

/// The claims of a DPoP proof JWT (RFC 9449 Section 4.2).
#[derive(Serialize)]
struct ProofClaims<'a> {
    jti: String,
    htm: &'a str,
    htu: &'a str,
    iat: u64,
    /// The hash of the access token the request presents, on the requests
    /// that present one
    #[serde(skip_serializing_if = "Option::is_none")]
    ath: Option<String>,
    /// The nonce the server demanded, when it has demanded one
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<&'a str>,
}

/// Refuses a public half that cannot verify what the private half signs.
///
/// [`PublicKey::supports`] only judges the key *type*: a P-256 JWK from an
/// unrelated key pair passes it. That matters far more here than it does
/// for a published JWK Set - the proof header is the only key a verifier
/// has to go on, so a mismatched pair does not merely publish the wrong
/// document, it makes every request this key ever signs fail remotely with
/// `invalid_dpop_proof`, which names nothing a caller could act on. One
/// signature at construction settles it locally instead.
///
/// `canonical` is the JWK as the proof header will carry it, so what is
/// checked is exactly what verifiers will be given.
fn ensure_halves_match(
    key: &EncodingKey,
    alg: Algorithm,
    canonical: &str,
) -> Result<(), ClientError> {
    let mismatch = || {
        ClientError::signing(
            "the public JWK does not verify what this key signs; the two halves belong to \
             different key pairs",
        )
    };

    let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_str(canonical)?;
    let public = jsonwebtoken::DecodingKey::from_jwk(&jwk).map_err(ClientError::signing)?;

    const PROBE: &[u8] = b"volga-oauth-client dpop key check";
    let signature = sign(PROBE, key, alg).map_err(ClientError::signing)?;

    match jsonwebtoken::crypto::verify(&signature, PROBE, &public, alg) {
        Ok(true) => Ok(()),
        Ok(false) => Err(mismatch()),
        // a verifier that cannot even be built from this pairing is the
        // same failure, reported one step earlier
        Err(_) => Err(mismatch()),
    }
}

/// Renders a signed proof as a header value.
pub(crate) fn proof_header(proof: String) -> Result<HeaderValue, ClientError> {
    HeaderValue::from_maybe_shared(bytes::Bytes::from(proof))
        .map_err(|_| ClientError::signing("the signed proof is not a valid header value"))
}

/// Returns the base64url-encoded SHA-256 of `bytes` - the encoding both
/// the `ath` claim and a JWK thumbprint are carried in.
fn base64url_sha256(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(digest(&SHA256, bytes).as_ref())
}

/// Returns the `htu` of a request: the target URI without its query and
/// fragment (RFC 9449 Section 4.2).
fn http_target_uri(url: &str) -> Result<String, ClientError> {
    let uri: Uri = url
        .parse()
        .map_err(|err| ClientError::validation(format!("invalid URL '{url}': {err}")))?;

    match (uri.scheme_str(), uri.authority()) {
        (Some(scheme), Some(authority)) => Ok(format!("{scheme}://{authority}{}", uri.path())),
        _ => Err(ClientError::validation(format!(
            "'{url}' is not an absolute URL; a DPoP proof has to name the request target"
        ))),
    }
}

/// Returns the origin of `url`, which is what a nonce is scoped to.
///
/// A URL that does not parse is its own origin: it will fail when the
/// request is built, and until then it must not share a nonce with
/// anything else.
fn origin_of(url: &str) -> String {
    match url.parse::<Uri>() {
        Ok(uri) => match (uri.scheme_str(), uri.authority()) {
            (Some(scheme), Some(authority)) => format!("{scheme}://{authority}"),
            _ => url.to_owned(),
        },
        Err(_) => url.to_owned(),
    }
}

/// Generates an ECDSA key pair, returning its PKCS#8 encoding and the
/// public JWK of its uncompressed point.
fn generate_ec(
    signing: &'static EcdsaSigningAlgorithm,
    curve: EcCurve,
) -> Result<(Vec<u8>, PublicKey), ClientError> {
    let pair = EcdsaKeyPair::generate(signing)
        .map_err(|err| ClientError::signing(format!("failed to generate a DPoP key: {err}")))?;

    let der = pair
        .to_pkcs8v1()
        .map_err(|err| ClientError::signing(format!("failed to encode the DPoP key: {err}")))?;

    // the public key is an uncompressed point: 0x04 followed by the two
    // coordinates (SEC 1 Section 2.3.3), which are exactly the `x` and `y`
    // of the JWK
    let Some((0x04, coordinates)) = pair.public_key().as_ref().split_first() else {
        return Err(ClientError::signing(
            "the generated key is not an uncompressed elliptic curve point",
        ));
    };
    let (x, y) = coordinates.split_at(coordinates.len() / 2);

    Ok((
        der.as_ref().to_vec(),
        PublicKey::Ec {
            crv: curve,
            x: URL_SAFE_NO_PAD.encode(x),
            y: URL_SAFE_NO_PAD.encode(y),
        },
    ))
}

/// Generates an Ed25519 key pair, returning its PKCS#8 encoding and the
/// public JWK of its raw public key.
fn generate_ed() -> Result<(Vec<u8>, PublicKey), ClientError> {
    let pair = Ed25519KeyPair::generate()
        .map_err(|err| ClientError::signing(format!("failed to generate a DPoP key: {err}")))?;

    let der = pair
        .to_pkcs8()
        .map_err(|err| ClientError::signing(format!("failed to encode the DPoP key: {err}")))?;

    Ok((
        der.as_ref().to_vec(),
        PublicKey::Okp {
            crv: OkpCurve::Ed25519,
            x: URL_SAFE_NO_PAD.encode(pair.public_key().as_ref()),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// A throwaway P-256 key in PKCS#8 PEM form, and its public half.
    const EC_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgIeRig+AlqV2rBdgt
BzEQ28UAk8/d5l2+4PDfsspynmShRANCAATP07xL4i2PpomWJmZSZZMbQqj4Ybbd
aLozept2OHnD6J7pNTHm12NdaEJ4knzrCkp6pho2EFIQh5cKnqHm+hQw
-----END PRIVATE KEY-----";

    fn public_jwk() -> PublicJwk {
        PublicJwk::new(PublicKey::Ec {
            crv: EcCurve::P256,
            x: "z9O8S-Itj6aJliZmUmWTG0Ko-GG23Wi6M3qbdjh5w-g".into(),
            y: "nuk1MebXY11oQniSfOsKSnqmGjYQUhCHlwqeoeb6FDA".into(),
        })
    }

    fn dpop() -> Dpop {
        Dpop::from_pem(EC_PEM, JwsAlgorithm::ES256, public_jwk()).expect("the test key must parse")
    }

    fn tokens(token_type: &str) -> TokenSet {
        TokenSet {
            access_token: "at".into(),
            token_type: token_type.into(),
            refresh_token: None,
            scope: None,
            id_token: None,
            expires_at: None,
        }
    }

    /// Decodes the header and claims of an unverified proof.
    fn parts(proof: &str) -> (Value, Value) {
        let decode = |part: &str| {
            serde_json::from_slice::<Value>(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
        };
        let mut parts = proof.split('.');
        let header = decode(parts.next().unwrap());
        let claims = decode(parts.next().unwrap());
        assert!(parts.next().is_some(), "the signature part is missing");
        (header, claims)
    }

    /// Verifies a proof the way a server would: the signature against the
    /// key the header publishes, which is the only key it has to go on.
    fn verify(proof: &str, algorithm: Algorithm) -> Value {
        let (header, _) = parts(proof);
        let jwk: jsonwebtoken::jwk::Jwk = serde_json::from_value(header["jwk"].clone())
            .expect("the header must publish a usable JWK");
        let key = jsonwebtoken::DecodingKey::from_jwk(&jwk).unwrap();

        let mut validation = jsonwebtoken::Validation::new(algorithm);
        // a proof carries no `exp` and no `aud`: freshness is `iat` plus
        // the nonce, which is the server's business, not the decoder's
        validation.required_spec_claims.clear();
        validation.validate_exp = false;

        jsonwebtoken::decode::<Value>(proof, &key, &validation)
            .expect("the proof must verify against the key it publishes")
            .claims
    }

    #[test]
    fn it_signs_an_rfc9449_proof() {
        let dpop = dpop();
        let proof = dpop
            .proof(&Method::POST, "https://auth.example.com/token")
            .sign()
            .unwrap();

        let (header, claims) = parts(&proof);
        assert_eq!(header["typ"], "dpop+jwt");
        assert_eq!(header["alg"], "ES256");
        // the public key travels in the header - a DPoP proof names its key
        // by value, never by `kid` as a client assertion does
        assert_eq!(header["jwk"]["kty"], "EC");
        assert_eq!(header["jwk"]["crv"], "P-256");
        assert!(header["jwk"].get("d").is_none());

        assert_eq!(claims["htm"], "POST");
        assert_eq!(claims["htu"], "https://auth.example.com/token");
        assert!(claims["iat"].as_u64().is_some());
        assert!(claims["jti"].as_str().is_some_and(|jti| !jti.is_empty()));
        // nothing is claimed that was not asked for
        assert!(claims.get("ath").is_none() && claims.get("nonce").is_none());

        // ...and it verifies against the key it publishes
        assert_eq!(verify(&proof, Algorithm::ES256)["htm"], "POST");
    }

    #[test]
    fn it_mints_a_fresh_jti_per_proof() {
        let dpop = dpop();
        let proof = |()| {
            dpop.proof(&Method::GET, "https://api.example.com/orders")
                .sign()
                .unwrap()
        };
        assert_ne!(parts(&proof(())).1["jti"], parts(&proof(())).1["jti"]);
    }

    #[test]
    fn it_binds_a_proof_to_the_token_it_presents() {
        // RFC 9449 Section 4.2: `ath` is the base64url SHA-256 of the
        // access token, so a captured proof cannot be replayed with another
        let proof = dpop()
            .proof(&Method::GET, "https://api.example.com/orders")
            .with_access_token("at")
            .sign()
            .unwrap();
        assert_eq!(
            parts(&proof).1["ath"],
            "sda5G2fCr6XjIpiNlGJjjTVN34oe9526mH-BXCK0uu4"
        );
    }

    #[test]
    fn it_strips_the_query_and_fragment_from_htu() {
        // RFC 9449 Section 4.2: `htu` is the target URI without them
        let cases = [
            (
                "https://api.example.com/orders?page=2",
                "https://api.example.com/orders",
            ),
            (
                "https://api.example.com/orders#top",
                "https://api.example.com/orders",
            ),
            ("https://api.example.com", "https://api.example.com/"),
            (
                "https://api.example.com:8443/a/b?x=1#y",
                "https://api.example.com:8443/a/b",
            ),
        ];
        for (url, expected) in cases {
            let proof = dpop().proof(&Method::GET, url).sign().unwrap();
            assert_eq!(parts(&proof).1["htu"], expected, "for {url}");
        }

        // a URL that names no target cannot be proven against
        for url in ["/orders", "not a url", "api.example.com/orders"] {
            assert!(
                matches!(
                    dpop().proof(&Method::GET, url).sign(),
                    Err(ClientError::Validation(_))
                ),
                "'{url}' was accepted"
            );
        }
    }

    #[test]
    fn it_computes_the_rfc7638_thumbprint_of_its_key() {
        // the `jkt` an authorization server binds the token to; it is the
        // digest of exactly the JWK the proof header carries
        let dpop = dpop();
        assert_eq!(
            dpop.thumbprint(),
            "TxWba5K7AwG8Ci-XnSC_4P7XKRGwROMZ8XPvDmYUhYI"
        );

        let (header, _) = parts(
            &dpop
                .proof(&Method::GET, "https://a.example")
                .sign()
                .unwrap(),
        );
        assert_eq!(
            base64url_sha256(serde_json::to_string(&header["jwk"]).unwrap().as_bytes()),
            dpop.thumbprint(),
            "the published key and the thumbprint must describe the same key"
        );
    }

    #[test]
    fn it_carries_the_nonce_of_the_server_it_is_talking_to() {
        let dpop = dpop();
        let auth = "https://auth.example.com/token";
        let api = "https://api.example.com/orders";

        // no nonce until a server hands one out
        assert!(dpop.nonce(auth).is_none());
        assert!(
            parts(&dpop.proof(&Method::POST, auth).sign().unwrap())
                .1
                .get("nonce")
                .is_none()
        );

        let mut headers = HeaderMap::new();
        headers.insert(DPOP_NONCE_HEADER, "n-1".parse().unwrap());
        assert_eq!(dpop.accept_nonce(auth, &headers).as_deref(), Some("n-1"));
        assert_eq!(dpop.nonce(auth).as_deref(), Some("n-1"));

        // ...and it goes into the next proof for that server
        let proof = dpop.proof(&Method::POST, auth).sign().unwrap();
        assert_eq!(parts(&proof).1["nonce"], "n-1");

        // a nonce is scoped to the server that issued it: another one has
        // its own, and must not be sent this one
        assert!(dpop.nonce(api).is_none());
        // ...but the whole origin shares it, whatever the path
        assert_eq!(
            dpop.nonce("https://auth.example.com/introspect").as_deref(),
            Some("n-1")
        );

        // what the response demanded is reported whether or not the shared
        // state already held it: a concurrent request may have stored this
        // very nonce first, and the request that was refused for it still
        // has to answer with it
        assert_eq!(dpop.accept_nonce(auth, &headers).as_deref(), Some("n-1"));
        assert!(!dpop.remember_nonce(auth, "n-1"));

        headers.insert(DPOP_NONCE_HEADER, "n-2".parse().unwrap());
        assert_eq!(dpop.accept_nonce(auth, &headers).as_deref(), Some("n-2"));

        // a response without one (or with an empty one) demands nothing and
        // leaves what is held alone
        assert!(dpop.accept_nonce(auth, &HeaderMap::new()).is_none());
        headers.insert(DPOP_NONCE_HEADER, "".parse().unwrap());
        assert!(dpop.accept_nonce(auth, &headers).is_none());
        assert_eq!(dpop.nonce(auth).as_deref(), Some("n-2"));

        // an explicit nonce wins over the remembered one
        let proof = dpop
            .proof(&Method::POST, auth)
            .with_nonce("n-9")
            .sign()
            .unwrap();
        assert_eq!(parts(&proof).1["nonce"], "n-9");
    }

    #[test]
    fn it_shares_key_and_nonces_across_clones() {
        let dpop = dpop();
        let clone = dpop.clone();
        clone.remember_nonce("https://auth.example.com/token", "n-1");

        assert_eq!(
            dpop.nonce("https://auth.example.com/token").as_deref(),
            Some("n-1")
        );
        assert_eq!(dpop.thumbprint(), clone.thumbprint());
    }

    #[test]
    fn it_fills_in_the_headers_of_a_protected_request() {
        let dpop = dpop();
        let mut headers = HeaderMap::new();
        dpop.authorize(
            &mut headers,
            &Method::GET,
            "https://api.example.com/orders?page=2",
            &tokens(DPOP),
        )
        .unwrap();

        assert_eq!(headers[AUTHORIZATION], "DPoP at");
        let (_, claims) = parts(headers[DPOP_HEADER].to_str().unwrap());
        assert_eq!(claims["htm"], "GET");
        assert_eq!(claims["htu"], "https://api.example.com/orders");
        assert_eq!(claims["ath"], "sda5G2fCr6XjIpiNlGJjjTVN34oe9526mH-BXCK0uu4");

        // the retry counterpart carries exactly the nonce it was handed,
        // whatever the shared state has moved on to in the meantime
        dpop.remember_nonce("https://api.example.com/orders", "stale");
        let mut retry = HeaderMap::new();
        dpop.authorize_with_nonce(
            &mut retry,
            &Method::GET,
            "https://api.example.com/orders",
            &tokens(DPOP),
            "demanded",
        )
        .unwrap();
        assert_eq!(
            parts(retry[DPOP_HEADER].to_str().unwrap()).1["nonce"],
            "demanded"
        );

        // a bearer token is not bound to this key: presenting it here would
        // be refused, and presenting it as `Bearer` would give up the
        // binding silently
        let err = dpop
            .authorize(
                &mut headers,
                &Method::GET,
                "https://api.example.com/orders",
                &tokens("Bearer"),
            )
            .unwrap_err();
        assert!(
            matches!(&err, ClientError::Validation(_)) && err.to_string().contains("Bearer"),
            "got: {err}"
        );
    }

    #[test]
    fn it_checks_the_algorithm_against_the_server_metadata() {
        let dpop = dpop();
        let mut metadata = AuthorizationServerMetadata::new("https://auth.example.com");

        // a server advertising nothing is not second-guessed
        assert!(dpop.ensure_supported(&metadata).is_ok());

        metadata.dpop_signing_alg_values_supported = vec!["RS256".into()];
        let err = dpop.ensure_supported(&metadata).unwrap_err();
        assert!(matches!(&err, ClientError::Validation(reason) if reason.contains("ES256")));

        metadata.dpop_signing_alg_values_supported = vec!["RS256".into(), "ES256".into()];
        assert!(dpop.ensure_supported(&metadata).is_ok());
    }

    #[test]
    fn it_generates_a_key_per_session() {
        for (algorithm, alg) in [
            (JwsAlgorithm::ES256, Algorithm::ES256),
            (JwsAlgorithm::ES384, Algorithm::ES384),
            (JwsAlgorithm::EdDSA, Algorithm::EdDSA),
        ] {
            let dpop = Dpop::generate_with(algorithm).unwrap();
            assert_eq!(dpop.algorithm(), algorithm);
            assert_eq!(dpop.public_jwk().algorithm(), Some(algorithm));

            // the generated key must actually sign, and the public half
            // published in the header must verify what it signed
            let proof = dpop
                .proof(&Method::POST, "https://auth.example.com/token")
                .sign()
                .unwrap();
            assert_eq!(verify(&proof, alg)["htm"], "POST");

            // every session gets its own key
            assert_ne!(
                dpop.thumbprint(),
                Dpop::generate_with(algorithm).unwrap().thumbprint()
            );
        }

        assert_eq!(Dpop::generate().unwrap().algorithm(), JwsAlgorithm::ES256);
    }

    #[test]
    fn it_refuses_keys_it_cannot_generate() {
        // an HMAC secret proves nothing about who signed, and RSA key
        // generation is far too slow to do per session
        for algorithm in [
            JwsAlgorithm::HS256,
            JwsAlgorithm::RS256,
            JwsAlgorithm::PS512,
        ] {
            let err = Dpop::generate_with(algorithm).unwrap_err();
            assert!(matches!(&err, ClientError::Signing(_)), "got: {err}");
        }
    }

    #[test]
    fn it_refuses_a_public_half_that_did_not_sign() {
        // a proof whose header advertises material of another type
        // verifies nowhere
        let rsa = PublicJwk::new(PublicKey::Rsa {
            n: "n".into(),
            e: "AQAB".into(),
        });
        let err = Dpop::from_pem(EC_PEM, JwsAlgorithm::ES256, rsa).unwrap_err();
        assert!(
            matches!(&err, ClientError::Signing(_)) && err.to_string().contains("RSA"),
            "got: {err}"
        );

        // ...and so does a key that does not parse at all
        assert!(matches!(
            Dpop::from_pem(b"not a pem", JwsAlgorithm::ES256, public_jwk()),
            Err(ClientError::Signing(_))
        ));
    }

    #[test]
    fn it_refuses_a_public_half_from_another_key_pair() {
        // the same key type and curve, so nothing about the *shape* of the
        // document is wrong - it is simply not this key. The proof header
        // is the only key a verifier gets, so this would otherwise fail
        // remotely on every single request
        let stranger = Dpop::generate().unwrap().public_jwk().clone();
        let err = Dpop::from_pem(EC_PEM, JwsAlgorithm::ES256, stranger).unwrap_err();
        assert!(
            matches!(&err, ClientError::Signing(_)) && err.to_string().contains("different key"),
            "got: {err}"
        );

        // ...while the matching half goes through
        assert!(Dpop::from_pem(EC_PEM, JwsAlgorithm::ES256, public_jwk()).is_ok());
    }

    #[test]
    fn it_redacts_the_key_in_debug_output() {
        let dpop = dpop();
        let debug = format!("{dpop:?}");
        assert!(debug.contains("[redacted]"));
        assert!(debug.contains("ES256"));

        let proof = dpop
            .proof(&Method::GET, "https://api.example.com")
            .with_access_token("at");
        let debug = format!("{proof:?}");
        assert!(!debug.contains("\"at\""), "{debug}");
        assert!(debug.contains("[redacted]"));
    }
}
