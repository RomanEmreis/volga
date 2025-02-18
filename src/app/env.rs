﻿use crate::App;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const DEFAULT_INDEX_FILE: &str = "index.html";

/// Describes a Web Server's Hosting Environment
#[derive(Debug, Clone)]
pub struct HostEnv {
    /// Root folder of static content
    /// 
    /// Default: `/`
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
}

impl Default for HostEnv {
    #[inline]
    fn default() -> Self {
        Self::new("/")
    }
}

impl HostEnv {
    /// Creates a new [`HostEnv`] with given content root
    #[inline]
    pub fn new<T: ?Sized + AsRef<OsStr>>(content_root: &T) -> Self {
        let content_root = PathBuf::from(content_root);
        let index_path = content_root.join(DEFAULT_INDEX_FILE);
        Self {
            show_directory: false,
            fallback_path: None,
            content_root,
            index_path,
        }
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
    
    /// Enables showing a list of files when root "/" is requested
    /// 
    /// Default: `false`
    pub fn with_files_listing(mut self) -> Self {
        self.show_directory = true;
        self
    }
    
    /// Returns the content root of Web Server
    /// >Note: the folder could not exist
    #[inline]
    pub fn content_root(&self) -> &Path {
        &self.content_root
    }
    
    /// Returns the relative path to the index file. 
    /// >Note: the file could not exist
    #[inline]
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Returns the relative path to the fallback file if it's specified. 
    /// >Note: the file could not exist
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
}

impl App {
    /// Configures web server's hosting environment
    ///
    /// Defaults:
    /// - content_root: `/`
    /// - index_path: `index.html`
    pub fn with_hosting_environment(mut self, env: HostEnv) -> Self {
        self.host_env = env;
        self
    }
    
    /// Specifies a root folder for static content
    /// 
    /// Default: `/`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::App;
    ///
    /// let app = App::new()
    ///     .with_content_root("static");
    /// ```
    pub fn with_content_root<T: ?Sized + AsRef<OsStr>>(mut self, content_root: &T) -> Self {
        let new_root = PathBuf::from(content_root);
        let mut env = HostEnv::new(content_root);
        
        env.show_directory = self.host_env.show_directory;
        env.index_path = new_root.join(self.host_env.index_path);
        env.fallback_path = self.host_env
            .fallback_path
            .map(|fallback_path| new_root.join(fallback_path));
        
        self.host_env = env;
        self
    }

    /// Updates the default index file name with the custom one
    ///
    /// Default: `index.html`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::App;
    ///
    /// let app = App::new()
    ///     .with_index_file("default.html");
    /// ```
    pub fn with_index_file<T: AsRef<Path>>(mut self, index_file: T) -> Self {
        self.host_env = self.host_env.with_index_file(index_file);
        self
    }

    /// Updates the fallback file name with the custom one
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// # use volga::App;
    ///
    /// let app = App::new()
    ///     .with_fallback_file("not_found.html");
    /// ```
    pub fn with_fallback_file<T: AsRef<Path>>(mut self, fallback_file: T) -> Self {
        self.host_env = self.host_env.with_fallback_file(fallback_file);
        self
    }

    /// Enables showing a list of files when root "/" is requested
    ///
    /// Default: `false`
    pub fn with_files_listing(mut self) -> Self {
        self.host_env = self.host_env.with_files_listing();
        self
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use crate::app::env::HostEnv;

    #[test]
    fn it_creates_default_host_env() {
        let env = HostEnv::default();
        
        assert_eq!(env.content_root, PathBuf::from("/"));
        assert_eq!(env.index_path, PathBuf::from("/index.html"));
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
    fn it_creates_with_index_file() {
        let env = HostEnv::new("/root")
            .with_index_file("default.html");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/default.html"));
        assert_eq!(env.fallback_path, None);
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_with_fallback_file() {
        let env = HostEnv::new("/root")
            .with_fallback_file("error.html");

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, Some(PathBuf::from("/root/error.html")));
        assert!(!env.show_directory);
    }

    #[test]
    fn it_creates_with_file_listing() {
        let env = HostEnv::new("/root")
            .with_files_listing();

        assert_eq!(env.content_root, PathBuf::from("/root"));
        assert_eq!(env.index_path, PathBuf::from("/root/index.html"));
        assert_eq!(env.fallback_path, None);
        assert!(env.show_directory);
    }
}