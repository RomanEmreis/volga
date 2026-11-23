//! File System tools and abstractions

use mime::Mime;
use std::path::Path;

#[cfg(feature = "static-files")]
pub mod static_files;

/// Returns a file MIME type or `application/octet-stream` if unable to determine
#[inline]
pub fn get_mime_or_octet_stream<P: AsRef<Path>>(path: P) -> Mime {
    mime_guess::from_path(&path).first_or_octet_stream()
}

/// Reduces a file name a client sent to one that stays inside the folder it is saved to.
///
/// Takes what follows the last `/` or `\`, so `../../etc/passwd` becomes `passwd` and
/// `C:\Users\me\report.pdf` becomes `report.pdf`. Returns `None` when nothing usable is
/// left: an empty name, one made only of dots and spaces (`.`, `..`), or one the platform
/// reads as more than a plain file name, such as `C:x` on Windows.
pub(crate) fn file_name_in_folder(name: &str) -> Option<&str> {
    use std::path::Component;

    let name = name.rsplit(['/', '\\']).next()?;
    if name.trim_matches(['.', ' ']).is_empty() {
        return None;
    }

    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Some(name),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::file_name_in_folder;

    #[test]
    fn it_keeps_a_plain_file_name() {
        assert_eq!(file_name_in_folder("report.pdf"), Some("report.pdf"));
        assert_eq!(file_name_in_folder(".env"), Some(".env"));
        assert_eq!(file_name_in_folder("a..b"), Some("a..b"));
        assert_eq!(file_name_in_folder("résumé.pdf"), Some("résumé.pdf"));
    }

    #[test]
    fn it_takes_the_last_segment_of_a_path() {
        assert_eq!(file_name_in_folder("../x.txt"), Some("x.txt"));
        assert_eq!(file_name_in_folder("../../etc/passwd"), Some("passwd"));
        assert_eq!(file_name_in_folder("/tmp/x.txt"), Some("x.txt"));
        assert_eq!(file_name_in_folder("a/b.txt"), Some("b.txt"));
        assert_eq!(file_name_in_folder("..\\x.txt"), Some("x.txt"));
        assert_eq!(
            file_name_in_folder("C:\\Users\\me\\report.pdf"),
            Some("report.pdf")
        );
    }

    #[test]
    fn it_refuses_a_name_with_nothing_usable_left() {
        for name in ["", ".", "..", "...", " . ", "a/", "a/..", "..\\", "/"] {
            assert_eq!(file_name_in_folder(name), None, "{name:?}");
        }
    }
}
