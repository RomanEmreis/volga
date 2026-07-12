//! Optional metadata caching hooks
//!
//! Discovery documents rarely change and are safe to cache; the volga
//! server-side handlers, for instance, serve them with a public one-hour
//! `Cache-Control`. [`MetadataCache`] lets applications plug in whatever
//! storage they already have without this crate shipping a caching layer:
//! eviction, TTL and size limits are entirely the implementor's concern.

use serde_json::Value;

/// Storage hook consulted by
/// [`DiscoveryClient`](crate::DiscoveryClient) before fetching a metadata
/// document and updated after a successful fetch
///
/// Keys are the exact metadata URLs being fetched (derived well-known URLs
/// or the URL advertised in a `WWW-Authenticate` challenge). A `get` hit
/// short-circuits the HTTP request entirely; the cached document still goes
/// through the same deserialization and semantic validation as a fresh
/// response, so a stale or corrupted entry fails loudly rather than
/// silently. `put` only ever receives documents that passed those checks —
/// a malformed or lying response is rejected without touching the cache.
///
/// Implementations must be thread-safe; both methods take `&self`, so
/// interior mutability is required.
pub trait MetadataCache: Send + Sync {
    /// Returns the cached document for `url`, if any
    fn get(&self, url: &str) -> Option<Value>;

    /// Stores the document fetched from `url`
    fn put(&self, url: &str, document: &Value);
}
