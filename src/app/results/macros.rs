﻿/// Produces an `OK 200` response with plain text or JSON body
/// 
/// # Examples
/// ## plain/text
/// ```no_run
/// use volga::ok;
///
/// ok!("healthy");
/// ```
/// ## plain/text without body
/// ```no_run
/// use volga::ok;
///
/// ok!();
/// ```
/// ## JSON
///```no_run
/// use volga::ok;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Health {
///    status: String
/// }
///
/// let health = Health { status: "healthy".into() };
/// ok!(health);
/// ```
/// ## Untyped JSON with custom headers
///```no_run
/// use volga::ok;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Health {
///    status: String
/// }
///
/// ok!({ "health": "healthy" }, [
///    ("x-api-key", "some api key")
/// ]);
/// ```
#[macro_export]
macro_rules! ok {
    // handles ok!()
    () => {
        $crate::Results::ok()
    };
    
    // handles ok!({ json })
    ({ $($json:tt)* }) => {
        $crate::Results::json(serde_json::json_internal!({ $($json)* }))
    };
    
    // handles ok!({ json }, [("key", "val")])
    ({ $($json:tt)* }, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        // We're not using a headers! macro here to avoid adding unnecessary use if it's not needed
        let mut headers = $crate::HttpHeaders::new();
        $(
            headers.insert($key.to_string(), $value.to_string());
        )*
        $crate::Results::from($crate::ResponseContext {
            status: 200,
            content: serde_json::json_internal!({ $($json)* }),
            headers
        })
    }};
    
    // handles ok!(json)
    ($var:ident) => {
        $crate::Results::json($var)
    };
    
    // handles ok!(json, [("key", "val")])
    ($e:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        // We're not using a headers! macro here to avoid adding unnecessary use if it's not needed
        let mut headers = $crate::HttpHeaders::new();
        $(
            headers.insert($key.to_string(), $value.to_string());
        )*
        $crate::Results::from($crate::ResponseContext {
            status: 200,
            content: $e,
            headers
        })
    }};
    
    // handles ok!("Hello {name}")
    ($fmt:tt) => {
        $crate::Results::json(format!($fmt))
    };
    
    // handles ok!(thing.to_string()) or ok!(5 + 5)
    ($e:expr) => {
        $crate::Results::json($e)
    };
    
    // handles ok!("Hello {}", name)
    ($($fmt:tt)*) => {
        $crate::Results::json(format!($($fmt)*))
    };
}

