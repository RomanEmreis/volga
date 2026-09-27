//! Resolving a request target to a path under the content root

use crate::http::endpoints::route::{percent_decode, split_path};
use std::{
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
/// The path is read the way the router reads it: split on the separators it was written
/// with, each segment percent-decoded, and the prefix compared with the decoded segments -
/// so a mount and a route under one prefix answer the same requests. What is left is kept
/// in the order it arrived, so what comes out addresses what the request asked for - there
/// is nothing to reassemble and nothing to re-order. A target the mount does not answer is
/// `None`: it is outside the prefix, or it carries a segment that does not name a file in
/// the directory before it - `.`, `..`, an encoded separator, a drive prefix. Those are
/// refused rather than dropped, since dropping one would answer a path the request never
/// asked for, and refusing them is what makes traversal impossible here rather than caught
/// after the fact.
///
/// A request target that does not decode - a malformed `%XX` escape, or escapes that are
/// not UTF-8 - is `None` as well. It is not the mount's to answer: the router decodes the
/// same segments the same way, so the request goes on to the `400` routing already decided
/// on, through the error handler, as any other malformed path does.
pub(crate) fn resolve(path: &str, prefix: &str) -> Option<Target> {
    let mut segments = split_path(path).map(percent_decode);

    for expected in split_path(prefix) {
        match segments.next()?.ok()? {
            segment if segment == expected => continue,
            _ => return None,
        }
    }

    let mut relative = PathBuf::new();
    for segment in segments {
        if !push_normal(&mut relative, segment.ok()?.as_ref()) {
            return None;
        }
    }

    let target = if relative.as_os_str().is_empty() {
        Target::Root
    } else {
        Target::Relative(relative)
    };

    Some(target)
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

#[cfg(test)]
mod tests {
    use super::{Target, resolve};
    use std::path::PathBuf;

    fn resolved(path: &str) -> Option<Target> {
        resolve(path, "")
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
    fn it_declines_malformed_percent_encoding() {
        for path in ["/%", "/%2", "/%zz", "/%2z", "/app%.css"] {
            assert_eq!(resolve(path, ""), None, "expected `{path}` to be declined");
        }
    }

    #[test]
    fn it_declines_percent_encoding_that_is_not_utf8() {
        assert_eq!(resolve("/%FF%FE", ""), None);
        assert_eq!(resolve("/static/%FF", "/static"), None);
    }

    #[test]
    fn it_resolves_under_a_prefix() {
        assert_eq!(resolve("/static", "/static"), Some(Target::Root));
        assert_eq!(resolve("/static/", "/static"), Some(Target::Root));
        assert_eq!(
            resolve("/static/assets/app.css", "/static"),
            Some(Target::Relative(
                ["assets", "app.css"].iter().collect::<PathBuf>()
            ))
        );
    }

    #[test]
    fn it_declines_a_target_outside_the_prefix() {
        for path in ["/", "/index.html", "/staticky/app.css", "/api/static"] {
            assert_eq!(
                resolve(path, "/static"),
                None,
                "expected `{path}` to be declined"
            );
        }
    }

    #[test]
    fn it_reads_the_prefix_the_way_the_router_reads_a_route() {
        for path in ["//static/app.css", "/static//app.css", "/st%61tic/app.css"] {
            assert_eq!(
                resolve(path, "/static"),
                Some(Target::Relative(PathBuf::from("app.css"))),
                "expected `{path}` to be resolved"
            );
        }
    }

    #[test]
    fn it_matches_a_prefix_spelled_with_characters_that_are_encoded_on_the_wire() {
        assert_eq!(
            resolve("/caf%C3%A9/my%20file.css", "/caf\u{e9}"),
            Some(Target::Relative(PathBuf::from("my file.css")))
        );
        assert_eq!(resolve("/caf%C3%A9", "/caf\u{e9}"), Some(Target::Root));
    }

    #[test]
    fn it_does_not_read_an_encoded_separator_as_the_end_of_the_prefix() {
        assert_eq!(resolve("/static%2Fapp.css", "/static"), None);
    }

    #[test]
    fn it_declines_a_traversal_out_of_the_prefix() {
        assert_eq!(resolve("/static/../index.html", "/static"), None);
    }
}
