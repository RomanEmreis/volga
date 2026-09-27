//! URL path arguments utilities

use super::DEFAULT_DEPTH;
use crate::error::Error;
use smallvec::SmallVec;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;

const QUERY_SEPARATOR: char = '&';
const QUERY_KEY_VALUE_SEPARATOR: char = '=';
const FORM_SPACE: char = '+';
const FORM_ESCAPE: char = '%';

/// The characters a form decoder reads as something other than themselves, and that a
/// decoded path argument carries as they are
const FORM_ONLY: [char; 3] = [QUERY_SEPARATOR, FORM_SPACE, FORM_ESCAPE];

/// The path arguments a route matched, in the order its pattern declares them
///
/// What [`FromPathArgs`](crate::http::endpoints::args::FromPathArgs) reads from. It is built
/// by the router and cannot be constructed outside of volga.
pub struct PathArgs {
    args: SmallVec<[PathArg; DEFAULT_DEPTH]>,
    encoded: OnceLock<String>,
}

/// A single matched path argument
///
/// What [`FromPathArg`](crate::http::endpoints::args::FromPathArg) reads from. It is built
/// by the router and cannot be constructed outside of volga.
#[derive(Debug, Clone)]
pub struct PathArg {
    /// Argument name
    pub(crate) name: Arc<str>,

    /// Argument value, percent-decoded
    pub(crate) value: Box<str>,
}

impl PathArg {
    /// Returns the name the route's pattern gives this argument.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the value, percent-decoded: `%20` is read as a space, `%25` as a `%` and
    /// `%2F` as a `/`, while a `+` is read as it is written.
    ///
    /// A path whose escapes are malformed, or do not decode to UTF-8, is answered `400`
    /// before any route is looked up, so a value is always the text the client meant.
    #[inline]
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Parses the value into `T` through [`FromStr`].
    ///
    /// # Errors
    /// A value that does not parse answers `400`, as a built-in path parameter does.
    ///
    /// # Example
    /// ```no_run
    /// use volga::error::Error;
    /// use volga::http::endpoints::args::{FromPathArg, PathArg};
    ///
    /// struct OrderId(u64);
    ///
    /// impl FromPathArg for OrderId {
    ///     fn from_path_arg(arg: &PathArg) -> Result<Self, Error> {
    ///         arg.parse().map(OrderId)
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn parse<T: FromStr>(&self) -> Result<T, Error> {
        self.value.parse().map_err(|_| {
            Error::client_error(format!(
                "Path parsing error: argument `{}` type mismatch",
                self.name
            ))
        })
    }
}

impl PathArgs {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            args: SmallVec::new(),
            encoded: OnceLock::new(),
        }
    }

    /// Returns an iterator over the args, in the order the route declares them.
    #[inline]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &PathArg> {
        self.args.iter()
    }

    /// Returns the number of args.
    #[inline]
    pub fn len(&self) -> usize {
        self.args.len()
    }

    /// Returns `true` if the route has no args.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.args.is_empty()
    }

    /// Returns the first arg, or `None` if it is empty.
    #[inline]
    #[allow(unused)]
    pub(crate) fn first(&self) -> Option<&PathArg> {
        self.args.first()
    }

    /// Relabels the args with the names the matched endpoint's own pattern was written
    /// with.
    ///
    /// The tree binds a parameter under the name of whichever route reached its position
    /// first, and that route is not always the one answering: two verbs may name one
    /// position two things. Extractors reading a parameter by name - `NamedPath<T>` and
    /// anything else going through [`PathArgs::encoded`] - read the answering route's.
    #[inline]
    pub(crate) fn rename(&mut self, names: &[Arc<str>]) {
        for (arg, name) in self.args.iter_mut().zip(names) {
            arg.name = Arc::clone(name);
        }
        let _ = self.encoded.take();
    }

    /// Append an item to the args vector.
    #[inline]
    pub(crate) fn push(&mut self, arg: PathArg) {
        self.args.push(arg);
        let _ = self.encoded.take();
    }

    /// Removes the last arg, the one read furthest into the path.
    #[inline]
    pub(crate) fn pop(&mut self) -> Option<PathArg> {
        let _ = self.encoded.take();
        self.args.pop()
    }

    /// Restures a query string of this route
    #[inline]
    pub(crate) fn encoded(&self) -> Result<&str, Error> {
        if self.args.is_empty() {
            return Err(Error::client_error("Path parsing error: missing arguments"));
        }

        let value = self.encoded.get_or_init(|| encode(&self.args));

        Ok(value.as_str())
    }

    /// Splits [`PathArgs`] into parts
    #[inline]
    pub(crate) fn into_parts(self) -> (SmallVec<[PathArg; DEFAULT_DEPTH]>, Option<String>) {
        let cached = self.encoded.into_inner();
        (self.args, cached)
    }

    /// Creates [`PathArgs`] from parts
    #[inline]
    pub(crate) fn from_parts(
        args: SmallVec<[PathArg; DEFAULT_DEPTH]>,
        cached: Option<String>,
    ) -> Self {
        let encoded = OnceLock::new();
        if let Some(value) = cached {
            let _ = encoded.set(value);
        }
        Self { args, encoded }
    }
}

