//! Request Body Limit

use crate::Limit;

const DEFAULT_BODY_SIZE: usize = 5 * 1024 * 1024; // 5 MB

/// Represents whether a request body has a configured limit of not
/// 
/// Default: 5 MB
#[derive(Debug, Copy, Clone)]
pub(crate) enum RequestBodyLimit {
    /// Body limit completely disabled
    Disabled,
    /// Configured body limit with a specific size
    Enabled(usize),
}

impl Default for RequestBodyLimit {
    #[inline]
    fn default() -> Self {
        Self::Enabled(DEFAULT_BODY_SIZE)
    }
}

impl From<Limit<usize>> for RequestBodyLimit {
    #[inline]
    fn from(limit: Limit<usize>) -> Self {
        match limit {
            Limit::Limited(limit) => Self::Enabled(limit),
            Limit::Unlimited => Self::Disabled,
            Limit::Default => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RequestBodyLimit, DEFAULT_BODY_SIZE};

    #[test]
    fn it_creates_default_body_limit() {
        let limit = RequestBodyLimit::default();
        let RequestBodyLimit::Enabled(limit) = limit else { unreachable!() };

        assert_eq!(limit, DEFAULT_BODY_SIZE)
    }
}