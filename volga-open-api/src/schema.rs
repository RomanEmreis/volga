//! Types and utilities for OpenAPI schema.

use std::collections::BTreeMap;
use serde_json::{Map, Value, json};
use serde::{
    de::{
        Visitor,
        DeserializeSeed,
        SeqAccess,
        MapAccess,
        IntoDeserializer,
        Error as DeError
    },
    Deserializer,
    Deserialize,
    Serialize,
};

/// Represents OpenAPI schema.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenApiSchema {
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub(super) schema_ref: Option<String>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(super) schema_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) format: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) properties: Option<BTreeMap<String, OpenApiSchema>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) required: Option<Vec<String>>,

    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) additional_properties: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) items: Option<Box<OpenApiSchema>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) nullable: Option<bool>,
}

impl OpenApiSchema {
    /// Generates schema for object field
    pub fn object() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("object".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for string field
    pub fn string() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("string".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for integer field
    pub fn integer() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("integer".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for number field
    pub fn number() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("number".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for boolean field
    pub fn boolean() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("boolean".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for binary field
    pub fn binary() -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("string".to_string()),
            format: Some("binary".to_string()),
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
        }
    }

    /// Generates schema for multipart form data
    pub fn multipart() -> Self {
        Self::object()
            .with_property("file", OpenApiSchema::binary())
            .with_property("meta", OpenApiSchema::string())
            .with_required(["file"])
    }

    /// Generates schema for an array of items
    pub fn array(items: OpenApiSchema) -> Self {
        Self {
            schema_ref: None,
            schema_type: Some("array".to_string()),
            format: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: Some(Box::new(items)),
            nullable: None,
        }
    }

    /// Generates schema reference
    pub fn reference(name: &str) -> Self {
        Self {
            schema_ref: Some(format!("#/components/schemas/{name}")),
            schema_type: None,
            title: None,
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
            nullable: None,
            format: None
        }
    }

    /// Generates schema from example
    pub fn from_example(example: &Value) -> Self {
        match example {
            Value::Null => OpenApiSchema::object().nullable(),
            Value::Bool(_) => OpenApiSchema::boolean(),
            Value::Number(number) => {
                if number.is_i64() || number.is_u64() {
                    OpenApiSchema::integer()
                } else {
                    OpenApiSchema::number()
                }
            }
            Value::String(_) => OpenApiSchema::string(),
            Value::Array(items) => {
                let item_schema = items
                    .first()
                    .map(OpenApiSchema::from_example)
                    .unwrap_or_else(OpenApiSchema::object);
                OpenApiSchema::array(item_schema)
            }
            Value::Object(map) => {
                let mut schema = OpenApiSchema::object();
                let mut required = Vec::new();
                for (key, value) in map {
                    schema = schema.with_property(key.clone(), OpenApiSchema::from_example(value));
                    required.push(key.clone());
                }
                schema.with_required(required)
            }
        }
    }

    /// Makes schema nullable
    pub fn nullable(mut self) -> Self {
        self.nullable = Some(true);
        self
    }

    /// Sets the title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the property schema
    pub fn with_property(mut self, name: impl Into<String>, schema: OpenApiSchema) -> Self {
        self.properties
            .get_or_insert_with(BTreeMap::new)
            .insert(name.into(), schema);
        self
    }

    /// Sets the format
    pub fn with_format(mut self, fmt: impl Into<String>) -> Self {
        self.format = Some(fmt.into());
        self
    }

    /// Sets the required fields for schema
    pub fn with_required<I, T>(mut self, required: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.required = Some(required.into_iter().map(Into::into).collect());
        self
    }

    /// Returns `true` if this schema is a reference
    pub fn is_ref(&self) -> bool {
        self.schema_ref.is_some()
    }
}

impl Default for OpenApiSchema {
    fn default() -> Self {
        Self::object()
    }
}

#[derive(Debug)]
pub(super) struct ProbeError(String);

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { 
        f.write_str(&self.0)
    }
}
impl std::error::Error for ProbeError {}
impl DeError for ProbeError {
    fn custom<M: std::fmt::Display>(msg: M) -> Self { 
        ProbeError(msg.to_string())
    }
}

pub(super) struct Probe {
    root: Option<(OpenApiSchema, Value)>,
}

impl Probe {
    pub(super) fn new() -> Self { 
        Self { root: None }
    }
    
    pub(super) fn finish(self) -> Option<(OpenApiSchema, Value)> { 
        self.root
    }
}

impl<'de> Deserializer<'de> for &mut Probe {
    type Error = ProbeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        visitor.visit_unit()
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::boolean(), Value::Bool(false)));
        visitor.visit_bool(false)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_i8(0)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_u8(0)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_i16(0)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_u16(0)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_i32(0)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_u32(0)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.root = Some((OpenApiSchema::number(), json!(0.0)));
        visitor.visit_f32(0.0)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_i64(0)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_u64(0)
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_i128(0)
    }

    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::integer(), Value::Number(0.into())));
        visitor.visit_u128(0)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::number(), json!(0.0)));
        visitor.visit_f64(0.0)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::string(), Value::String(String::new())));
        visitor.visit_borrowed_str("")
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::string(), Value::String(String::new())));
        visitor.visit_string(String::new())
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        struct SomeDeserializer<'a>(&'a mut Probe);

        impl<'de, 'a> Deserializer<'de> for SomeDeserializer<'a> {
            type Error = ProbeError;

            fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
            where 
                V: Visitor<'de>
            {
                (&mut *self.0).deserialize_any(visitor)
            }

            serde::forward_to_deserialize_any! {
                bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
                bytes byte_buf unit unit_struct newtype_struct seq tuple tuple_struct
                map struct enum identifier ignored_any option
            }
        }

        let out = visitor.visit_some(SomeDeserializer(self))?;

        if let Some((schema, example)) = self.root.take() {
            self.root = Some((schema.nullable(), example));
        } else {
            self.root = Some((OpenApiSchema::object().nullable(), Value::Null));
        }

        Ok(out)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        visitor.visit_seq(SeqProbeAccess { 
            probe: self, 
            yielded: false
        })
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        self.root = Some((OpenApiSchema::object(), Value::Object(Map::new())));
        visitor.visit_map(EmptyMapAccess)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        let access = StructProbeAccess {
            fields,
            idx: 0,
            parent: self,
            obj_schema: OpenApiSchema::object(),
            example: Map::new(),
            required: Vec::new(),
        };

        visitor.visit_map(access)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>
    {
        Err(
            ProbeError("enums are not supported for automatic schema inference".into())
        )
    }

    serde::forward_to_deserialize_any! {
        char bytes byte_buf unit unit_struct
        newtype_struct tuple tuple_struct identifier ignored_any
    }
}