impl fmt::Debug for PathArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathArgs")
            .field("args", &self.args.as_slice())
            .finish_non_exhaustive()
    }
}

impl Clone for PathArgs {
    #[inline]
    fn clone(&self) -> Self {
        let encoded = OnceLock::new();
        if let Some(value) = self.encoded.get() {
            let _ = encoded.set(value.clone());
        }
        Self {
            args: self.args.clone(),
            encoded,
        }
    }
}

#[cfg(test)]
impl FromIterator<PathArg> for PathArgs {
    #[inline]
    fn from_iter<T: IntoIterator<Item = PathArg>>(iter: T) -> Self {
        let mut args = PathArgs::new();
        for arg in iter {
            args.args.push(arg);
        }
        args
    }
}

#[cfg(test)]
impl From<SmallVec<[PathArg; DEFAULT_DEPTH]>> for PathArgs {
    #[inline]
    fn from(args: SmallVec<[PathArg; DEFAULT_DEPTH]>) -> Self {
        Self {
            args,
            encoded: OnceLock::new(),
        }
    }
}

#[cfg(test)]
impl IntoIterator for PathArgs {
    type Item = PathArg;
    type IntoIter = smallvec::IntoIter<[PathArg; DEFAULT_DEPTH]>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.args.into_iter()
    }
}

#[inline]
fn encode(args: &SmallVec<[PathArg; DEFAULT_DEPTH]>) -> String {
    let capacity = args
        .iter()
        .fold(0, |acc, arg| acc + arg.name.len() + arg.value.len() + 1)
        + args.len().saturating_sub(1);

    let mut result = String::with_capacity(capacity);
    let mut iter = args.iter();

    if let Some(first) = iter.next() {
        result.push_str(first.name.as_ref());
        result.push(QUERY_KEY_VALUE_SEPARATOR);
        push_value(&mut result, first.value.as_ref());
        for s in iter {
            result.push(QUERY_SEPARATOR);
            result.push_str(s.name.as_ref());
            result.push(QUERY_KEY_VALUE_SEPARATOR);
            push_value(&mut result, s.value.as_ref());
        }
    }

    result
}

