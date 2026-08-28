#![allow(missing_docs)]
#![cfg(feature = "validation-derive")]

use serde::Deserialize;
use volga::validation::{
    Constraint, ConstraintKind, NumericBound, Validate, ValidationError, rules::length as len_of,
};

#[derive(Deserialize, Validate)]
struct KeyValue {
    #[validate(length(min = 1, max = 8))]
    key: String,

    #[validate(length(max = 4))]
    tags: Vec<String>,

    #[validate(length(equal = 3))]
    code: String,
}

#[derive(Deserialize, Validate)]
struct Filter {
    #[validate(range(min = 1, max = 100))]
    per_page: u32,

    #[validate(range(min = -10))]
    offset: i32,

    #[validate(range(max = 1.5))]
    ratio: f64,

    #[validate(length(min = 1))]
    sort: Option<String>,
}

#[test]
fn it_reports_the_length_rules() {
    let payload = KeyValue {
        key: String::new(),
        tags: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        code: "ab".into(),
    };

    let err = payload.validate().unwrap_err();

    assert_eq!(
        err.entries().collect::<Vec<_>>(),
        vec![
            (Some("key"), "length must be between 1 and 8"),
            (Some("tags"), "length must be at most 4"),
            (Some("code"), "length must be exactly 3"),
        ]
    );
}

#[test]
fn it_accepts_a_valid_payload() {
    let payload = KeyValue {
        key: "name".into(),
        tags: vec!["a".into()],
        code: "abc".into(),
    };

    assert!(payload.validate().is_ok());
}

#[test]
fn it_measures_strings_in_characters() {
    // eight characters, more than eight bytes
    let payload = KeyValue {
        key: "привет!!".into(),
        tags: Vec::new(),
        code: "abc".into(),
    };

    assert_eq!(len_of(&payload.key), 8);
    assert!(payload.validate().is_ok());
}

#[test]
fn it_reports_the_range_rules() {
    let payload = Filter {
        per_page: 1000,
        offset: -20,
        ratio: 2.0,
        sort: None,
    };

    let err = payload.validate().unwrap_err();

    assert_eq!(
        err.entries().collect::<Vec<_>>(),
        vec![
            (Some("per_page"), "must be between 1 and 100"),
            (Some("offset"), "must be at least -10"),
            (Some("ratio"), "must be at most 1.5"),
        ]
    );
}

#[test]
fn it_skips_a_none_and_checks_a_some() {
    let valid = Filter {
        per_page: 10,
        offset: 0,
        ratio: 1.0,
        sort: None,
    };
    assert!(valid.validate().is_ok());

    let invalid = Filter {
        sort: Some(String::new()),
        ..valid
    };

    assert_eq!(
        invalid.validate().unwrap_err().to_string(),
        "sort: length must be at least 1"
    );
}

#[test]
fn it_publishes_the_constraints() {
    assert_eq!(
        KeyValue::constraints(),
        &[
            Constraint::new("key", ConstraintKind::MinSize(1)),
            Constraint::new("key", ConstraintKind::MaxSize(8)),
            Constraint::new("tags", ConstraintKind::MaxSize(4)),
            Constraint::new("code", ConstraintKind::MinSize(3)),
            Constraint::new("code", ConstraintKind::MaxSize(3)),
        ]
    );
    assert_eq!(
        Filter::constraints(),
        &[
            Constraint::new("per_page", ConstraintKind::Minimum(NumericBound::Int(1))),
            Constraint::new("per_page", ConstraintKind::Maximum(NumericBound::Int(100))),
            Constraint::new("offset", ConstraintKind::Minimum(NumericBound::Int(-10))),
            Constraint::new("ratio", ConstraintKind::Maximum(NumericBound::Float(1.5))),
            Constraint::new("sort", ConstraintKind::MinSize(1)),
        ]
    );
}

