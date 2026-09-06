//! Resolving a request target to a path under the content root

use crate::error::Error;
use std::{
    borrow::Cow,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

/// What a request target addresses on a static file mount.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Target {
    /// The mount point itself: `/` for a mount that answers the whole application,
    /// the group's prefix for a scoped one.
    Root,

    /// A path under the content root, built from the segments of the request target.
    Relative(PathBuf),
}

/// Resolves the request target `path` against the `prefix` a mount answers under.
///
/// The path is read as it arrived and its segments are kept as they are, so what comes out
/// addresses what the request asked for - there is nothing to reassemble and nothing to
/// re-order. A target the mount does not answer is `Ok(None)`: it is outside the prefix, or
/// it carries a segment that does not name a file in the directory before it - `.`, `..`, an
/// encoded separator, a drive prefix. Those are refused rather than dropped, since dropping
/// one would answer a path the request never asked for, and refusing them is what makes
/// traversal impossible here rather than caught after the fact.
///
/// An `Err` is a request target that is not a valid one at all - a malformed `%XX` escape,
/// or one that does not decode to UTF-8.
pub(crate) fn resolve(path: &str, prefix: &str) -> Result<Option<Target>, Error> {
    let Some(rest) = strip_mount_prefix(path, prefix) else {
        return Ok(None);
    };

    let mut relative = PathBuf::new();
    for segment in rest.split('/').filter(|segment| !segment.is_empty()) {
        let segment = percent_decode(segment)?;
        if !push_normal(&mut relative, segment.as_ref()) {
            return Ok(None);
        }
    }

    let target = if relative.as_os_str().is_empty() {
        Target::Root
    } else {
        Target::Relative(relative)
    };

    Ok(Some(target))
}

/// Returns what is left of `path` once the `prefix` the mount answers under is taken off it,
/// or `None` when the target is outside that mount.
///
/// An empty prefix is a mount that answers the whole application, and the prefix carries no
/// trailing slash, so `/static` addresses the mount point of `/static` itself while
/// `/staticky` is outside it.
#[inline]
fn strip_mount_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.is_empty() {
        return Some(path);
    }

    let rest = path.strip_prefix(prefix)?;
    match rest.as_bytes().first() {
        None | Some(b'/') => Some(rest),
        Some(_) => None,
    }
}

/// Pushes `segment` onto `path` when it names a single ordinary component, and returns
/// `false` when it names anything else.
#[inline]
fn push_normal(path: &mut PathBuf, segment: &str) -> bool {
    // A NUL is a normal component as far as `Path` is concerned and an invalid argument as
    // far as the filesystem is concerned - it would be reported as a failure rather than as
    // the missing file it is.
    if segment.contains('\0') {
        return false;
    }

    let mut components = Path::new(segment).components();
    match (components.next(), components.next()) {
        // The equality check catches what `Components` normalizes away on its own: a
        // decoded `app.css/` is one `Normal` component and is not the name that arrived.
        (Some(Component::Normal(name)), None) if name == OsStr::new(segment) => {
            path.push(name);
            true
        }
        _ => false,
    }
}

/// Decodes the `%XX` escapes of a single path segment.
///
/// Unlike form decoding, `+` is left as it is: in a request target it is a
/// literal plus sign rather than a space (RFC 3986 Section 3.3).
#[inline]
fn percent_decode(segment: &str) -> Result<Cow<'_, str>, Error> {
    if !segment.contains('%') {
        return Ok(Cow::Borrowed(segment));
    }

    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            decoded.push(bytes[i]);
            i += 1;
            continue;
        }

        let escape = bytes.get(i + 1..i + 3).ok_or_else(malformed_escape)?;
        let hi = char::from(escape[0])
            .to_digit(16)
            .ok_or_else(malformed_escape)?;

        let lo = char::from(escape[1])
            .to_digit(16)
            .ok_or_else(malformed_escape)?;

        decoded.push((hi * 16 + lo) as u8);
        i += 3;
    }

    String::from_utf8(decoded)
        .map(Cow::Owned)
        .map_err(|_| malformed_escape())
}

#[inline]
fn malformed_escape() -> Error {
    Error::client_error("Static files error: malformed percent-encoding in the request path")
}

#[cfg(test)]
mod tests {
    use super::{Cow, Target, percent_decode, resolve};
    use std::path::PathBuf;

    fn resolved(path: &str) -> Option<Target> {
        resolve(path, "").unwrap()
    }

