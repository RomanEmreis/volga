﻿//! Extractors for uri query

use futures_util::future::{ready, Ready};
use serde::de::DeserializeOwned;

use std::{
    fmt::{self, Display, Formatter},
    ops::{Deref, DerefMut}
};

use hyper::Uri;

use crate::{error::Error, HttpRequest};
use crate::http::endpoints::args::{FromPayload, FromRequestRef, Payload, Source};

/// Wraps typed data extracted from [`Uri`]
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, Query, ok};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Params {
///     name: String,
/// }
///
/// async fn handle(params: Query<Params>) -> HttpResult {
///     ok!("Hello {}", params.name)
/// }
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Query<T>(pub T);

impl<T> Query<T> {
    /// Unwraps the inner `T`
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Display> Display for Query<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: DeserializeOwned> Query<T> {
    /// Parses the string slice into [`Query<T>`]
    #[inline]
    pub(crate) fn from_query_str(query_str: &str) -> Result<Self, Error> {
        serde_urlencoded::from_str::<T>(query_str)
            .map(Query)
            .map_err(QueryError::from)
    }

    /// Parses the request [`Uri`] into [`Query<T>`]
    #[inline]
    pub(crate) fn from_uri(uri: &Uri) -> Result<Self, Error> {
        Self::from_query_str(uri.query().unwrap_or(""))
    }
}

/// Extracts `Uri` query from request into `Query<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned> FromRequestRef for Query<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::from_uri(req.uri())
    }
}

/// Extracts `Uri` query from request parts into `Query<T>`
/// where T is deserializable `struct`
impl<T: DeserializeOwned + Send> FromPayload for Query<T> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Query(uri) = payload {
            ready(Self::from_uri(uri))
        } else {
            unreachable!()
        }
    }

    #[inline]
    fn source() -> Source {
        Source::Query
    }
}

/// Describes errors of query extractor
struct QueryError;
impl QueryError {
    fn from(err: serde::de::value::Error) -> Error {
        Error::client_error(format!("Query parsing error: {}", err))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use hyper::Uri;
    use serde::Deserialize;
    use crate::Query;
    use crate::http::endpoints::args::{FromPayload, Payload};

    #[derive(Deserialize)]
    struct User {
        name: String,
        age: i32
    }

    #[derive(Deserialize)]
    struct OptionalUser {
        name: Option<String>,
        age: Option<i32>
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let uri = "https://www.example.com/api/get?name=John&age=33".parse::<Uri>().unwrap();
        
        let query = Query::<User>::from_payload(Payload::Query(&uri)).await.unwrap();

        assert_eq!(query.name, "John");
        assert_eq!(query.age, 33);
    }

    #[tokio::test]
    async fn it_reads_as_optional_from_payload() {
        let uri = "https://www.example.com/api/get?name=John".parse::<Uri>().unwrap();

        let query = Query::<OptionalUser>::from_payload(Payload::Query(&uri)).await.unwrap();

        assert!(query.age.is_none());
        assert_eq!(query.0.name.unwrap(), "John");
    }

    #[tokio::test]
    async fn it_reads_hash_map_from_payload() {
        let uri = "https://www.example.com/api/get?name=John&age=33".parse::<Uri>().unwrap();

        let query = Query::<HashMap<String, String>>::from_payload(Payload::Query(&uri)).await.unwrap();

        assert_eq!(query.0.get("name").unwrap(), "John");
        assert_eq!(query.0.get("age").unwrap(), "33");
    }
    
    #[test]
    fn it_parses_struct_from_request() {
        let query_str = "name=John&age=33";
        
        let query = Query::<User>::from_query_str(query_str).unwrap();
        
        assert_eq!(query.0.name, "John");
        assert_eq!(query.0.age, 33);
    }

    #[test]
    fn it_parses_struct_with_option_from_request() {
        let query_str = "name=John";

        let query = Query::<OptionalUser>::from_query_str(query_str).unwrap();

        assert_eq!(query.0.name.unwrap(), "John");
        assert!(query.0.age.is_none());
    }

    #[test]
    fn it_parses_empty_query_into_struct_with_option_from_request() {
        let query_str = "";

        let query = Query::<OptionalUser>::from_query_str(query_str).unwrap();

        assert!(query.0.name.is_none());
        assert!(query.0.age.is_none());
    }

    #[test]
    fn it_parses_struct_from_uri() {
        let uri = "https://www.example.com/api/get?name=John&age=33".parse::<Uri>().unwrap();

        let query = Query::<User>::from_uri(&uri).unwrap();

        assert_eq!(query.0.name, "John");
        assert_eq!(query.0.age, 33);
    }

    #[test]
    fn it_parses_struct_with_option_from_uri() {
        let uri = "https://www.example.com/api/get?name=John".parse::<Uri>().unwrap();

        let query = Query::<OptionalUser>::from_uri(&uri).unwrap();

        assert_eq!(query.0.name.unwrap(), "John");
        assert!(query.0.age.is_none());
    }

    #[test]
    fn it_parses_uri_without_query_into_struct_with_option_from_uri() {
        let uri = "https://www.example.com/api/get".parse::<Uri>().unwrap();

        let query = Query::<OptionalUser>::from_uri(&uri).unwrap();

        assert!(query.0.name.is_none());
        assert!(query.0.age.is_none());
    }

    #[test]
    fn it_parses_hash_map_from_request() {
        let query_str = "name=John&age=33";

        let query = Query::<HashMap<String, String>>::from_query_str(query_str).unwrap();

        assert_eq!(query.0.get("name").unwrap(), "John");
        assert_eq!(query.0.get("age").unwrap(), "33");
    }

    #[test]
    fn it_parses_hash_map_from_uri() {
        let uri = "https://www.example.com/api/get?name=John&age=33".parse::<Uri>().unwrap();

        let query = Query::<HashMap<String, String>>::from_uri(&uri).unwrap();

        assert_eq!(query.0.get("name").unwrap(), "John");
        assert_eq!(query.0.get("age").unwrap(), "33");
    }
}