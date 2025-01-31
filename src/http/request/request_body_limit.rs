
const DEFAULT_BODY_SIZE: usize = 5 * 1024 * 1024; // 5 MB

#[derive(Debug, Copy, Clone)]
pub enum RequestBodyLimit {
    Disabled,
    Enabled(usize),
}

impl Default for RequestBodyLimit {
    fn default() -> Self {
        Self::Enabled(DEFAULT_BODY_SIZE)
    }
}

impl RequestBodyLimit {
    
}