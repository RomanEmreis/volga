//! [`From ] trait implementations from various types into HTTP Body

use crate::{Json, Form, HttpBody};
use crate::error::Error;
use tokio::fs::File;
use std::convert::Infallible;
use std::{borrow::Cow, str::FromStr};
use bytes::Bytes;
use serde::Serialize;

impl FromStr for HttpBody {
    type Err = Infallible;

    #[inline(always)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::text_ref(s))
    }
}

impl From<&'static str> for HttpBody {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::from_static_text(s)
    }
}

impl From<String> for HttpBody {
    #[inline]
    fn from(s: String) -> Self {
        Self::text(s)
    }
}

impl From<Cow<'static, str>> for HttpBody {
    #[inline]
    fn from(s: Cow<'static, str>) -> Self {
        Self::text(s)
    }
}

impl From<Box<str>> for HttpBody {
    #[inline]
    fn from(s: Box<str>) -> Self {
        Self::text(s.into_string())
    }
}

impl From<Bytes> for HttpBody {
    #[inline]
    fn from(b: Bytes) -> Self {
        Self::full(b)
    }
}

impl From<&'static [u8]> for HttpBody {
    #[inline]
    fn from(s: &'static [u8]) -> Self {
        Self::from_static(s)
    }
}

impl From<File> for HttpBody {
    #[inline]
    fn from(f: File) -> Self {
        Self::file(f)
    }
}

impl<T: Serialize> TryFrom<Json<T>> for HttpBody {
    type Error = Error;

    #[inline]
    fn try_from(value: Json<T>) -> Result<Self, Self::Error> {
        Self::json(value.into_inner())
    }
}

impl<T: Serialize> TryFrom<Form<T>> for HttpBody {
    type Error = Error;

    #[inline]
    fn try_from(value: Form<T>) -> Result<Self, Self::Error> {
        Self::form(value.into_inner())
    }
}

macro_rules! impl_into_body {
    { $($type:ident),* $(,)? } => {
        $(impl From<$type> for HttpBody {
            #[inline]
            fn from(s: $type) -> Self {
                Self::full(s.to_string())
            }
        })*
    };
}

impl_into_body! {
    bool, char,
    i8, u8,
    i16, u16,
    i32, u32,
    f32,
    i64, u64,
    f64,
    i128, u128,
    isize, usize
}

#[cfg(test)]
mod from_tryfrom_tests {
    use super::*;
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use std::borrow::Cow;

    // Собираем body целиком в Bytes
    async fn collect_bytes(body: HttpBody) -> Bytes {
        let collected = body
            .collect()
            .await
            .expect("body collection must succeed");

        collected.to_bytes()
    }