struct SeqProbeAccess<'a> {
    probe: &'a mut Probe,
    yielded: bool,
}

impl<'de, 'a> SeqAccess<'de> for SeqProbeAccess<'a> {
    type Error = ProbeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>
    {
        if self.yielded {
            if let Some((item_schema, item_example)) = self.probe.root.take() {
                self.probe.root = Some((
                    OpenApiSchema::array(item_schema),
                    Value::Array(vec![item_example]),
                ));
            } else {
                self.probe.root = Some((
                    OpenApiSchema::array(OpenApiSchema::object()),
                    Value::Array(vec![]),
                ));
            }
            return Ok(None);
        }

        self.yielded = true;
        let v = seed.deserialize(&mut *self.probe)?;
        Ok(Some(v))
    }
}

struct StructProbeAccess<'a> {
    fields: &'static [&'static str],
    idx: usize,
    parent: &'a mut Probe,

    obj_schema: OpenApiSchema,
    example: Map<String, Value>,
    required: Vec<String>,
}

impl<'de, 'a> MapAccess<'de> for StructProbeAccess<'a> {
    type Error = ProbeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>
    {
        if self.idx >= self.fields.len() {
            let schema = self.obj_schema.clone().with_required(self.required.clone());
            let ex = Value::Object(std::mem::take(&mut self.example));
            self.parent.root = Some((schema, ex));
            return Ok(None);
        }

        let key = self.fields[self.idx];
        let key_de = key.into_deserializer();
        let k = seed.deserialize(key_de)?;
        Ok(Some(k))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>
    {
        let field = self.fields[self.idx];
        self.idx += 1;

        let v = seed.deserialize(&mut *self.parent)?;

        if let Some((field_schema, field_example)) = self.parent.root.take() {
            let is_required = field_schema.nullable != Some(true) && field_example != Value::Null;
            self.obj_schema = self
                .obj_schema
                .clone()
                .with_property(field.to_string(), field_schema);

            self.example.insert(field.to_string(), field_example);
            if is_required {
                self.required.push(field.to_string());
            }
        } else {
            self.obj_schema = self
                .obj_schema
                .clone()
                .with_property(field.to_string(), OpenApiSchema::object());

            self.example.insert(field.to_string(), Value::Null);
        }

        Ok(v)
    }
}

struct EmptyMapAccess;
impl<'de> MapAccess<'de> for EmptyMapAccess {
    type Error = ProbeError;

