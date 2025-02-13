use crate::error::Error;
use chrono::{DateTime, Utc};
use handlebars::{Handlebars, RenderError, TemplateError};
use tokio::fs::read_dir;
use serde::ser::{Serialize, Serializer, SerializeStruct};

use std::{
    time::UNIX_EPOCH,
    path::Path
};

const HTML_TEMPLATE: &str = 
    r#"<html>
    <head><title>Index Of {{directory}}</title></head>
    <body>
        <h2>Index Of {{directory}}</h2>
        <table border='1'>
            <tr><th>Name</th><th>Size (bytes)</th><th>Last Modified</th></tr>
            {{#each entries}}
            <tr>
                <td><a href='{{link}}'>{{name}}</a></td>
                <td>{{size}}</td>
                <td>{{modified}}</td>
            </tr>
            {{/each}}
        </table>
    </body>
    </html>"#;

struct FileEntry {
    name: String,
    size: String,
    modified: String,
    link: String,
    is_dir: bool,
}

impl Serialize for FileEntry {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer
            .serialize_struct("FileEntry", 5)?;
        
        state.serialize_field("name", &self.name)?;
        state.serialize_field("size", &self.size)?;
        state.serialize_field("modified", &self.modified)?;
        state.serialize_field("link", &self.link)?;
        state.serialize_field("is_dir", &self.is_dir)?;
        state.end()
    }
}

#[inline]
pub(super) async fn generate_html(directory: &Path, is_root: bool) -> Result<String, Error> {
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
                size = metadata.len().to_string();
            }
            let modified = metadata.modified().ok()
                .and_then(|time| {
                    let duration = time.duration_since(UNIX_EPOCH).ok()?;
                    Some(DateTime::<Utc>::from(UNIX_EPOCH + duration).to_rfc3339())
                })
                .unwrap_or_else(|| "Unknown".to_string());

            let link = display_name.clone();
            entries.push(FileEntry { name: display_name, size, modified, link, is_dir });
        }
    }

    // Sort: Directories first, then files
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    
    let mut handlebars = Handlebars::new();
    let data = serde_json::json!({ "directory": directory, "entries": entries });
    handlebars.register_template_string("directory", HTML_TEMPLATE)?;
    handlebars.render("directory", &data).map_err(Error::from)
}

impl From<TemplateError> for Error {
    fn from(err: TemplateError) -> Self {
        Self::new("handlebars::TemplateError", err)
    }
}

impl From<RenderError> for Error {
    fn from(err: RenderError) -> Self {
        Self::new("handlebars::RenderError", err)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use crate::fs::static_files::file_listing::generate_html;
    
    #[tokio::test]
    async fn it_creates_file_listing_html() {
        let dir = Path::new("tests/resources");
        let html = generate_html(dir, true).await;
        
        assert!(html.is_ok());
    }
}