#[test]
fn it_leaves_the_constraints_empty_for_a_hand_written_impl() {
    struct Plain;

    impl Validate for Plain {
        type Error = ValidationError;

        fn validate(&self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    assert!(Plain::constraints().is_empty());
}

#[derive(Deserialize, Validate)]
struct Item {
    #[validate(length(min = 1))]
    name: String,
}

#[derive(Deserialize, Validate)]
struct Order {
    #[validate(nested)]
    head: Item,

    #[validate(nested)]
    items: Vec<Item>,

    #[validate(nested)]
    note: Option<Item>,
}

#[test]
fn it_merges_nested_failures_under_the_field() {
    let order = Order {
        head: Item {
            name: String::new(),
        },
        items: vec![
            Item { name: "ok".into() },
            Item {
                name: String::new(),
            },
        ],
        note: Some(Item {
            name: String::new(),
        }),
    };

    let err = order.validate().unwrap_err();

    assert_eq!(
        err.entries().collect::<Vec<_>>(),
        vec![
            (Some("head.name"), "length must be at least 1"),
            (Some("items[1].name"), "length must be at least 1"),
            (Some("note.name"), "length must be at least 1"),
        ]
    );
}

#[test]
fn it_passes_a_valid_nested_payload() {
    let order = Order {
        head: Item {
            name: "head".into(),
        },
        items: vec![Item { name: "one".into() }],
        note: None,
    };

    assert!(order.validate().is_ok());
}

fn is_supported_sort(value: &str) -> Result<(), ValidationError> {
    match value {
        "asc" | "desc" => Ok(()),
        other => Err(ValidationError::message(format!(
            "`{other}` is not a sort order"
        ))),
    }
}

fn from_is_before_to(page: &Page) -> Result<(), ValidationError> {
    if page.from > page.to {
        return Err(ValidationError::field("from", "must not be after `to`"));
    }
    Ok(())
}

#[derive(Deserialize, Validate)]
#[validate(schema = "from_is_before_to")]
struct Page {
    #[validate(range(min = 0))]
    from: u32,

    to: u32,

    #[validate(custom = "is_supported_sort")]
    sort: String,

    #[validate(length(min = 1, message = "key is required"))]
    key: String,
}

#[test]
fn it_runs_the_custom_check_and_the_cross_field_rule() {
    let page = Page {
        from: 5,
        to: 1,
        sort: "sideways".into(),
        key: String::new(),
    };

    let err = page.validate().unwrap_err();

    assert_eq!(
        err.entries().collect::<Vec<_>>(),
        vec![
            (Some("sort"), "`sideways` is not a sort order"),
            (Some("key"), "key is required"),
            (Some("from"), "must not be after `to`"),
        ]
    );
}

#[test]
fn it_passes_when_the_cross_field_rule_holds() {
    let page = Page {
        from: 1,
        to: 5,
        sort: "asc".into(),
        key: "k".into(),
    };

    assert!(page.validate().is_ok());
}

#[derive(Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
struct Renamed {
    #[validate(length(min = 1))]
    page_size: String,

    #[serde(rename = "sortOrder")]
    #[validate(length(min = 1))]
    sort: String,

    #[serde(rename = "ignored")]
    #[validate(rename = "explicit", length(min = 1))]
    overridden: String,
}

#[test]
fn it_reports_the_name_the_client_sent() {
    let payload = Renamed {
        page_size: String::new(),
        sort: String::new(),
        overridden: String::new(),
    };

    let err = payload.validate().unwrap_err();

    assert_eq!(
        err.entries().map(|(field, _)| field).collect::<Vec<_>>(),
        vec![Some("pageSize"), Some("sortOrder"), Some("explicit")]
    );
    assert_eq!(
        Renamed::constraints()[0],
        Constraint::new("pageSize", ConstraintKind::MinSize(1))
    );
}

#[cfg(all(feature = "test", feature = "openapi"))]
#[tokio::test]
async fn it_publishes_the_constraints_in_the_openapi_spec() {
    use volga::{ValidJson, ValidQuery, ok, test::TestServer};

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

    let params = spec["paths"]["/items"]["get"]["parameters"]
        .as_array()
        .expect("query parameters")
        .iter()
        .map(|p| (p["name"].as_str().unwrap().to_owned(), p["schema"].clone()))
        .collect::<std::collections::HashMap<_, _>>();

    // A whole bound is published whole, a fractional one as written
    assert_eq!(params["per_page"]["minimum"], 1);
    assert_eq!(params["per_page"]["maximum"], 100);
    assert_eq!(params["offset"]["minimum"], -10);
    assert_eq!(params["ratio"]["maximum"], 1.5);
    assert_eq!(params["sort"]["minLength"], 1);

    // The body schema is hoisted into the components, so the constraints travel with it
    assert_eq!(
        spec["paths"]["/put"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/KeyValue"
    );
    let body = &spec["components"]["schemas"]["KeyValue"]["properties"];

    assert_eq!(body["key"]["minLength"], 1);
    assert_eq!(body["key"]["maxLength"], 8);
    // A collection is published under the array keywords - `maxLength` counts characters
    assert_eq!(body["tags"]["maxItems"], 4);
    assert!(body["tags"]["maxLength"].is_null());

    server.shutdown().await;
}

#[derive(Deserialize, Validate)]
struct Bounded {
    #[validate(range(min = 0.0))]
    at_least: f64,

    #[validate(range(max = 1.5))]
    at_most: f64,

    #[validate(range(min = 0.0, max = 1.0))]
    between: f64,
}

#[test]
fn it_rejects_a_value_no_bound_holds_for() {
    // Every comparison against `NaN` is false, so a rule phrased as "violates the bound"
    // would let it through - one-sided rules included
    let payload = Bounded {
        at_least: f64::NAN,
        at_most: f64::NAN,
        between: f64::NAN,
    };

    let err = payload.validate().unwrap_err();

    assert_eq!(
        err.entries().collect::<Vec<_>>(),
        vec![
            (Some("at_least"), "must be at least 0.0"),
            (Some("at_most"), "must be at most 1.5"),
            (Some("between"), "must be between 0.0 and 1.0"),
        ]
    );
}

#[test]
fn it_still_accepts_the_bounds_themselves() {
    let payload = Bounded {
        at_least: 0.0,
        at_most: 1.5,
        between: 1.0,
    };

    assert!(payload.validate().is_ok());

    let payload = Bounded {
        at_least: -0.1,
        ..payload
    };

    assert_eq!(
        payload.validate().unwrap_err().to_string(),
        "at_least: must be at least 0.0"
    );
}

#[derive(Deserialize, Validate)]
struct Shapes {
    #[validate(length(min = 1, max = 8))]
    text: String,

    #[validate(length(max = 4))]
    list: Vec<u8>,

    #[validate(length(max = 4))]
    set: std::collections::HashSet<u8>,

    #[validate(length(min = 1))]
    map: std::collections::HashMap<String, u8>,

    #[validate(length(max = 2))]
    maybe_list: Option<Vec<u8>>,
}

#[test]
fn it_publishes_one_size_rule_per_shape() {
    // The macro no longer guesses the keyword - it says "size", and the schema, which is
    // the only place the shape is known, publishes it as length, items or properties
    assert_eq!(
        Shapes::constraints(),
        &[
            Constraint::new("text", ConstraintKind::MinSize(1)),
            Constraint::new("text", ConstraintKind::MaxSize(8)),
            Constraint::new("list", ConstraintKind::MaxSize(4)),
            Constraint::new("set", ConstraintKind::MaxSize(4)),
            Constraint::new("map", ConstraintKind::MinSize(1)),
            Constraint::new("maybe_list", ConstraintKind::MaxSize(2)),
        ]
    );
}

#[test]
fn it_enforces_every_shape_at_runtime() {
    let payload = Shapes {
        text: String::new(),
        list: vec![1, 2, 3, 4, 5],
        set: std::collections::HashSet::from([1, 2, 3, 4, 5]),
        map: std::collections::HashMap::new(),
        maybe_list: Some(vec![1, 2, 3]),
    };

    let err = payload.validate().unwrap_err();

    assert_eq!(
        err.entries().map(|(field, _)| field).collect::<Vec<_>>(),
        vec![
            Some("text"),
            Some("list"),
            Some("set"),
            Some("map"),
            Some("maybe_list")
        ]
    );
}

const MAX_PAGE: u32 = 100;

#[derive(Deserialize, Validate)]
struct Constant {
    // A non-literal bound has no text for a default message, so one has to be given -
    // the value itself is still known to const evaluation and still published
    #[validate(range(max = MAX_PAGE, message = "must be at most one page"))]
    page: u32,
}

#[test]
fn it_publishes_a_constant_bound() {
    assert_eq!(
        Constant::constraints(),
        &[Constraint::new(
            "page",
            ConstraintKind::Maximum(NumericBound::Float(100.0)),
        )]
    );
    assert_eq!(
        Constant { page: 101 }.validate().unwrap_err().to_string(),
        "page: must be at most one page"
    );
    assert!(Constant { page: 100 }.validate().is_ok());
}

#[cfg(all(feature = "test", feature = "openapi"))]
mod spec {
    use super::*;
    use volga::{ValidJson, ValidQuery, ok, test::TestServer};

    #[derive(Deserialize, Validate)]
    struct Collide {
        // Same wire name as `KeyValue::key`, different rule and a different location
        #[validate(range(min = 10, max = 20))]
        key: u32,
    }

    async fn spec_of(setup: impl FnOnce(&mut volga::App) + Send + 'static) -> serde_json::Value {
        let server = TestServer::builder()
            .configure(|app| app.with_open_api(|config| config.with_title("Validation")))
            .setup(|app| {
                setup(app);
                app.use_open_api();
            })
            .build()
            .await;

        let spec = server
            .client()
            .get(server.url("/openapi.json"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        server.shutdown().await;
        spec
    }

    #[tokio::test]
    async fn it_keeps_each_extractor_s_constraints_to_itself() {
        let spec = spec_of(|app| {
            app.map_post(
                "/collide",
                async |query: ValidQuery<Collide>, body: ValidJson<KeyValue>| {
                    ok!("{}{}", query.key, body.key)
                },
            );
        })
        .await;

        let parameter = &spec["paths"]["/collide"]["post"]["parameters"][0];
        assert_eq!(parameter["name"], "key");
        assert_eq!(parameter["schema"]["minimum"], 10);
        assert_eq!(parameter["schema"]["maximum"], 20);
        // The body's rule for the same name must not have leaked onto the query parameter
        assert!(parameter["schema"]["minLength"].is_null());

        let property = &spec["components"]["schemas"]["KeyValue"]["properties"]["key"];
        assert_eq!(property["minLength"], 1);
        assert_eq!(property["maxLength"], 8);
        // ... nor the query's rule onto the body property
        assert!(property["minimum"].is_null());
    }

    #[tokio::test]
    async fn it_publishes_the_tighter_of_two_rules() {
        let spec = spec_of(|app| {
            app.map_post("/twice", async |body: ValidJson<Twice>| {
                ok!("{}", body.page)
            });
        })
        .await;

        let twice = &spec["components"]["schemas"]["Twice"]["properties"];

        // The order the rules were written in must not decide what the schema says
        assert_eq!(twice["page"]["minimum"], 10);
        assert_eq!(twice["name"]["maxLength"], 8);
    }

    #[tokio::test]
    async fn it_publishes_a_collection_hidden_behind_an_alias_as_a_collection() {
        let spec = spec_of(|app| {
            app.map_post("/aliased", async |body: ValidJson<Aliased>| {
                ok!("{}", body.tags.len())
            });
        })
        .await;

        let tags = &spec["components"]["schemas"]["Aliased"]["properties"]["tags"];

        assert_eq!(tags["type"], "array");
        assert_eq!(tags["maxItems"], 4);
        assert!(tags["maxLength"].is_null());
    }

    #[tokio::test]
    async fn it_publishes_the_constraints_of_a_generic_nested_field() {
        let spec = spec_of(|app| {
            app.map_post("/envelope", async |body: ValidJson<Envelope<Item>>| {
                ok!("{}", body.item.name)
            });
        })
        .await;

        let envelope = spec["components"]["schemas"]
            .as_object()
            .expect("components")
            .values()
            .find(|schema| schema["properties"]["item"].is_object())
            .expect("the envelope schema");

        assert_eq!(
            envelope["properties"]["item"]["properties"]["name"]["minLength"],
            1
        );
    }

    #[tokio::test]
    async fn it_publishes_the_constraints_of_a_nested_type() {
        let spec = spec_of(|app| {
            app.map_post("/order", async |order: ValidJson<Order>| {
                ok!("{}", order.head.name)
            });
        })
        .await;

        let order = &spec["components"]["schemas"]["Order"]["properties"];

        // The nested type enforces its own rule, so the schema has to say so
        assert_eq!(order["head"]["properties"]["name"]["minLength"], 1);
        assert_eq!(
            order["items"]["items"]["properties"]["name"]["minLength"],
            1
        );
        assert_eq!(order["note"]["properties"]["name"]["minLength"], 1);
    }
}

#[derive(Deserialize, Validate)]
struct Twice {
    // Both rules run, so the field really is bounded by the tighter of the two
    #[validate(range(min = 1))]
    #[validate(range(min = 10))]
    page: u32,

    #[validate(length(max = 64))]
    #[validate(length(max = 8))]
    name: String,
}

#[test]
fn it_enforces_and_publishes_the_tighter_of_two_rules() {
    assert!(
        Twice {
            page: 5,
            name: "ok".into()
        }
        .validate()
        .is_err()
    );
    assert!(
        Twice {
            page: 10,
            name: "0123456789".into()
        }
        .validate()
        .is_err()
    );
    assert!(
        Twice {
            page: 10,
            name: "ok".into()
        }
        .validate()
        .is_ok()
    );
}

type Tags = Vec<String>;

#[derive(Deserialize, Validate)]
struct Aliased {
    // The type is spelled as an alias, so nothing upstream of the schema can tell it is a
    // collection - the keyword has to be chosen where the shape is known
    #[validate(length(max = 4))]
    tags: Tags,
}

#[derive(Deserialize, Validate)]
struct Envelope<T>
where
    T: Validate<Error = ValidationError>,
{
    #[validate(nested)]
    item: T,
}

#[derive(Deserialize, Validate)]
struct Huge {
    // Past `2^53`, where an `f64` can no longer hold every whole number
    #[validate(range(min = 9007199254740993))]
    id: u64,

    // Past `i64::MAX`, where only an unsigned bound reaches
    #[validate(range(min = 9223372036854775809))]
    unsigned: u64,
}

#[test]
fn it_publishes_a_whole_bound_without_moving_it() {
    assert_eq!(
        Huge::constraints(),
        &[
            Constraint::new(
                "id",
                ConstraintKind::Minimum(NumericBound::Int(9007199254740993))
            ),
            Constraint::new(
                "unsigned",
                ConstraintKind::Minimum(NumericBound::UInt(9223372036854775809))
            ),
        ]
    );

    // The check compares the exact value, so the published bound has to be exact too
    let valid = Huge {
        id: 9007199254740993,
        unsigned: 9223372036854775809,
    };
    assert!(valid.validate().is_ok());
    assert!(
        Huge {
            id: 9007199254740992,
            ..valid
        }
        .validate()
        .is_err()
    );
    assert!(
        Huge {
            unsigned: 9223372036854775808,
            ..valid
        }
        .validate()
        .is_err()
    );
}

fn is_a_conflict(page: &Page422) -> Result<(), ValidationError> {
    if page.from > page.to {
        return Err(ValidationError::field("from", "must not be after `to`")
            .with_status(volga::http::StatusCode::UNPROCESSABLE_ENTITY));
    }
    Ok(())
}

#[derive(Deserialize, Validate)]
#[validate(schema = "is_a_conflict")]
struct Page422 {
    from: u32,
    to: u32,
}

#[test]
fn it_keeps_the_status_a_merged_rule_asked_for() {
    let err = Page422 { from: 5, to: 1 }.validate().unwrap_err();

    assert_eq!(err.status(), volga::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        volga::error::Error::from(err).status(),
        volga::http::StatusCode::UNPROCESSABLE_ENTITY
    );
}