    fn next_key_seed<K>(&mut self, _seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>
    {
        Ok(None)
    }

    fn next_value_seed<V>(&mut self, _seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>
    {
        Err(ProbeError("no values".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_from_example_object_keeps_properties() {
        let schema = OpenApiSchema::from_example(&serde_json::json!({
            "name": "Alice",
            "age": 42
        }));

        let props = schema.properties.expect("schema properties");
        assert_eq!(
            props.get("name").expect("name").schema_type.as_deref(),
            Some("string")
        );
        assert_eq!(
            props.get("age").expect("age").schema_type.as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn schema_from_example_handles_null_and_arrays() {
        let null_schema = OpenApiSchema::from_example(&Value::Null);
        assert_eq!(null_schema.schema_type.as_deref(), Some("object"));
        assert_eq!(null_schema.nullable, Some(true));

        let array_schema = OpenApiSchema::from_example(&serde_json::json!([1, 2, 3]));
        assert_eq!(array_schema.schema_type.as_deref(), Some("array"));
        assert_eq!(
            array_schema
                .items
                .expect("array items")
                .schema_type
                .as_deref(),
            Some("integer")
        );
    }

    #[test]
    fn multipart_schema_contains_expected_fields() {
        let schema = OpenApiSchema::multipart();
        let props = schema.properties.expect("properties");

        assert!(props.contains_key("file"));
        assert!(props.contains_key("meta"));
        assert_eq!(schema.required.expect("required"), vec!["file"]);
    }

    #[test]
    fn schema_reference_marks_ref_and_path() {
        let schema = OpenApiSchema::reference("Payload");
        assert!(schema.is_ref());
        assert_eq!(
            schema.schema_ref.as_deref(),
            Some("#/components/schemas/Payload")
        );
    }

    #[test]
    fn number_and_boolean_builders_set_expected_types() {
        assert_eq!(
            OpenApiSchema::number().schema_type.as_deref(),
            Some("number")
        );
        assert_eq!(
            OpenApiSchema::boolean().schema_type.as_deref(),
            Some("boolean")
        );
    }

    #[test]
    fn with_format_sets_schema_format() {
        let schema = OpenApiSchema::string().with_format("uuid");
        assert_eq!(schema.format.as_deref(), Some("uuid"));
    }

    #[test]
    fn default_schema_is_object() {
        let schema = OpenApiSchema::default();
        assert_eq!(schema.schema_type.as_deref(), Some("object"));
    }

    #[test]
    fn probe_does_not_mark_option_fields_as_required() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Input {
            required_name: String,
            optional_age: Option<()>,
        }

        let mut probe = Probe::new();
        let _ = Input::deserialize(&mut probe);
        let (schema, _) = probe.finish().expect("schema should be produced");

        let required = schema.required.expect("required list");
        assert!(required.contains(&"required_name".to_string()));
        assert!(!required.contains(&"optional_age".to_string()));
    }

    #[test]
    fn probe_supports_i32_fields_for_schema_inference() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct NumericInput {
            value: i32,
        }

        let mut probe = Probe::new();
        let _ = NumericInput::deserialize(&mut probe);
        let (schema, _) = probe.finish().expect("schema should be produced");

        let props = schema.properties.expect("properties");
        assert_eq!(props["value"].schema_type.as_deref(), Some("integer"));
    }

    #[test]
    fn probe_supports_u32_fields_for_schema_inference() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct NumericInput {
            value: u32,
        }

        let mut probe = Probe::new();
        let _ = NumericInput::deserialize(&mut probe);
        let (schema, _) = probe.finish().expect("schema should be produced");

        let props = schema.properties.expect("properties");
        assert_eq!(props["value"].schema_type.as_deref(), Some("integer"));
    }

    #[test]
    fn probe_supports_f32_fields_for_schema_inference() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct NumericInput {
            value: f32,
        }

        let mut probe = Probe::new();
        let _ = NumericInput::deserialize(&mut probe);
        let (schema, _) = probe.finish().expect("schema should be produced");

        let props = schema.properties.expect("properties");
        assert_eq!(props["value"].schema_type.as_deref(), Some("number"));
    }
}