    #[test]
    fn it_resolves_the_root() {
        assert_eq!(resolved("/"), Some(Target::Root));
        assert_eq!(resolved(""), Some(Target::Root));
        assert_eq!(resolved("//"), Some(Target::Root));
    }

    #[test]
    fn it_resolves_a_single_segment() {
        assert_eq!(
            resolved("/favicon.svg"),
            Some(Target::Relative(PathBuf::from("favicon.svg")))
        );
    }

    #[test]
    fn it_resolves_a_nested_path_in_the_order_it_arrived() {
        assert_eq!(
            resolved("/assets/app.css"),
            Some(Target::Relative(
                ["assets", "app.css"].iter().collect::<PathBuf>()
            ))
        );
    }

    #[test]
    fn it_resolves_a_path_deeper_than_any_ceiling() {
        let deep = "/a/b/c/d/e/f/g/h/i/j";
        let expected = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
            .iter()
            .collect::<PathBuf>();

        assert_eq!(resolved(deep), Some(Target::Relative(expected)));
    }

    #[test]
    fn it_ignores_empty_segments() {
        assert_eq!(
            resolved("//assets//app.css/"),
            Some(Target::Relative(
                ["assets", "app.css"].iter().collect::<PathBuf>()
            ))
        );
    }

    #[test]
    fn it_percent_decodes_segments() {
        assert_eq!(
            resolved("/my%20file.css"),
            Some(Target::Relative(PathBuf::from("my file.css")))
        );
        assert_eq!(
            resolved("/%D1%84%D0%B0%D0%B9%D0%BB.txt"),
            Some(Target::Relative(PathBuf::from(
                "\u{0444}\u{0430}\u{0439}\u{043b}.txt"
            )))
        );
    }

    #[test]
    fn it_leaves_a_plus_alone_when_decoding() {
        // `+` is a space in a form body, but a literal plus in a request target.
        assert_eq!(
            resolved("/my+file.css"),
            Some(Target::Relative(PathBuf::from("my+file.css")))
        );
    }

    #[test]
    fn it_declines_a_traversal_segment() {
        for path in [
            "/../secret",
            "/assets/../../secret",
            "/%2e%2e/secret",
            "/./app.css",
        ] {
            assert_eq!(resolved(path), None, "expected `{path}` to be declined");
        }
    }

    #[test]
    fn it_declines_an_encoded_separator() {
        // A decoded segment that carries a separator names a path rather than a file, so it
        // is not the name the request asked for either way.
        for path in ["/assets%2Fapp.css", "/assets%2f..%2fsecret"] {
            assert_eq!(resolved(path), None, "expected `{path}` to be declined");
        }
    }

    #[test]
    fn it_declines_a_segment_carrying_a_nul() {
        assert_eq!(resolved("/app.css%00.txt"), None);
    }

    #[test]
    fn it_declines_an_absolute_segment() {
        assert_eq!(resolved("/%2Fetc%2Fpasswd"), None);
    }

    #[test]
    fn it_rejects_malformed_percent_encoding() {
        for path in ["/%", "/%2", "/%zz", "/%2z", "/app%.css"] {
            assert!(
                resolve(path, "").is_err(),
                "expected `{path}` to be rejected"
            );
        }
    }

    #[test]
    fn it_rejects_percent_encoding_that_is_not_utf8() {
        assert!(resolve("/%FF%FE", "").is_err());
    }

    #[test]
    fn it_borrows_a_segment_that_needs_no_decoding() {
        assert!(matches!(percent_decode("app.css"), Ok(Cow::Borrowed(_))));
    }

    #[test]
    fn it_resolves_under_a_prefix() {
        assert_eq!(resolve("/static", "/static").unwrap(), Some(Target::Root));
        assert_eq!(resolve("/static/", "/static").unwrap(), Some(Target::Root));
        assert_eq!(
            resolve("/static/assets/app.css", "/static").unwrap(),
            Some(Target::Relative(
                ["assets", "app.css"].iter().collect::<PathBuf>()
            ))
        );
    }

    #[test]
    fn it_declines_a_target_outside_the_prefix() {
        for path in ["/", "/index.html", "/staticky/app.css", "/api/static"] {
            assert_eq!(
                resolve(path, "/static").unwrap(),
                None,
                "expected `{path}` to be declined"
            );
        }
    }

    #[test]
    fn it_declines_a_traversal_out_of_the_prefix() {
        assert_eq!(resolve("/static/../index.html", "/static").unwrap(), None);
    }
}
