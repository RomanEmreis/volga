//! Application Host Environment configuration

use crate::{App, headers::CacheControl};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const DEFAULT_INDEX_FILE: &str = "index.html";
const DEFAULT_CONTENT_ROOT: &str = "/static";

/// Describes a Web Server's Hosting Environment
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct HostEnv {
    /// Root folder of static content
    ///
    /// Default: `/static`
    content_root: PathBuf,

    /// Path to the `index.html` file
    ///
    /// Default: `/index.html`
    index_path: PathBuf,

    /// Path to the fallback file
    ///
    /// Default: `None`
    fallback_path: Option<PathBuf>,

    /// Specifies whether to show a content root directory
    ///
    /// Default: `false`
    show_directory: bool,

    /// `Cache-Control` for the files addressed by a content-hashed name
    ///
    /// Default: `max-age=86400, public, immutable`
    asset_cache_control: CacheControl,

    /// `Cache-Control` for the files addressed by a stable name - the index file
    /// and the fallback file
    ///
    /// Default: `no-cache`
    shell_cache_control: CacheControl,
}

impl Default for HostEnv {
    #[inline]
    fn default() -> Self {
        Self::new(DEFAULT_CONTENT_ROOT)
    }
}

impl HostEnv {
    /// Creates a new [`HostEnv`] with the given content root
    #[inline]
    pub fn new<T: ?Sized + AsRef<OsStr>>(content_root: &T) -> Self {
        let content_root = PathBuf::from(content_root);
        warn_if_root_is_fs_root(&content_root);

        let index_path = content_root.join(DEFAULT_INDEX_FILE);
        Self {
            show_directory: false,
            fallback_path: None,
            asset_cache_control: CacheControl::ASSET,
            shell_cache_control: CacheControl::SHELL,
            content_root,
            index_path,
        }
    }

    /// Specifies a root folder for static content
    ///
    /// Default: `/static`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::app::HostEnv;
    ///
    /// let app = HostEnv::default()
    ///     .with_content_root("static");
    /// ```
    pub fn with_content_root<T: ?Sized + AsRef<OsStr>>(mut self, root: &T) -> Self {
        self.content_root = PathBuf::from(root);
        warn_if_root_is_fs_root(&self.content_root);

        if let Some(file_name) = self.index_path.file_name() {
            self.index_path = self.content_root.join(file_name);
        }

        if let Some(fallback_file) = self.fallback_path.as_ref().and_then(|p| p.file_name()) {
            self.fallback_path = Some(self.content_root.join(fallback_file));
        }

        self
    }

    /// Updates the default index file name with the custom one
    ///
    /// Default: `index.html`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::app::HostEnv;
    ///
    /// let env = HostEnv::default()
    ///     .with_index_file("default.html");
    ///
    /// assert_eq!(env.index_path().to_str().unwrap(), "default.html");
    /// ```
    pub fn with_index_file<T: AsRef<Path>>(mut self, index_file: T) -> Self {
        let index_path = self.content_root.join(index_file);
        self.index_path = index_path;
        self
    }

    /// Updates the fallback file name with the custom one
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::app::HostEnv;
    ///
    /// let env = HostEnv::default()
    ///     .with_fallback_file("not_found.html");
    ///
    /// assert_eq!(env.fallback_path().unwrap().to_str().unwrap(), "not_found.html");
    /// ```
    pub fn with_fallback_file<T: AsRef<Path>>(mut self, fallback_file: T) -> Self {
        let fallback_path = self.content_root.join(fallback_file);
        self.fallback_path = Some(fallback_path);
        self
    }

    /// Configures the `Cache-Control` header of the static files that are addressed
    /// by a content-hashed name, which is every file but the index and the fallback one.
    ///
    /// The closure receives the policy currently configured, which starts as
    /// [`CacheControl::ASSET`] - `max-age=86400, public, immutable`.
    ///
    /// # Example
    /// ```no_run
    /// # use volga::app::HostEnv;
    ///
    /// // Keeps the assets fresh for an hour instead of a day
    /// let env = HostEnv::new("static")
    ///     .with_asset_cache_control(|cc| cc.with_max_age(60 * 60));
    /// ```
    pub fn with_asset_cache_control<F>(mut self, config: F) -> Self
    where
        F: FnOnce(CacheControl) -> CacheControl,
    {
        self.asset_cache_control = config(self.asset_cache_control);
        self
    }