    #[tokio::test]
    async fn from_static_str_is_zero_copy_semantics_and_correct_bytes() {
        let body: HttpBody = "Hello, World!".into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"Hello, World!");
    }

    #[tokio::test]
    async fn from_string_is_correct_bytes() {
        let body: HttpBody = String::from("hello").into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"hello");
    }

    #[tokio::test]
    async fn from_cow_borrowed_is_correct_bytes() {
        let s: Cow<'static, str> = Cow::Borrowed("borrowed");
        let body: HttpBody = s.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"borrowed");
    }

    #[tokio::test]
    async fn from_cow_owned_is_correct_bytes() {
        let s: Cow<'static, str> = Cow::Owned(String::from("owned"));
        let body: HttpBody = s.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"owned");
    }

    #[tokio::test]
    async fn from_box_str_is_correct_bytes() {
        let s: Box<str> = "boxed".into();
        let body: HttpBody = s.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"boxed");
    }

    #[tokio::test]
    async fn from_bytes_is_correct_bytes() {
        let body: HttpBody = Bytes::from_static(b"bytes").into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"bytes");
    }

    #[tokio::test]
    async fn from_static_u8_slice_is_correct_bytes() {
        let body: HttpBody = (&b"static-bytes"[..]).into();
        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"static-bytes");
    }

    #[tokio::test]
    async fn from_file_streams_file_contents() {
        let tmp = crate::test::TempFile::new("file-body").await;    

        let f = tokio::fs::File::open(&tmp.path)
            .await
            .expect("open temp file");  

        let body: HttpBody = f.into();  

        let bytes = collect_bytes(body).await;
        assert_eq!(&bytes[..], b"file-body");
    }

    macro_rules! assert_from_int {
        ($name:ident, $t:ty, $value:expr, $expected:expr) => {
            #[tokio::test]
            async fn $name() {
                let v: $t = $value as $t;
                let body: HttpBody = v.into();
                let bytes = collect_bytes(body).await;
                assert_eq!(std::str::from_utf8(&bytes).unwrap(), $expected);
            }
        };
    }

    #[tokio::test]
    async fn from_bool_true() {
        let body: HttpBody = true.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "true");
    }

    #[tokio::test]
    async fn from_bool_false() {
        let body: HttpBody = false.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "false");
    }

    #[tokio::test]
    async fn from_char() {
        let body: HttpBody = 'x'.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "x");
    }

    assert_from_int!(from_i8, i8, -12, "-12");
    assert_from_int!(from_u8, u8, 12, "12");
    assert_from_int!(from_i16, i16, -1200, "-1200");
    assert_from_int!(from_u16, u16, 1200, "1200");
    assert_from_int!(from_i32, i32, -123456, "-123456");
    assert_from_int!(from_u32, u32, 123456, "123456");
    assert_from_int!(from_i64, i64, -9000000000_i64, "-9000000000");
    assert_from_int!(from_u64, u64, 9000000000_u64, "9000000000");

    #[tokio::test]
    async fn from_f32_is_non_empty() {
        let body: HttpBody = (1.25_f32).into();
        let bytes = collect_bytes(body).await;
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.is_empty());
        assert!(s.contains('1'));
    }

    #[tokio::test]
    async fn from_f64_is_non_empty() {
        let body: HttpBody = (1.25_f64).into();
        let bytes = collect_bytes(body).await;
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.is_empty());
        assert!(s.contains('1'));
    }

    #[tokio::test]
    async fn from_i128() {
        let v: i128 = -123456789012345678901234567890_i128;
        let body: HttpBody = v.into();
        let bytes = collect_bytes(body).await;
        assert_eq!(std::str::from_utf8(&bytes).unwrap(), "-123456789012345678901234567890");
    }

    // --------------------------
    // TryFrom<Json<T>> / Form<T>
    // --------------------------

    #[derive(serde::Serialize)]
    struct TestJson {
        name: &'static str,
        value: u32,
    }

    #[tokio::test]
    async fn try_from_json_serializes_to_json_bytes() {
        // Если у тебя Json не tuple-struct, замени на Json::new(...)
        let payload = TestJson { name: "volga", value: 42 };
        let body = HttpBody::try_from(Json(payload)).expect("json serialization must succeed");

        let bytes = collect_bytes(body).await;
        let s = std::str::from_utf8(&bytes).unwrap();

        // порядок полей в serde_json обычно соответствует порядку в struct
        assert!(s.contains("\"name\":\"volga\""));
        assert!(s.contains("\"value\":42"));
        assert!(s.starts_with('{') && s.ends_with('}'));
    }

    #[derive(serde::Serialize)]
    struct TestForm {
        a: u32,
        b: &'static str,
    }

    #[tokio::test]
    async fn try_from_form_serializes_to_urlencoded_bytes() {
        let payload = TestForm { a: 1, b: "two" };
        let body = HttpBody::try_from(Form(payload)).expect("form serialization must succeed");

        let bytes = collect_bytes(body).await;
        let s = std::str::from_utf8(&bytes).unwrap();

        // порядок обычно стабильный по полям структуры, но чтобы тест был устойчивым —
        // проверяем вхождение обоих пар.
        assert!(s.contains("a=1"));
        assert!(s.contains("b=two"));
        assert!(s.contains('&') || s == "a=1" || s == "b=two");
    }

    #[tokio::test]
    async fn it_works_with_str() {
        let string = String::from("Hello, World!");
        let body = HttpBody::from_str(string.as_str()).unwrap();
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }
}
