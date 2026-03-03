use crate::error::Error;
use tokio::fs::read_dir;
use std::path::Path;

#[cfg(debug_assertions)]
use std::time::UNIX_EPOCH;

struct FileEntry {
    name: String,
    size: String,
    modified: String,
    link: String,
    is_dir: bool,
}

#[inline]
pub(super) async fn generate_html(
    directory: &Path,
    display_directory: &str,
    is_root: bool
) -> Result<String, Error> {
    let mut entries = Vec::new();

    if !is_root {
        entries.push(FileEntry {
            name: "../".to_string(),
            size: "-".to_string(),
            modified: "-".to_string(),
            link: "../".to_string(),
            is_dir: true,
        });
    }

    if let Ok(mut dir_entries) = read_dir(directory).await {
        while let Some(entry) = dir_entries.next_entry().await.ok().flatten() {
            let metadata = match entry.metadata().await {
                Ok(meta) => meta,
                Err(_) => continue,
            };

            let name = entry.file_name()
                .into_string()
                .unwrap_or_else(|_| "[Invalid UTF-8]".to_string());

            let is_dir = metadata.is_dir();
            let display_name;
            let size;
            if is_dir {
                display_name = format!("{name}/");
                size = "-".to_string();
            } else {
                display_name = name.clone();
                size = if cfg!(debug_assertions) {
                    metadata.len().to_string()
                } else {
                    "-".to_string()
                };
            }

            #[cfg(not(debug_assertions))]
            let modified = "-".to_string();

            #[cfg(debug_assertions)]
            let modified = metadata.modified().ok()
                .and_then(fmt_system_time)
                .unwrap_or_else(|| "Unknown".to_string());

            let link = display_name.clone();
            entries.push(FileEntry { name: display_name, size, modified, link, is_dir });
        }
    }

    // Sort: directories first, then files, both alphabetically
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));

    Ok(render_html(display_directory, &entries))
}

fn render_html(directory: &str, entries: &[FileEntry]) -> String {
    let mut html = String::with_capacity(512 + entries.len() * 128);

    html.push_str("<html>\n    <head><title>Index Of ");
    push_escaped(&mut html, directory);
    html.push_str("</title></head>\n    <body>\n        <h2>Index Of ");
    push_escaped(&mut html, directory);
    html.push_str("</h2>\n        <table border='1'>\n");
    html.push_str("            <tr><th>Name</th><th>Size (bytes)</th><th>Last Modified</th></tr>\n");

    for entry in entries {
        html.push_str("            <tr><td><a href='");
        push_escaped(&mut html, &entry.link);
        html.push_str("'>");
        push_escaped(&mut html, &entry.name);
        html.push_str("</a></td><td>");
        push_escaped(&mut html, &entry.size);
        html.push_str("</td><td>");
        push_escaped(&mut html, &entry.modified);
        html.push_str("</td></tr>\n");
    }

    html.push_str("        </table>\n    </body>\n    </html>");
    html
}

/// Appends `s` to `out` with HTML-special characters escaped.
fn push_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
}

/// Formats a [`SystemTime`] as an RFC 3339 UTC timestamp (e.g. `2024-03-01T12:00:00Z`).
///
/// Uses Howard Hinnant's Euclidean civil-calendar algorithm to avoid any
/// date-arithmetic dependencies.
#[cfg(debug_assertions)]
fn fmt_system_time(time: std::time::SystemTime) -> Option<String> {
    let secs = time.duration_since(UNIX_EPOCH).ok()?.as_secs();

    let days = secs / 86400;
    let rem  = secs % 86400;
    let hh   = rem / 3600;
    let mm   = (rem % 3600) / 60;
    let ss   = rem % 60;

    // Civil date from days since Unix epoch (Hinnant's algorithm).
    let z   = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y   = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp  = (5 * doy + 2) / 153;
    let d   = doy - (153 * mp + 2) / 5 + 1;
    let mo  = if mp < 10 { mp + 3 } else { mp - 9 };
    let y   = if mo <= 2 { y + 1 } else { y };

    Some(format!("{y:04}-{mo:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use crate::fs::static_files::file_listing::generate_html;

    #[tokio::test]
    async fn it_creates_file_listing_html() {
        let dir = Path::new("tests/resources");
        let html = generate_html(
            dir,
            "/tests/resources",
            true
        ).await;

        assert!(html.is_ok());
    }
}
