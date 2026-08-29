#![allow(missing_docs)]
#![cfg(feature = "test")]

use serde::{Deserialize, Serialize};
use volga::validation::{Valid, ValidJson, ValidQuery, Validate, ValidationError};
use volga::{HttpResult, Json, error::Error, http::StatusCode, ok, status, test::TestServer};

#[derive(Serialize, Deserialize)]
struct KeyValue {
    key: String,
    value: String,
}

impl Validate for KeyValue {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        let mut err = ValidationError::new();
        if self.key.is_empty() {
            err.push("key", "key is required");
        }
        if self.value.len() > 8 {
            err.push("value", "value is too long");
        }
        err.into_result()
    }
}

#[derive(Deserialize)]
struct Filter {
    per_page: u32,
}

impl Validate for Filter {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.per_page == 0 || self.per_page > 100 {
            return Err(ValidationError::field(
                "per_page",
                "must be between 1 and 100",
            ));
        }
        Ok(())
    }
}

fn key_value(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: value.into(),
    }
}

#[tokio::test]
async fn it_passes_a_valid_body_to_the_handler() {
    let server = TestServer::spawn(|app| {
        app.map_post("/put", async |val: ValidJson<KeyValue>| {
            ok!("{}={}", val.key, val.value)
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json(&key_value("name", "John"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "name=John");

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_an_invalid_body_before_the_handler() {
    let server = TestServer::spawn(|app| {
        app.map_post("/put", async |_: ValidJson<KeyValue>| -> HttpResult {
            unreachable!("the handler must not be called for an invalid payload")
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json(&key_value("", "0123456789"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.text().await.unwrap(),
        "key: key is required; value: value is too long"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_validates_query_parameters() {
    let server = TestServer::spawn(|app| {
        app.map_get("/items", async |filter: ValidQuery<Filter>| {
            ok!("{}", filter.per_page)
        });
    })
    .await;

    let client = server.client();

    let response = client
        .get(server.url("/items?per_page=25"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "25");

    let response = client
        .get(server.url("/items?per_page=1000000"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.text().await.unwrap(),
        "per_page: must be between 1 and 100"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_validates_the_query_and_the_body_of_the_same_request() {
    let server = TestServer::spawn(|app| {
        app.map_post(
            "/items",
            async |filter: ValidQuery<Filter>, val: ValidJson<KeyValue>| {
                ok!("{}:{}", filter.per_page, val.key)
            },
        );
    })
    .await;

    let client = server.client();

    let response = client
        .post(server.url("/items?per_page=25"))
        .json(&key_value("name", "John"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "25:name");

    // the query is extracted first, so it is the one that short-circuits
    let response = client
        .post(server.url("/items?per_page=0"))
        .json(&key_value("", "John"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.text().await.unwrap(),
        "per_page: must be between 1 and 100"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_composes_with_option() {
    let server = TestServer::spawn(|app| {
        app.map_post("/put", async |val: Option<ValidJson<KeyValue>>| match val {
            Some(val) => ok!("{}", val.key),
            None => ok!("none"),
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json(&key_value("", "John"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "none");

    server.shutdown().await;
}

#[tokio::test]
async fn it_composes_with_result() {
    let server = TestServer::spawn(|app| {
        app.map_post(
            "/put",
            async |val: Result<ValidJson<KeyValue>, Error>| match val {
                Ok(val) => ok!("{}", val.key),
                Err(err) => status!(422, "{err}"),
            },
        );
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json(&key_value("", "John"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 422);
    assert_eq!(response.text().await.unwrap(), "key: key is required");

    server.shutdown().await;
}

#[tokio::test]
async fn it_reports_a_custom_status() {
    #[derive(Deserialize)]
    struct Strict(String);

    impl Validate for Strict {
        type Error = ValidationError;

        fn validate(&self) -> Result<(), Self::Error> {
            Err(
                ValidationError::message(format!("`{}` is never valid", self.0))
                    .with_status(StatusCode::CONFLICT),
            )
        }
    }

    let server = TestServer::spawn(|app| {
        app.map_post("/put", async |_: Valid<Json<Strict>>| -> HttpResult {
            unreachable!("the handler must not be called for an invalid payload")
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json("anything")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert_eq!(response.text().await.unwrap(), "`anything` is never valid");

    server.shutdown().await;
}

#[cfg(feature = "problem-details")]
#[tokio::test]
async fn it_renders_problem_details() {
    let server = TestServer::spawn(|app| {
        app.use_problem_details();
        app.map_post("/put", async |_: ValidJson<KeyValue>| -> HttpResult {
            unreachable!("the handler must not be called for an invalid payload")
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/put"))
        .json(&key_value("", "0123456789"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );

    let problem: serde_json::Value = response.json().await.unwrap();

    assert_eq!(problem["status"], 400);
    assert_eq!(problem["errors"]["key"][0], "key is required");
    assert_eq!(problem["errors"]["value"][0], "value is too long");

    server.shutdown().await;
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn it_keeps_the_wrapped_extractor_in_the_openapi_spec() {
    let server = TestServer::builder()
        .configure(|app| app.with_open_api(|config| config.with_title("Validation")))
        .setup(|app| {
            app.map_get("/items", async |filter: ValidQuery<Filter>| {
                ok!("{}", filter.per_page)
            });
            app.map_post("/put", async |val: ValidJson<KeyValue>| ok!("{}", val.key));
            app.use_open_api();
        })
        .build()
        .await;

    let spec: serde_json::Value = server
        .client()
        .get(server.url("/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let params = &spec["paths"]["/items"]["get"]["parameters"];

    assert_eq!(params[0]["name"], "per_page");
    assert_eq!(params[0]["in"], "query");
    assert!(
        !spec["paths"]["/put"]["post"]["requestBody"]["content"]["application/json"].is_null(),
        "the JSON request body must stay in the spec"
    );

    server.shutdown().await;
}