/// Produces `OK 200` response with file body
/// 
/// > This macro is working only within async context, make sure that you are using `AsyncEndpointsMapping`
/// 
/// # Examples
/// ## Default usage
///```no_run
/// use volga::file;
/// use tokio::fs::File;
///
/// # async fn dox() -> std::io::Result<()> {
/// let file_name = "example.txt";
/// let file_data = File::open(file_name).await?;
///
/// file!(file_name, file_data);
/// # Ok(())
/// # }
/// ```
/// ## Custom headers
///```no_run
/// use volga::{file, App, Router};
/// use tokio::fs::File;
///
/// # async fn dox() -> std::io::Result<()> {
/// let file_name = "example.txt";
/// let file_data = File::open(file_name).await?;
/// 
/// file!(file_name, file_data, [
///    ("x-api-key", "some api key")
/// ]);
/// # Ok(())   
/// # }
/// ```
#[macro_export]
macro_rules! file {
    ($file_name:expr, $e:expr) => {
        $crate::Results::file($file_name, $e).await
    };
    ($file_name:expr, $e:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {{
        // We're not using a headers! macro here to avoid adding unnecessary use if it's not needed
        let mut headers = $crate::HttpHeaders::new();
        $(
            headers.insert($key.to_string(), $value.to_string());
        )*
        $crate::Results::file_with_custom_headers($file_name, $e, headers).await
    }};
}

/// Creates HTTP Request/Response headers
/// # Examples
///```no_run
///use volga::headers;
///
///let headers = headers![
///    ("header 1", "value 1"),
///    ("header 2", "value 2"),
///];
/// ```
#[macro_export]
macro_rules! headers {
    ( $( ($key:expr, $value:expr) ),* $(,)? ) => {{
        let mut headers = $crate::HttpHeaders::new();
        $(
            headers.insert($key.to_string(), $value.to_string());
        )*
        headers
    }};
}

/// Produces a response with specified `StatusCode` with plain text or JSON body
/// 
/// # Examples
/// ## Without body
/// ```no_run
/// use volga::status;
///
/// status!(404);
/// ```
/// ## plain/text body
/// ```no_run
/// use volga::status;
///
/// status!(401, "Unauthorized!");
/// ```
/// ## JSON body
/// ```no_run
/// use volga::status;
/// use serde::Serialize;
/// 
/// #[derive(Serialize)]
/// struct ErrorMessage {
///     error: String
/// }
/// 
/// let error = ErrorMessage { error: "some error message".into() };
/// status!(401, error);
/// ```
#[macro_export]
macro_rules! status {
    (200) => {
        $crate::Results::ok()
    };
    (400) => {
        $crate::Results::bad_request(None)
    };
    (404) => {
        $crate::Results::not_found()
    };
    ($status:expr, { $($json:tt)* }) => {
        $crate::Results::json_with_status(
            hyper::StatusCode::from_u16($status).unwrap_or(hyper::StatusCode::OK), 
            serde_json::json_internal!({ $($json)* }))
    };
    ($status:expr) => {
        $crate::Results::status(
            hyper::StatusCode::from_u16($status).unwrap_or(hyper::StatusCode::OK), 
            mime::TEXT_PLAIN.as_ref(), 
            $crate::app::body::HttpBody::empty())
    };
    ($status:expr, $e:expr) => {
        $crate::Results::json_with_status(
            hyper::StatusCode::from_u16($status).unwrap_or(hyper::StatusCode::OK),
            $e)
    };
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use std::path::Path;
    use serde::Serialize;
    use tokio::fs::File;
    use crate::test_utils::read_file_bytes;

    #[derive(Serialize)]
    struct TestPayload {
        name: String
    }
    
    #[tokio::test]
    async fn it_creates_json_ok_response() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(payload);
        
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_json_from_inline_struct_ok_response() {
        let response = ok!(TestPayload { name: "test".into() });

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_json_ok_response() {
        let response = ok!({ "name": "test" });

        assert!(response.is_ok());
        
        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }
    
    #[tokio::test]
    async fn it_creates_text_ok_response() {
        let text = "test";
        let response = ok!(text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
    }

    #[tokio::test]
    async fn it_creates_literal_text_ok_response() {
        let response = ok!("test");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
    }

    #[tokio::test]
    async fn it_creates_expr_ok_response() {
        let response = ok!(5 + 5);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "10");
    }

    #[tokio::test]
    async fn it_creates_number_ok_response() {
        let number = 100;
        let response = ok!(number);
        // this is known issue will be fixed in future releases.
        //let response = ok!(100);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "100");
    }

    #[tokio::test]
    async fn it_creates_boolean_ok_response() {
        let response = ok!(true);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "true");
    }

    #[tokio::test]
    async fn it_creates_char_ok_response() {
        let ch = 'a';
        let response = ok!(ch);
        // this is known issue will be fixed in future releases.
        //let response = ok!('a');

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"a\"");
    }

    #[tokio::test]
    async fn it_creates_array_ok_response() {
        let vec = vec![1,2,3];
        let response = ok!(vec);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "[1,2,3]");
    }

    #[tokio::test]
    async fn it_creates_formatted_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {}", text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"This is text: test\"");
    }

    #[tokio::test]
    async fn it_creates_interpolated_text_ok_response() {
        let text = "test";
        let response = ok!("This is text: {text}");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"This is text: test\"");
    }
    
    #[tokio::test]
    async fn it_creates_empty_ok_response() {
        let response = ok!();

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }
    
    #[tokio::test]
    async fn it_creates_file_with_ok_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        let file = File::open(path).await.unwrap();
        
        let response = file!(file_name, file);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;
        
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
    }

    #[tokio::test]
    async fn it_creates_file_with_ok_and_custom_headers_response() {
        let path = Path::new("tests/resources/test_file.txt");
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        
        let file = File::open(path).await.unwrap();

        let response = file!(file_name, file, [
            ("x-api-key", "some api key")
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = read_file_bytes(&mut response).await;
        
        assert_eq!(String::from_utf8_lossy(body.as_slice()), "Hello, this is some file content!");
    }

    #[tokio::test]
    async fn it_creates_200_response() {
        let response = status!(200);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_creates_200_with_text_response() {
        let text = "test";
        let response = status!(200, text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
    }

    #[tokio::test]
    async fn it_creates_200_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(200, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_200_response_with_json_body() {
        let response = status!(200, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_400_response() {
        let response = status!(400);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_creates_400_with_text_response() {
        let text = "test";
        let response = status!(400, text);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"test\"");
    }

    #[tokio::test]
    async fn it_creates_400_with_json_response() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(400, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_400_response_with_json_body() {
        let response = status!(400, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }
    
    #[tokio::test]
    async fn it_creates_404_response() {
        let response = status!(404);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_creates_empty_401_response() {
        let response = status!(401);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_creates_401_response_with_text_body() {
        let response = status!(401, "You are not authorized!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"You are not authorized!\"");
    }

    #[tokio::test]
    async fn it_creates_401_response_with_json_body() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(401, payload);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_401_response_with_json_body() {
        let response = status!(401, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_empty_403_response() {
        let response = status!(403);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(body.len(), 0);
    }

    #[tokio::test]
    async fn it_creates_403_response_with_text_body() {
        let response = status!(403, "It's forbidden!");

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"It's forbidden!\"");
    }

    #[tokio::test]
    async fn it_creates_403_response_with_json_body() {
        let payload = TestPayload { name: "test".into() };
        let response = status!(403, payload);
        
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }

    #[tokio::test]
    async fn it_creates_anonymous_type_403_response_with_json_body() {
        let response = status!(403, { "name": "test" });

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
    }
    
    #[tokio::test]
    async fn it_creates_headers() {
        let headers = headers![
            ("header 1", "value 1"),
            ("header 2", "value 2")
        ];
        
        assert_eq!(headers.get("header 1").unwrap(), "value 1");
        assert_eq!(headers.get("header 2").unwrap(), "value 2")
    }
    
    #[tokio::test]
    async fn in_creates_text_response_with_custom_headers() {
        let response = ok!("ok", [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);
        
        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();
        
        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_text_response_with_empty_custom_headers() {
        #[allow(unused_mut)]
        let response = ok!("ok", []);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "\"ok\"");
        assert_eq!(response.headers().len(), 2);
    }

    #[tokio::test]
    async fn in_creates_json_response_with_custom_headers() {
        let payload = TestPayload { name: "test".into() };
        let response = ok!(payload, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }

    #[tokio::test]
    async fn in_creates_anonymous_json_response_with_custom_headers() {
        let response = ok!({ "name": "test" }, [
            ("x-api-key", "some api key"),
            ("x-req-id", "some req id"),
        ]);

        assert!(response.is_ok());

        let mut response = response.unwrap();
        let body = &response.body_mut().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(body), "{\"name\":\"test\"}");
        assert_eq!(response.headers().get("x-api-key").unwrap(), "some api key");
        assert_eq!(response.headers().get("x-req-id").unwrap(), "some req id");
    }
}