/// Appends a value to the encoded args, escaping what form decoding reads differently from
/// the value itself.
///
/// The encoded args are read back as a form, and the value has been percent-decoded
/// already, so three characters mean something there that they do not mean in it:
///
/// - `&` separates two pairs, so a value carrying one would end its own pair early and
///   start a pair the route never bound: `/files/a&admin=true` would read as `path=a` and
///   `admin=true`.
/// - `+` is a space, so `/files/C++` would read as `C  `.
/// - `%` starts an escape, so `/p/100%25`, decoded to `100%`, would be decoded a second time
///   and read as an error, and `/p/%2520`, decoded to `%20`, as a space.
///
/// A path segment may carry any of them, and a catch-all value carries whatever the rest of
/// the path does. Every other character is read back as it is written.
#[inline]
fn push_value(result: &mut String, value: &str) {
    let mut rest = value;
    while let Some(at) = rest.find(FORM_ONLY) {
        result.push_str(&rest[..at]);
        result.push_str(match rest.as_bytes()[at] {
            b'&' => "%26",
            b'+' => "%2B",
            _ => "%25",
        });
        rest = &rest[at + 1..];
    }
    result.push_str(rest);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(name: &str, value: &str) -> PathArg {
        PathArg {
            name: name.into(),
            value: value.into(),
        }
    }

    #[test]
    fn it_makes_query_str() {
        let args: PathArgs = smallvec::smallvec![
            PathArg {
                name: "id".into(),
                value: "123".into()
            },
            PathArg {
                name: "name".into(),
                value: "John".into()
            }
        ]
        .into();

        let query_str = args.encoded().unwrap();
        assert_eq!(query_str, "id=123&name=John");
    }

    #[test]
    fn it_makes_query_str_empty() {
        let args: PathArgs = smallvec::smallvec![].into();

        let result = args.encoded();
        assert!(result.is_err());
    }

    #[test]
    fn it_makes_query_str_single_arg() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123")].into();

        let query_str = args.encoded().unwrap();
        assert_eq!(query_str, "id=123");
    }

    #[test]
    fn it_makes_query_str_with_empty_name_or_value() {
        let args: PathArgs = smallvec::smallvec![arg("", "123"), arg("name", "")].into();

        let query_str = args.encoded().unwrap();
        assert_eq!(query_str, "=123&name=");
    }

    #[test]
    fn it_escapes_what_a_form_reads_differently_from_a_path() {
        let args: PathArgs =
            smallvec::smallvec![arg("path", "a&admin=true/C++/b&"), arg("id", "7")].into();

        let query_str = args.encoded().unwrap();
        assert_eq!(query_str, "path=a%26admin=true/C%2B%2B/b%26&id=7");

        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Params {
            path: String,
            id: u32,
            admin: Option<bool>,
        }

        let params: Params = serde_urlencoded::from_str(query_str).unwrap();
        assert_eq!(
            params,
            Params {
                path: "a&admin=true/C++/b&".into(),
                id: 7,
                admin: None
            }
        );
    }

    /// A value is decoded by the router already, so a `%` in it is a percent sign and is
    /// read back as one rather than decoded again
    #[test]
    fn it_reads_a_percent_sign_back_as_it_is() {
        let args: PathArgs = smallvec::smallvec![
            arg("name", "John Doe+"),
            arg("rate", "100%"),
            arg("raw", "%20")
        ]
        .into();

        let query_str = args.encoded().unwrap();
        assert_eq!(query_str, "name=John Doe%2B&rate=100%25&raw=%2520");

        let decoded: std::collections::HashMap<String, String> =
            serde_urlencoded::from_str(query_str).unwrap();
        assert_eq!(decoded["name"], "John Doe+");
        assert_eq!(decoded["rate"], "100%");
        assert_eq!(decoded["raw"], "%20");
    }

    /// Anything else a decoded value can carry is read back as it is written
    #[test]
    fn it_reads_any_decoded_value_back_as_it_is() {
        let values = ["caf\u{e9}", "a/b", "a=b", "\u{0}", " ", "?#[]", "a%2Fb&c+d"];
        let args: PathArgs = values
            .iter()
            .enumerate()
            .map(|(i, value)| arg(&format!("v{i}"), value))
            .collect();

        let decoded: std::collections::HashMap<String, String> =
            serde_urlencoded::from_str(args.encoded().unwrap()).unwrap();

        for (i, value) in values.iter().enumerate() {
            assert_eq!(decoded[&format!("v{i}")], *value);
        }
    }

    #[test]
    fn push_invalidates_cached_query_str() {
        let mut args: PathArgs = smallvec::smallvec![arg("id", "123")].into();

        // no cache yet
        let (parts, cached) = args.clone().into_parts();
        assert_eq!(parts.len(), 1);
        assert!(cached.is_none());

        // warm cache
        assert_eq!(args.encoded().unwrap(), "id=123");

        // cache exists now
        let (_parts, cached) = args.clone().into_parts();
        assert_eq!(cached.as_deref(), Some("id=123"));

        // push => must drop cache
        args.push(arg("name", "John"));

        let (_parts, cached) = args.clone().into_parts();
        assert!(cached.is_none());

        // and recomputes correctly
        assert_eq!(args.encoded().unwrap(), "id=123&name=John");
    }

    #[test]
    fn query_str_is_cached_between_calls_without_mutations() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123"), arg("name", "John")].into();

        let q1 = args.encoded().unwrap();
        let q2 = args.encoded().unwrap();

        assert_eq!(q1, "id=123&name=John");
        assert_eq!(q2, "id=123&name=John");

        // Cached string lives inside OnceLock, so the &str should point to the same allocation.
        assert_eq!(q1.as_ptr(), q2.as_ptr());
        assert_eq!(q1.len(), q2.len());
    }

    #[test]
    fn clone_clones_cache_when_initialized() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123"), arg("name", "John")].into();

        // warm cache in original
        assert_eq!(args.encoded().unwrap(), "id=123&name=John");
        assert_eq!(
            args.encoded.get().map(|s| s.as_str()),
            Some("id=123&name=John")
        );

        // clone should carry the cache
        let cloned = args.clone();
        assert_eq!(
            cloned.encoded.get().map(|s| s.as_str()),
            Some("id=123&name=John")
        );

        // still correct
        assert_eq!(cloned.encoded().unwrap(), "id=123&name=John");
    }

    #[test]
    fn clone_has_no_cache_when_original_not_initialized() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123")].into();

        // original cache not initialized
        assert!(args.encoded.get().is_none());

        let cloned = args.clone();
        assert!(cloned.encoded.get().is_none());

        // but can compute normally
        assert_eq!(cloned.encoded().unwrap(), "id=123");
    }

    #[test]
    fn into_parts_returns_args_and_cached_when_present() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123"), arg("name", "John")].into();

        // warm cache
        let _ = args.encoded().unwrap();

        let (parts, cached) = args.into_parts();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name.as_ref(), "id");
        assert_eq!(parts[0].value.as_ref(), "123");
        assert_eq!(parts[1].name.as_ref(), "name");
        assert_eq!(parts[1].value.as_ref(), "John");

        assert_eq!(cached.as_deref(), Some("id=123&name=John"));
    }

    #[test]
    fn into_parts_returns_none_cached_if_never_computed() {
        let args: PathArgs = smallvec::smallvec![arg("id", "123")].into();

        let (_parts, cached) = args.into_parts();
        assert!(cached.is_none());
    }

    #[test]
    fn from_parts_restores_cached_query_str() {
        let parts: SmallVec<[PathArg; DEFAULT_DEPTH]> =
            smallvec::smallvec![arg("id", "123"), arg("name", "John")];

        let args = PathArgs::from_parts(parts, Some("id=123&name=John".to_string()));

        // Should return exactly the cached value (not recomputed).
        let q1 = args.encoded().unwrap();
        let q2 = args.encoded().unwrap();

        assert_eq!(q1, "id=123&name=John");
        assert_eq!(q1.as_ptr(), q2.as_ptr());
    }

    #[test]
    fn from_parts_with_none_cached_computes_on_demand() {
        let parts: SmallVec<[PathArg; DEFAULT_DEPTH]> =
            smallvec::smallvec![arg("id", "123"), arg("name", "John")];

        let args = PathArgs::from_parts(parts, None);

        let q1 = args.encoded().unwrap();
        let q2 = args.encoded().unwrap();

        assert_eq!(q1, "id=123&name=John");
        assert_eq!(q1.as_ptr(), q2.as_ptr()); // computed once then cached
    }

    #[test]
    fn from_iterator_preserves_order() {
        let items = vec![arg("a", "1"), arg("b", "2"), arg("c", "3")];
        let args: PathArgs = items.into_iter().collect();

        let q = args.encoded().unwrap();
        assert_eq!(q, "a=1&b=2&c=3");
    }

    #[test]
    fn into_iterator_yields_in_order() {
        let args: PathArgs = smallvec::smallvec![arg("a", "1"), arg("b", "2")].into();

        let collected: Vec<(String, String)> = args
            .into_iter()
            .map(|p| (p.name.as_ref().to_string(), p.value.as_ref().to_string()))
            .collect();

        assert_eq!(
            collected,
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), "2".to_string())
            ]
        );
    }

    #[test]
    fn first_returns_none_when_empty_and_some_when_not() {
        let empty = PathArgs::new();
        assert!(empty.first().is_none());

        let non_empty: PathArgs = smallvec::smallvec![arg("id", "123")].into();
        let first = non_empty.first().unwrap();
        assert_eq!(first.name.as_ref(), "id");
        assert_eq!(first.value.as_ref(), "123");
    }

    #[test]
    fn iter_yields_all_items() {
        let args: PathArgs =
            smallvec::smallvec![arg("a", "1"), arg("b", "2"), arg("c", "3")].into();
        let names: Vec<&str> = args.iter().map(|a| a.name.as_ref()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
