//! Building blocks the `#[derive(Validate)]` expansion calls into.
//!
//! Everything here is usable by hand as well - the derive writes out the same
//! calls a hand-written [`Validate`] impl would make.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::{Validate, ValidationError};

/// Reports the length of a value for the `length` rule.
///
/// Strings are measured in characters (Unicode scalar values), which is what
/// OpenAPI's `minLength` / `maxLength` count; collections are measured in elements.
pub trait Length {
    /// Returns the length of this value
    fn validation_length(&self) -> usize;
}

impl Length for str {
    #[inline]
    fn validation_length(&self) -> usize {
        self.chars().count()
    }
}

impl Length for String {
    #[inline]
    fn validation_length(&self) -> usize {
        self.as_str().validation_length()
    }
}

impl<T> Length for [T] {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<T> Length for Vec<T> {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<K, V, S> Length for HashMap<K, V, S> {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<T, S> Length for HashSet<T, S> {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<K, V> Length for BTreeMap<K, V> {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<T> Length for BTreeSet<T> {
    #[inline]
    fn validation_length(&self) -> usize {
        self.len()
    }
}

impl<T: Length + ?Sized> Length for &T {
    #[inline]
    fn validation_length(&self) -> usize {
        (**self).validation_length()
    }
}

/// Returns the length of `value` for the `length` rule
#[inline]
pub fn length<T: Length + ?Sized>(value: &T) -> usize {
    value.validation_length()
}

/// Runs a nested [`Validate`] and merges its failures under `field`
#[inline]
pub fn nested<T>(errors: &mut ValidationError, field: &str, value: &T)
where
    T: Validate<Error = ValidationError> + ?Sized,
{
    if let Err(err) = value.validate() {
        errors.merge_at(field, err);
    }
}

/// Runs a nested [`Validate`] whose fields the client sent at this level, merging its
/// failures as they are.
///
/// This is what `#[serde(flatten)]` asks for: the nested type has no field of its own on the
/// wire, so naming one in the failure would point at something the client never sent.
#[inline]
pub fn nested_flat<T>(errors: &mut ValidationError, value: &T)
where
    T: Validate<Error = ValidationError> + ?Sized,
{
    if let Err(err) = value.validate() {
        errors.merge(err);
    }
}

/// Runs a nested [`Validate`] over every element, merging failures under `field[index]`
#[inline]
pub fn nested_each<'a, T, I>(errors: &mut ValidationError, field: &str, values: I)
where
    T: Validate<Error = ValidationError> + 'a,
    I: IntoIterator<Item = &'a T>,
{
    for (index, value) in values.into_iter().enumerate() {
        if let Err(err) = value.validate() {
            errors.merge_at(&format!("{field}[{index}]"), err);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Inner {
        key: String,
    }

    impl Validate for Inner {
        type Error = ValidationError;

        fn validate(&self) -> Result<(), Self::Error> {
            if self.key.is_empty() {
                return Err(ValidationError::field("key", "key is required"));
            }
            Ok(())
        }
    }

    #[test]
    fn it_measures_strings_in_characters() {
        assert_eq!(length("hello"), 5);
        assert_eq!(length(&String::from("привет")), 6);
        assert_eq!(length("noel\u{0308}"), 5);
    }

    #[test]
    fn it_measures_collections_in_elements() {
        assert_eq!(length(&vec![1, 2, 3]), 3);
        assert_eq!(length(&[1, 2][..]), 2);
        assert_eq!(length(&HashMap::from([("a", 1)])), 1);
        assert_eq!(length(&HashSet::from([1, 2])), 2);
        assert_eq!(length(&BTreeMap::from([("a", 1)])), 1);
        assert_eq!(length(&BTreeSet::from([1, 2, 3])), 3);
    }

    #[test]
    fn it_merges_nested_failures() {
        let mut errors = ValidationError::new();
        nested(&mut errors, "inner", &Inner { key: String::new() });

        assert_eq!(errors.to_string(), "inner.key: key is required");
    }

    #[test]
    fn it_merges_flattened_failures_without_naming_a_field() {
        let mut errors = ValidationError::new();
        nested_flat(&mut errors, &Inner { key: String::new() });

        assert_eq!(errors.to_string(), "key: key is required");
    }

    #[test]
    fn it_merges_nested_failures_of_every_element() {
        let mut errors = ValidationError::new();
        let items = vec![Inner { key: "ok".into() }, Inner { key: String::new() }];

        nested_each(&mut errors, "items", &items);

        assert_eq!(errors.to_string(), "items[1].key: key is required");
    }
}
