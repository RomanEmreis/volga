//! URL path arguments utilities

use smallvec::SmallVec;
use super::DEFAULT_DEPTH;
use crate::error::Error;
use std::sync::Arc;
use std::sync::OnceLock;

const QUERY_SEPARATOR: char = '&';
const QUERY_KEY_VALUE_SEPARATOR: char = '=';

/// Route path arguments
/// 
/// > **Note:** This type is part of Volga's public API but is primarily intended
/// > for framework-level extractors and middleware. It should not be
/// > constructed manually.
#[derive(Debug)]
pub struct PathArgs {
    args: SmallVec<[PathArg; DEFAULT_DEPTH]>,
    encoded: OnceLock<String>,
}

/// A single matched path argument.
///
/// > **Note:** This type is part of Volga's public API but is primarily intended
/// > for framework-level extractors and middleware. It should not be
/// > constructed manually.
#[derive(Debug, Clone)]
pub struct PathArg {
    /// Argument name
    pub(crate) name: Arc<str>,

    /// Argument value
    pub(crate) value: Arc<str>,
}

impl PathArgs {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            args: SmallVec::new(),
            encoded: OnceLock::new(),
        }
    }

    /// Returns an iterator over the args.
    ///
    /// The iterator yields all items from start to end.
    #[inline]
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, PathArg> {
        self.args.iter()
    }

    /// Returns the first arg, or `None` if it is empty.
    #[inline]
    #[allow(unused)]
    pub(crate) fn first(&self) -> Option<&PathArg> {
        self.args.first()
    }

    /// Append an item to the args vector.
    #[inline]
    pub(crate) fn push(&mut self, arg: PathArg) {
        self.args.push(arg);
        let _ = self.encoded.take();
    }

    /// Restures a query string of this route
    #[inline]
    pub(crate) fn encoded(&self) -> Result<&str, Error> {
        if self.args.is_empty() {
            return Err(Error::client_error("Path parsing error: missing arguments"));
        }

        let value = self
            .encoded
            .get_or_init(|| encode(&self.args));

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

impl Default for PathArgs {
    #[inline]
    fn default() -> Self {
        Self::new()
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

impl From<SmallVec<[PathArg; DEFAULT_DEPTH]>> for PathArgs {
    #[inline]
    fn from(args: SmallVec<[PathArg; DEFAULT_DEPTH]>) -> Self {
        Self {
            args,
            encoded: OnceLock::new(),
        }
    }
}

impl IntoIterator for PathArgs {
    type Item = PathArg;
    type IntoIter = smallvec::IntoIter<[PathArg; DEFAULT_DEPTH]>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.args.into_iter()
    }
}

#[inline]
fn encode(args: &SmallVec<[PathArg; DEFAULT_DEPTH]>) -> String  {
    let capacity = args.iter().fold(0, |acc, arg| {
        acc + arg.name.len() + arg.value.len() + 1
    }) + args.len().saturating_sub(1);

    let mut result = String::with_capacity(capacity);
    let mut iter = args.iter();

    if let Some(first) = iter.next() {
        result.push_str(first.name.as_ref());
        result.push(QUERY_KEY_VALUE_SEPARATOR);
        result.push_str(first.value.as_ref());
        for s in iter {
            result.push(QUERY_SEPARATOR);
            result.push_str(s.name.as_ref());
            result.push(QUERY_KEY_VALUE_SEPARATOR);
            result.push_str(s.value.as_ref());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(name: &str, value: &str) -> PathArg {
        PathArg { name: name.into(), value: value.into() }
    }

    #[test]
    fn it_makes_query_str() {
        let args: PathArgs = smallvec::smallvec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
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
            vec![("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())]
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
        let args: PathArgs = smallvec::smallvec![arg("a", "1"), arg("b", "2"), arg("c", "3")].into();
        let names: Vec<&str> = args.iter().map(|a| a.name.as_ref()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