    /// Configures the `Cache-Control` header of the static files that are addressed
    /// by a stable name - the index file and the fallback file.
    ///
    /// The closure receives the policy currently configured, which starts as
    /// [`CacheControl::SHELL`] - `no-cache`, so that a deploy is picked up on the next
    /// request rather than after the `max-age` of the previous one.
    ///
    /// # Example
    /// ```no_run
    /// # use volga::app::HostEnv;
    ///
    /// // Never stores the shell at all
    /// let env = HostEnv::new("static")
    ///     .with_fallback_file("index.html")
    ///     .with_shell_cache_control(|cc| cc.with_no_store());
    /// ```
    pub fn with_shell_cache_control<F>(mut self, config: F) -> Self
    where
        F: FnOnce(CacheControl) -> CacheControl,
    {
        self.shell_cache_control = config(self.shell_cache_control);
        self
    }

    /// Enables showing a list of files when root "/static" is requested
    ///
    /// Default: `false`
    pub fn with_files_listing(mut self) -> Self {
        warn_if_listing_enabled_in_release();

        self.show_directory = true;
        self
    }

    /// Returns the content root of Web Server
    ///
    /// > ***Note:*** the folder could not exist
    #[inline]
    pub fn content_root(&self) -> &Path {
        &self.content_root
    }

    /// Returns the relative path to the index file.
    ///
    /// > **Note:** the file could not exist
    #[inline]
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Returns the relative path to the fallback file if it's specified.
    ///
    /// > **Note:** the file could not exist
    #[inline]
    pub fn fallback_path(&self) -> Option<&Path> {
        match &self.fallback_path {
            Some(path) => Some(path),
            None => None,
        }
    }

    /// Returns `true` if directory listing is enabled
    #[inline]
    pub fn show_files_listing(&self) -> bool {
        self.show_directory
    }

    /// Returns the `Cache-Control` policy of the files addressed by a content-hashed name
    #[inline]
    pub fn asset_cache_control(&self) -> CacheControl {
        self.asset_cache_control
    }

    /// Returns the `Cache-Control` policy of the index and the fallback files
    #[inline]
    pub fn shell_cache_control(&self) -> CacheControl {
        self.shell_cache_control
    }

    /// Returns `true` if `path` is addressed by a stable name - the index file
    /// or the fallback file - and so must not be served as immutable.
    #[inline]
    pub(crate) fn is_shell_path(&self, path: &Path) -> bool {
        path == self.index_path || self.fallback_path.as_deref() == Some(path)
    }
}

impl App {
    /// Configures web server's hosting environment
    ///
    /// Defaults:
    /// - content_root: `/static`
    /// - index_path: `index.html`
    pub fn with_host_env<T>(mut self, config: T) -> Self
    where
        T: FnOnce(HostEnv) -> HostEnv,
    {
        self.host_env = config(self.host_env);
        self
    }

    /// Configures web server's hosting environment
    ///
    /// Defaults:
    /// - content_root: `/static`
    /// - index_path: `index.html`
    pub fn set_host_env(mut self, env: HostEnv) -> Self {
        self.host_env = env;
        self
    }
}

#[inline]
fn warn_if_root_is_fs_root(path: &Path) {
    if path == Path::new("/") {
        warn(
            "HostEnv content_root is set to '/', which can expose the entire filesystem. Consider using a dedicated static directory.",
        );
    }
}

#[inline]
fn warn_if_listing_enabled_in_release() {
    #[cfg(not(debug_assertions))]
    warn(
        "Static files listing is enabled in release mode; this may leak file metadata. Consider disabling it for production.",
    );
}

