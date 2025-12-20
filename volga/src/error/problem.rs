//! Problem Details implementation

/// Produces an error response in a [Problem Details](https://www.rfc-editor.org/rfc/rfc9457) format
/// 
/// # Example
/// ```no_run
/// # use volga::problem;
/// let problem_details = problem! {
///     "type": "https://tools.ietf.org/html/rfc9110#section-15.5.1",
///     "title": "Bad Request",
///     "status": 400,
///     "details": "Your request parameters didn't validate.",
///     "instance": "/some/resource/path",
///     "invalid-params": [
///         { "name": "id", "reason": "Must be a positive integer" }
///     ]
/// };
/// ```
#[macro_export]
macro_rules! problem {
    (
        "status": $status:expr 
        $(, $key:tt : $value:tt)* 
        $(,)?
    ) => {{
        let status = $crate::http::StatusCode::from_u16($status)
            .unwrap_or($crate::http::StatusCode::OK);
        $crate::response!(
            status,
            $crate::HttpBody::json($crate::json::json_internal!({
                "type": $crate::error::Problem::get_problem_type_url($status),
                "title": status.canonical_reason().unwrap_or("unknown status code"),
                "status": $status,
                $($key: $value),*
            })),
            [
                ($crate::headers::CONTENT_TYPE, "problem+json"),
            ]
        )        
    }};
    
    (
        "type": $type:expr,
        "status": $status:expr 
        $(, $key:tt : $value:tt)* $(,)?
    ) => {{
        let status = $crate::http::StatusCode::from_u16($status)
            .unwrap_or($crate::http::StatusCode::OK);
        $crate::response!(
            status,
            $crate::HttpBody::json($crate::json::json_internal!({
                "type": $type,
                "title": status.canonical_reason().unwrap_or("unknown status code"),
                "status": $status,
                $($key: $value),*
            })),
            [
                ($crate::headers::CONTENT_TYPE, "problem+json"),
            ]
        )        
    }};
    
    (
        "title": $title:expr,
        "status": $status:expr 
        $(, $key:tt : $value:tt)* $(,)?
    ) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json($crate::json::json_internal!({
                "type": $crate::error::Problem::get_problem_type_url($status),
                "title": $title,
                "status": $status,
                $($key: $value),*
            })),
            [
                ($crate::headers::CONTENT_TYPE, "problem+json"),
            ]
        )        
    };
    
    (
        "type": $type:expr,
        "title": $title:expr,
        "status": $status:expr 
        $(, $key:tt : $value:tt)* $(,)?
    ) => {
        $crate::response!(
            $crate::http::StatusCode::from_u16($status).unwrap_or($crate::http::StatusCode::OK),
            $crate::HttpBody::json($crate::json::json_internal!({
                "type": $type,
                "title": $title,
                "status": $status,
                $($key: $value),*
            })),
            [
                ($crate::headers::CONTENT_TYPE, "problem+json"),
            ]
        )        
    };
}

/// Holds tools to work with Problem Details
#[allow(missing_debug_implementations)]
pub struct Problem;

impl Problem {
    /// Returns a URL to the RFC 9110 section depending on status code
    pub fn get_problem_type_url(status: u16) -> String {
        let minor = if status < 500 { 5 } else { 6 };
        let suffix = (status % 100) + 1;
        match status {
            421 => "https://tools.ietf.org/html/rfc9110#section-15.5.20".into(),
            422 => "https://tools.ietf.org/html/rfc9110#section-15.5.21".into(),
            426 => "https://tools.ietf.org/html/rfc9110#section-15.5.22".into(),
            _ => format!("https://tools.ietf.org/html/rfc9110#section-15.{minor}.{suffix}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    
    #[tokio::test]
    async fn it_correctly_parses_type_for_client_errors() {
        let client_errors = [
            (400, "https://tools.ietf.org/html/rfc9110#section-15.5.1"),
            (401, "https://tools.ietf.org/html/rfc9110#section-15.5.2"),
            (402, "https://tools.ietf.org/html/rfc9110#section-15.5.3"),
            (403, "https://tools.ietf.org/html/rfc9110#section-15.5.4"),
            (404, "https://tools.ietf.org/html/rfc9110#section-15.5.5"),
            (405, "https://tools.ietf.org/html/rfc9110#section-15.5.6"),
            (406, "https://tools.ietf.org/html/rfc9110#section-15.5.7"),
            (407, "https://tools.ietf.org/html/rfc9110#section-15.5.8"),
            (408, "https://tools.ietf.org/html/rfc9110#section-15.5.9"),
            (409, "https://tools.ietf.org/html/rfc9110#section-15.5.10"),
            (410, "https://tools.ietf.org/html/rfc9110#section-15.5.11"),
            (411, "https://tools.ietf.org/html/rfc9110#section-15.5.12"),
            (412, "https://tools.ietf.org/html/rfc9110#section-15.5.13"),
            (413, "https://tools.ietf.org/html/rfc9110#section-15.5.14"),
            (414, "https://tools.ietf.org/html/rfc9110#section-15.5.15"),
            (415, "https://tools.ietf.org/html/rfc9110#section-15.5.16"),
            (416, "https://tools.ietf.org/html/rfc9110#section-15.5.17"),
            (417, "https://tools.ietf.org/html/rfc9110#section-15.5.18"),
            (418, "https://tools.ietf.org/html/rfc9110#section-15.5.19"),
            (421, "https://tools.ietf.org/html/rfc9110#section-15.5.20"),
            (422, "https://tools.ietf.org/html/rfc9110#section-15.5.21"),
            (426, "https://tools.ietf.org/html/rfc9110#section-15.5.22"),
        ];

        assert(client_errors).await;
    }

    #[tokio::test]
    async fn it_correctly_parses_type_for_server_errors() {
        let server_errors = [
            (500, "https://tools.ietf.org/html/rfc9110#section-15.6.1"),
            (501, "https://tools.ietf.org/html/rfc9110#section-15.6.2"),
            (502, "https://tools.ietf.org/html/rfc9110#section-15.6.3"),
            (503, "https://tools.ietf.org/html/rfc9110#section-15.6.4"),
            (504, "https://tools.ietf.org/html/rfc9110#section-15.6.5"),
            (505, "https://tools.ietf.org/html/rfc9110#section-15.6.6"),
        ];

        assert(server_errors).await;
    }
    
    async fn assert<const N: usize>(test_cases: [(u16, &str); N]) {
        for (status, url) in test_cases{
            let mut problem_details = problem! { "status": status }.unwrap();
            let body = &problem_details.body_mut().collect().await.unwrap().to_bytes();
            assert_eq!(String::from_utf8_lossy(body), format!(
                "{{\"status\":{},\"title\":\"{}\",\"type\":\"{}\"}}",
                status,
                problem_details.status().canonical_reason().unwrap(),
                url
            ));

            assert_eq!(problem_details.status(), status);
        }
    }
}