/// Reports a hosting configuration hazard.
///
/// These are found while the [`HostEnv`] is being built, long before a request is served,
/// and they describe a setup that is unlikely to be intended - so they are reported through
/// `tracing` when the feature is on, and on `stderr` when it is off, rather than being
/// dropped along with the feature.
#[inline]
fn warn(message: &str) {
    #[cfg(feature = "tracing")]
    tracing::warn!("{message}");
    #[cfg(not(feature = "tracing"))]
    eprintln!("WARN: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use std::path::PathBuf;

    #[test]
    fn it_defaults_to_immutable_assets_and_a_revalidated_shell() {
        let env = HostEnv::new("/root");

        assert_eq!(
            env.asset_cache_control().to_string(),
            "max-age=86400, public, immutable"
        );
        assert_eq!(env.shell_cache_control().to_string(), "no-cache");
    }

    #[test]
    fn it_configures_asset_cache_control() {
        let env = HostEnv::new("/root").with_asset_cache_control(|cc| cc.with_max_age(60));

        assert_eq!(
            env.asset_cache_control().to_string(),
            "max-age=60, public, immutable"
        );
        assert_eq!(env.shell_cache_control().to_string(), "no-cache");
    }

    #[test]
    fn it_configures_shell_cache_control() {
        let env = HostEnv::new("/root").with_shell_cache_control(|cc| cc.with_no_store());

        assert_eq!(env.shell_cache_control().to_string(), "no-cache, no-store");
        assert_eq!(
            env.asset_cache_control().to_string(),
            "max-age=86400, public, immutable"
        );
    }

    #[test]
    fn it_recognizes_the_shell_paths() {
        let env = HostEnv::new("/root").with_fallback_file("404.html");

        assert!(env.is_shell_path(Path::new("/root/index.html")));
        assert!(env.is_shell_path(Path::new("/root/404.html")));
        assert!(!env.is_shell_path(Path::new("/root/assets/app.css")));
    }

    #[test]
    fn it_recognizes_no_fallback_as_a_shell_path() {
        let env = HostEnv::new("/root");

        assert!(env.is_shell_path(Path::new("/root/index.html")));
        assert!(!env.is_shell_path(Path::new("/root/404.html")));
    }

    #[test]
    fn it_creates_default_host_env() {
        let env = HostEnv::default();

        assert_eq!(env.content_root, PathBuf::from(DEFAULT_CONTENT_ROOT));
        assert_eq!(env.index_path, PathBuf::from("/static/index.html"));
        assert_eq!(env.fallback_path, None);
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_host_env() {
        let env = HostEnv::new("/root");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, None);
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_host_env_with_content_root() {
        let env = HostEnv::default().with_content_root("/root");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, None);
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_with_index_file() {
        let env = HostEnv::new("/root").with_index_file("default.html");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/default.html"));
        assert_eq!(env.fallback_path, None);
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_with_fallback_file() {
        let env = HostEnv::new("/root").with_fallback_file("error.html");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, Some(PathBuf::from("/root/error.html")));
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_with_file_listing() {
        let env = HostEnv::new("/root").with_files_listing();

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, None);
        assert!(env.show_directory);
    }

    #[test]
    fn it_updates_content_root() {
        let app = App::new().with_host_env(|env| env.with_content_root("tests/resources"));

        assert_eq!(app.host_env.content_root, PathBuf::from("tests/resources"));
        assert_eq!(
            app.host_env.index_path,
            PathBuf::from("tests/resources/index.html")
        );
        assert_eq!(app.host_env.fallback_path, None);
        assert!(!app.host_env.show_directory);
    }

    #[test]
    fn it_updates_index_file_with_content_root() {
        let app = App::new().with_host_env(|env| {
            env.with_content_root("tests/resources")
                .with_index_file("default.html")
        });

        assert_eq!(app.host_env.content_root, PathBuf::from("tests/resources"));
        assert_eq!(
            app.host_env.index_path,
            PathBuf::from("tests/resources/default.html")
        );
        assert_eq!(app.host_env.fallback_path, None);
        assert!(!app.host_env.show_directory);
    }

    #[test]
    fn it_updates_fallback_file_with_content_root() {
        let app = App::new().with_host_env(|env| {
            env.with_fallback_file("404.html")
                .with_content_root("tests/resources")
        });

        assert_eq!(app.host_env.content_root, PathBuf::from("tests/resources"));
        assert_eq!(
            app.host_env.index_path,
            PathBuf::from("tests/resources/index.html")
        );
        assert_eq!(
            app.host_env.fallback_path,
            Some(PathBuf::from("tests/resources/404.html"))
        );
        assert!(!app.host_env.show_directory);
    }
}
