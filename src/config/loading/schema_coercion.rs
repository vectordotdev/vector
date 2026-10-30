//! Coercion for Vector's generated configuration schemas.
//!
//! This is a preparation pass, not a general JSON Schema validator. Serde remains
//! authoritative for final component validation.
//! The loader invokes this pass after environment-variable and secret substitution.

use std::collections::HashSet;

use serde_json::{Number, Value};
use snafu::{OptionExt, Snafu};
use vector_config::constants::{METADATA, SERDE_ALIASES, SERDE_STRING_ONLY, SERDE_VARIANT_ALIASES};

use super::{ignored_fields, merge::ConfigNode};

const NULL_JSON_TYPE: &str = "null";
const BOOL_JSON_TYPE: &str = "boolean";
const NUMBER_JSON_TYPE: &str = "number";
const STRING_JSON_TYPE: &str = "string";
const ARRAY_JSON_TYPE: &str = "array";
const OBJECT_JSON_TYPE: &str = "object";
const DEFINITION_PREFIX: &str = "#/definitions/";
const COMPONENT_MAPS: [&str; 5] = [
    "sources",
    "transforms",
    "sinks",
    "enrichment_tables",
    "secret",
];
const PROVIDER: &str = "provider";

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("{message}"))]
    Merge { message: String },

    #[snafu(display("Expected boolean at '{path}', found '{actual}'"))]
    ExpectedBool { path: String, actual: &'static str },

    #[snafu(display("Expected integer at '{path}', found '{actual}'"))]
    ExpectedInteger { path: String, actual: &'static str },

    #[snafu(display("Expected number at '{path}', found '{actual}'"))]
    ExpectedNumber { path: String, actual: &'static str },

    #[snafu(display("Expected string at '{path}', found '{actual}'"))]
    ExpectedString { path: String, actual: &'static str },

    #[snafu(display("Expected null at '{path}', found '{actual}'"))]
    ExpectedNull { path: String, actual: &'static str },

    #[snafu(display("Expected array at '{path}', found '{actual}'"))]
    ExpectedArray { path: String, actual: &'static str },

    #[snafu(display("Expected object at '{path}', found '{actual}'"))]
    ExpectedObject { path: String, actual: &'static str },

    #[snafu(display("Unexpected property '{key}' at '{path}'"))]
    UnexpectedProperty { path: String, key: String },

    #[snafu(display("Unexpected extra array element at '{path}[{index}]'"))]
    UnexpectedArrayElement { path: String, index: usize },

    #[snafu(display("Schema reference '{reference}' not found at path '{path}'"))]
    SchemaReferenceNotFound { path: String, reference: String },

    #[snafu(display("Unsupported schema reference '{reference}' at path '{path}'"))]
    UnsupportedSchemaReference { path: String, reference: String },

    #[snafu(display("Unexpected property '{path}'"))]
    DisallowedProperty { path: String },

    #[snafu(display("Value at '{path}' is not one of the allowed enum options"))]
    InvalidEnumValue { path: String },

    #[snafu(display("Value at '{path}' does not match required constant '{expected}'"))]
    InvalidConst { path: String, expected: String },

    #[snafu(display("Coercion failed at '{path}': {message}"))]
    Coerce { path: String, message: String },
}

use vector_common::pastey::paste;

macro_rules! fail_expected {
    ($variant:ident, $val:expr, $path_components:expr) => {
        paste! {
            [<Expected $variant Snafu>] {
                path: $path_components.join("."),
                actual: get_json_type($val),
            }
            .fail()
        }
    };
}

/// Coerces parsed values using a generated Vector schema.
///
/// Owns traversal state so callers do not have to manage definition lookup or
/// error paths. The schema is borrowed and may be reused across calls.
pub struct ValueCoercer<'a> {
    schema: &'a Value,
    tree: Option<ConfigNode>,
    definitions: Option<&'a Value>,
    component_schemas: Vec<&'a Value>,
    path: Vec<String>,
}

impl<'a> ValueCoercer<'a> {
    /// Creates a coercer for a root schema and its local definitions.
    #[must_use]
    pub fn new(schema: &'a Value) -> Self {
        let mut coercer = Self {
            schema,
            tree: None,
            definitions: schema.get("definitions"),
            component_schemas: Vec::new(),
            path: Vec::new(),
        };
        coercer.component_schemas = coercer
            .object_schemas(schema)
            .filter_map(|root| root.get("properties"))
            .flat_map(|properties| COMPONENT_MAPS.iter().filter_map(|key| properties.get(key)))
            .flat_map(|map| coercer.object_schemas(map))
            .filter_map(|map| map.get("additionalProperties"))
            .filter(|schema| schema.is_object())
            .chain(
                coercer
                    .object_schemas(schema)
                    .filter_map(|root| root.get("properties")?.get(PROVIDER)),
            )
            .collect();
        coercer
    }

    /// Coerces a value in place. On error, some fields may already be coerced.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if `value` does not match the schema, for example when a value has
    /// the wrong type, an object has a disallowed property, an array has extra elements, or a
    /// value is not an allowed enum option or constant. Also returns an error if the schema
    /// contains a missing or unsupported `$ref`.
    pub fn coerce(&mut self, value: &mut Value) -> Result<(), Error> {
        self.path.clear();
        self.coerce_value(value, self.schema)
    }

    /// Resolves a merge tree in its final configuration coordinates. Histories are
    /// checked at the corresponding field, using that field's schema only.
    pub(super) fn coerce_tree(&mut self, tree: &ConfigNode) -> Result<Value, Error> {
        self.path.clear();
        self.tree = Some(tree.clone());
        let mut value = tree.latest();
        let result = self.coerce_field(&mut value, self.schema);
        let tree = self
            .tree
            .take()
            .expect("merge tree is retained during coercion");
        result?;
        // Fields without a schema still follow the raw merge rules. Typed fields
        // have already converted their scalar histories in place.
        tree.check_raw_merges().map_err(|errors| Error::Merge {
            message: errors.join("\n"),
        })?;
        Ok(value)
    }

    fn coerce_field(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        self.coerce_value(value, schema)?;
        if let Some(node) = self
            .tree
            .as_mut()
            .and_then(|tree| tree.at_path_mut(&self.path))
            && matches!(node, ConfigNode::Scalar(values) if values.len() > 1 && values.iter().any(Value::is_string))
        {
            let mut scalar_coercer = Self::new(self.schema);
            scalar_coercer.path.clone_from(&self.path);
            node.resolve_scalar_history(value, &self.path, |previous| {
                scalar_coercer
                    .coerce_value(previous, schema)
                    .map_err(|error| error.to_string())
            })
            .map_err(|message| Error::Merge { message })?;
        }
        Ok(())
    }

    fn definition(&self, reference: &str) -> Option<&'a Value> {
        let key = reference.strip_prefix(DEFINITION_PREFIX)?;
        self.definitions?.as_object()?.get(key)
    }

    // The JSON-value counterpart of RootSchema::root_map_value_schema: follow
    // flattened fields and local references, not nested properties or unions.
    fn object_schemas(&self, schema: &'a Value) -> impl Iterator<Item = &'a Value> {
        let mut pending = vec![schema];
        let mut seen = HashSet::new();
        std::iter::from_fn(move || {
            let schema = pending.pop()?;
            if let Some(all_of) = schema.get("allOf").and_then(Value::as_array) {
                pending.extend(all_of.iter().rev());
            }
            if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
                && seen.insert(reference)
                && let Some(target) = self.definition(reference)
            {
                pending.push(target);
            }
            Some(schema)
        })
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(clippy::match_same_arms, reason = "Preserve branch-specific context")]
    fn coerce_value(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        if self.is_unknown_component(value, schema) {
            return Ok(());
        }

        match schema {
            Value::Bool(true) => Ok(()),
            Value::Bool(false) => DisallowedPropertySnafu {
                path: self.path.join("."),
            }
            .fail(),
            Value::Object(_) => self.coerce_object_schema(value, schema),
            // Preserve the existing no-op behavior for non-schema values.
            _ => Ok(()),
        }
    }

    fn is_unknown_component(&self, value: &Value, schema: &Value) -> bool {
        // Unknown component kinds are diagnosed by serde. Restrict this escape
        // hatch to the outer component schema, never an arbitrary union branch.
        let at_component = (self.path.len() == 2
            && COMPONENT_MAPS.contains(&self.path[0].as_str()))
            || (self.path.len() == 1 && self.path[0] == PROVIDER);
        at_component
            && self
                .component_schemas
                .iter()
                .any(|outer| std::ptr::eq(*outer, schema))
            && value
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| !self.schema_contains_type_discriminant(schema, kind))
    }

    fn coerce_object_schema(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        if let Some(object) = schema.as_object()
            && let Some(tag) = object
                .get(METADATA)
                .and_then(|metadata| metadata.get(ignored_fields::SERDE_OPTIONAL_TAG))
                .and_then(Value::as_str)
        {
            // Use the same accepted-input adjustment as ignored-field diagnostics.
            let mut object = object.clone();
            if let Some(metadata) = object.get_mut(METADATA).and_then(Value::as_object_mut) {
                metadata.remove(ignored_fields::SERDE_OPTIONAL_TAG);
            }
            ignored_fields::make_tag_optional(&mut object, tag);
            return self.coerce_object_schema(value, &Value::Object(object));
        }

        // Some custom string deserializers (for example ASCII characters) reject
        // native numbers. Do not turn those inputs into valid-looking strings.
        if schema
            .get(METADATA)
            .and_then(|metadata| metadata.get(SERDE_STRING_ONLY))
            .and_then(Value::as_bool)
            == Some(true)
            && !value.is_string()
        {
            return fail_expected!(String, value, self.path);
        }

        // Keywords can coexist. Apply every constraint in this order.
        self.apply_reference(value, schema)?;
        self.apply_all_of(value, schema)?;
        self.apply_one_of(value, schema)?;
        self.apply_any_of(value, schema)?;
        self.apply_enum(value, schema)?;
        self.apply_const(value, schema)?;
        self.coerce_schema_type(value, schema)?;

        // Evaluate negation against the coerced value.
        self.apply_not(value, schema)
    }

    fn coerce_schema_type(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        match schema.get("type") {
            Some(Value::String(expected)) => self.coerce_type(value, expected, schema),
            Some(Value::Array(types)) => {
                let allowed: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
                self.coerce_multiple_types(value, &allowed, schema)
            }
            Some(_) => Ok(()),
            None => match value {
                Value::Object(_) => self.coerce_object(value, schema),
                Value::Array(_) => self.coerce_array(value, schema),
                _ => Ok(()),
            },
        }
    }

    // Vector emits these simple negations for absent tags, nonzero numbers,
    // and mutually exclusive optional fields. Do not coerce to test a negation.
    fn apply_not(&self, value: &Value, schema: &Value) -> Result<(), Error> {
        let Some(negated) = schema.get("not") else {
            return Ok(());
        };
        let matches = if let Some(boolean) = negated.as_bool() {
            boolean
        } else if let Some(object) = negated.as_object() {
            if object.is_empty() {
                true
            } else if object.len() == 1 && object.contains_key("required") {
                let required = object["required"].as_array().ok_or_else(|| Error::Coerce {
                    path: self.path.join("."),
                    message: "Invalid negated required predicate".into(),
                })?;
                value.as_object().is_none_or(|value| {
                    required
                        .iter()
                        .all(|key| key.as_str().is_some_and(|key| value.contains_key(key)))
                })
            } else if object.len() == 1 && object.contains_key("const") {
                value == &object["const"]
            } else if object.len() == 1 && object.contains_key("type") {
                self.schema_matches_value_type(negated, value)
            } else {
                return CoerceSnafu {
                    path: self.path.join("."),
                    message: "Unsupported negated schema".to_owned(),
                }
                .fail();
            }
        } else {
            return CoerceSnafu {
                path: self.path.join("."),
                message: "Invalid negated schema".to_owned(),
            }
            .fail();
        };
        if matches {
            CoerceSnafu {
                path: self.path.join("."),
                message: "Value matches a forbidden schema".to_owned(),
            }
            .fail()
        } else {
            Ok(())
        }
    }

    fn field_aliases(schema: &Value) -> impl Iterator<Item = &str> {
        schema
            .get(METADATA)
            .and_then(|m| m.get(SERDE_ALIASES))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }

    fn matches_variant_alias(schema: &Value, value: &str) -> bool {
        schema
            .get(METADATA)
            .and_then(|metadata| metadata.get(SERDE_VARIANT_ALIASES))
            .and_then(Value::as_array)
            .is_some_and(|aliases| aliases.iter().any(|alias| alias.as_str() == Some(value)))
    }

    fn apply_reference(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) else {
            return Ok(());
        };

        if ref_str.starts_with(DEFINITION_PREFIX) {
            let def_schema = self
                .definition(ref_str)
                .context(SchemaReferenceNotFoundSnafu {
                    path: self.path.join("."),
                    reference: ref_str.to_string(),
                })?;

            self.coerce_value(value, def_schema)?;
            Ok(())
        } else {
            UnsupportedSchemaReferenceSnafu {
                path: self.path.join("."),
                reference: ref_str.to_string(),
            }
            .fail()
        }
    }

    fn apply_all_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(all_of) = schema.get("allOf").and_then(|v| v.as_array()) else {
            return Ok(());
        };

        for sub_schema in all_of {
            self.coerce_value(value, sub_schema)?;
        }

        Ok(())
    }

    /// Select a `oneOf` variant, treating untagged schemas like `anyOf`.
    /// Final validation, including ambiguous variants, is left to serde.
    fn apply_one_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(variants) = schema.get("oneOf").and_then(|v| v.as_array()) else {
            return Ok(());
        };

        // If this oneOf is marked untagged, treat it like anyOf
        let is_untagged = schema
            .get("_metadata")
            .and_then(|m| m.get("docs::enum_tagging"))
            .and_then(Value::as_str)
            == Some("untagged");

        if is_untagged {
            self.coerce_any_of(value, variants)
        } else {
            self.coerce_one_of(value, variants)
        }
    }

    /// Try an `anyOf` variant, leaving final validation to serde.
    fn apply_any_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(variants) = schema.get("anyOf").and_then(|v| v.as_array()) else {
            return Ok(());
        };
        self.coerce_any_of(value, variants)
    }

    fn apply_enum(&self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) else {
            return Ok(());
        };

        if Self::coerce_allowed_value(value, schema, enum_vals) {
            Ok(())
        } else {
            InvalidEnumValueSnafu {
                path: self.path.join("."),
            }
            .fail()
        }
    }

    fn apply_const(&self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(const_val) = schema.get("const") else {
            return Ok(());
        };

        if Self::coerce_allowed_value(value, schema, std::slice::from_ref(const_val)) {
            Ok(())
        } else {
            InvalidConstSnafu {
                path: self.path.join("."),
                expected: const_val.to_string(),
            }
            .fail()
        }
    }

    /// Shared scalar conversions for `enum` and `const`, preserving exact matches first.
    fn coerce_allowed_value(value: &mut Value, schema: &Value, allowed: &[Value]) -> bool {
        // Exact match
        if allowed.iter().any(|opt| value == opt)
            || value
                .as_str()
                .is_some_and(|value| Self::matches_variant_alias(schema, value))
        {
            return true;
        }

        // Try coercions from string
        if let Value::String(s) = value {
            let s_trimmed = s.trim();

            // String → Bool
            if let Some(b) = parse_bool(s_trimmed)
                && allowed.iter().any(|opt| opt.as_bool() == Some(b))
            {
                *value = Value::Bool(b);
                return true;
            }

            // String → Number
            if let Some(n) = parse_number(s_trimmed)
                && allowed.iter().any(|opt| opt.as_number() == Some(&n))
            {
                *value = Value::Number(n);
                return true;
            }

            // String → Null
            if is_null_literal(s_trimmed) && allowed.iter().any(serde_json::Value::is_null) {
                *value = Value::Null;
                return true;
            }
        }

        // Number → String
        if let Value::Number(n) = value {
            let val_str = n.to_string();
            if allowed
                .iter()
                .any(|opt| opt.as_str() == Some(val_str.as_str()))
            {
                *value = Value::String(val_str);
                return true;
            }
        }

        // Bool → String
        if let Value::Bool(b) = value {
            let val_str = b.to_string();
            if let Some(matched) = allowed
                .iter()
                .filter_map(Value::as_str)
                .find(|allowed| allowed.eq_ignore_ascii_case(&val_str))
            {
                *value = Value::String(matched.to_owned());
                return true;
            }
        }

        false
    }

    /// Ensure `value` matches one of the allowed types in `allowed`.
    /// If needed, convert the value to one of those types.
    fn coerce_multiple_types(
        &mut self,
        value: &mut Value,
        allowed_types: &[&str],
        schema: &Value,
    ) -> Result<(), Error> {
        // Preserve an explicitly allowed null before trying conversions such as
        // wrapping a scalar into an array. Nested unions may defer to serde.
        if value.is_null() && allowed_types.contains(&NULL_JSON_TYPE) {
            return Ok(());
        }

        for allowed_type in allowed_types {
            let mut new_value = value.clone();

            let result = self.coerce_type(&mut new_value, allowed_type, schema);

            if result.is_ok() {
                *value = new_value;
                return Ok(());
            }
        }

        CoerceSnafu {
            path: self.path.join("."),
            message: format!(
                "Expected {} but found {}",
                allowed_types.join(" or "),
                get_json_type(value)
            ),
        }
        .fail()
    }

    /// Ensure `value` matches the expected single `expected_type`. Converts the value if possible.
    fn coerce_type(
        &mut self,
        value: &mut Value,
        expected_type: &str,
        schema: &Value,
    ) -> Result<(), Error> {
        match expected_type {
            "null" => self.coerce_null(value),
            "boolean" => self.coerce_bool(value),
            "integer" => self.coerce_integer(value),
            "number" => self.coerce_number(value),
            "string" => self.coerce_string(value),
            "object" => self.coerce_object(value, schema),
            "array" => {
                // Any type can be wrapped to an array. This is needed because  we have deserialization logic that accepts
                // e.g. a single string and converts it to an array, set or some other collection.
                if !value.is_array() {
                    *value = Value::Array(vec![value.clone()]);
                }
                self.coerce_array(value, schema)
            }
            _ => Ok(()), // silently skip unknown types
        }
    }

    /// Coerce all entries of an object value according to the schema's properties and additionalProperties.
    fn coerce_object(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let actual = get_json_type(value);
        let obj = value.as_object_mut().context(ExpectedObjectSnafu {
            path: self.path.join("."),
            actual,
        })?;

        let properties = schema.get("properties").and_then(|p| p.as_object());
        let additional_properties = schema.get("additionalProperties");
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                let present = obj.contains_key(key)
                    || properties.and_then(|p| p.get(key)).is_some_and(|field| {
                        Self::field_aliases(field).any(|alias| obj.contains_key(alias))
                    });
                if !present {
                    return CoerceSnafu {
                        path: self.path.join("."),
                        message: format!("Missing required property '{key}'"),
                    }
                    .fail();
                }
            }
        }

        for (key, field) in obj.iter_mut() {
            let field_schema = properties
                .and_then(|props| {
                    props.get(key).or_else(|| {
                        props
                            .values()
                            .find(|field| Self::field_aliases(field).any(|alias| alias == key))
                    })
                })
                // `true` allows the property without imposing a schema.
                .or_else(|| additional_properties.filter(|schema| **schema != Value::Bool(true)));
            let Some(field_schema) = field_schema else {
                // Ignored-field diagnostics belong to the configuration loader.
                continue;
            };
            if field_schema == &Value::Bool(false) {
                return UnexpectedPropertySnafu {
                    path: self.path.join("."),
                    key: key.to_owned(),
                }
                .fail();
            }

            let initial_len = self.path.len();
            self.path.push(key.to_owned());
            let result = self.coerce_field(field, field_schema);
            self.path.truncate(initial_len);
            result?;
        }

        Ok(())
    }

    /// Coerce all elements of an array value according to the schema's items definition.
    fn coerce_array(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let actual = get_json_type(value);
        let arr = value.as_array_mut().context(ExpectedArraySnafu {
            path: self.path.join("."),
            actual,
        })?;

        let items = schema.get("items");
        let additional_items = schema.get("additionalItems");

        let Some(items_schema) = items else {
            // No "items" schema means all values are accepted as-is
            return Ok(());
        };

        match items_schema {
            Value::Array(tuple_schemas) => {
                for (idx, item_val) in arr.iter_mut().enumerate() {
                    let initial_len = self.path.len();
                    self.path.push(idx.to_string());

                    let schema = tuple_schemas.get(idx).or(additional_items);

                    if let Some(item_schema) = schema {
                        if item_schema == &Value::Bool(false) {
                            self.path.truncate(initial_len);
                            return UnexpectedArrayElementSnafu {
                                path: self.path.join("."),
                                index: idx,
                            }
                            .fail();
                        }

                        let result = self.coerce_field(item_val, item_schema);
                        self.path.truncate(initial_len);
                        result?;
                    } else {
                        self.path.truncate(initial_len);
                    }
                }
            }

            item_schema => {
                for (idx, item_val) in arr.iter_mut().enumerate() {
                    let initial_len = self.path.len();
                    self.path.push(idx.to_string());
                    let result = self.coerce_field(item_val, item_schema);
                    self.path.truncate(initial_len);
                    result?;
                }
            }
        }

        Ok(())
    }

    /// Prefer shape-preserving variants, leaving final validation to serde.
    fn coerce_one_of(&mut self, value: &mut Value, schemas: &[Value]) -> Result<(), Error> {
        let initial_len = self.path.len();
        match self.coerce_any_of(value, schemas) {
            Ok(()) => return Ok(()),
            Err(error @ Error::Merge { .. }) => return Err(error),
            Err(_) => {}
        }

        // No variant succeeded. If the value carries a `type` discriminant that matches a known
        // variant, re-run coercion strictly against that variant so callers get a path-aware
        // coercion error (e.g. "expected integer at sources.my_source.count") rather than a
        // silent pass-through. The configuration loader handles ignored-field diagnostics.
        if let Some(type_val) = value
            .as_object()
            .and_then(|o| o.get("type"))
            .and_then(|t| t.as_str())
        {
            for schema in schemas {
                if self.schema_matches_type_discriminant(schema, type_val) {
                    let mut candidate = value.clone();
                    let result = self.coerce_value(&mut candidate, schema);
                    self.path.truncate(initial_len);
                    return result.map(|()| {
                        *value = candidate;
                    });
                }
            }
        }

        CoerceSnafu {
            path: self.path.join("."),
            message: "No matching oneOf variant".to_owned(),
        }
        .fail()
    }

    /// Returns true if `schema` (after resolving any `$ref` and walking `allOf`) has a
    /// `properties.type.const` equal to `expected`.
    fn schema_matches_type_discriminant(&self, schema: &Value, expected: &str) -> bool {
        let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
            match self.definition(ref_str) {
                Some(def) => def,
                None => return false,
            }
        } else {
            schema
        };

        if resolved
            .get("properties")
            .and_then(|p| p.get("type"))
            .is_some_and(|tag| {
                tag.get("const").and_then(Value::as_str) == Some(expected)
                    || Self::matches_variant_alias(tag, expected)
            })
        {
            return true;
        }

        // Walk allOf in case the discriminant is embedded there.
        if let Some(all_of) = resolved.get("allOf").and_then(|v| v.as_array())
            && all_of
                .iter()
                .any(|sub| self.schema_matches_type_discriminant(sub, expected))
        {
            return true;
        }

        false
    }

    /// Returns true if `schema`, after resolving `$ref`, `allOf`, and unions, contains
    /// any variant that claims `expected` as its `type` discriminant. Components whose
    /// type is not compiled in pass through coercion for Serde to diagnose.
    fn schema_contains_type_discriminant(&self, schema: &Value, expected: &str) -> bool {
        let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
            match self.definition(ref_str) {
                Some(def) => def,
                None => return false,
            }
        } else {
            schema
        };

        if self.schema_matches_type_discriminant(resolved, expected) {
            return true;
        }

        ["allOf", "oneOf", "anyOf"].iter().any(|keyword| {
            resolved
                .get(keyword)
                .and_then(Value::as_array)
                .is_some_and(|schemas| {
                    schemas
                        .iter()
                        .any(|schema| self.schema_contains_type_discriminant(schema, expected))
                })
        })
    }

    /// Prefer a structurally compatible variant, failing if none can be coerced.
    fn coerce_any_of(&mut self, value: &mut Value, schemas: &[Value]) -> Result<(), Error> {
        let initial_len = self.path.len();

        // Untagged unions are deserialized according to the input's shape. Preserve that behavior by
        // trying structurally compatible variants before variants that require coercion. For example,
        // an object in a `string | object` union must not be stringified before the object variant is
        // considered.
        for prefer_matching_type in [true, false] {
            for schema in schemas {
                if self.schema_matches_value_type(schema, value) != prefer_matching_type {
                    continue;
                }

                self.path.truncate(initial_len);
                let mut candidate = value.clone();
                // Choose the variant from the final definitions. Historical values
                // must not cause a different branch to win or disappear in a fallback.
                let tree = self.tree.take();
                let result = self.coerce_value(&mut candidate, schema);
                self.tree = tree;
                if result.is_ok() {
                    self.path.truncate(initial_len);
                    if self.tree.is_some() {
                        self.coerce_value(value, schema)?;
                    } else {
                        *value = candidate;
                    }
                    return Ok(());
                }
            }
        }

        self.path.truncate(initial_len);
        CoerceSnafu {
            path: self.path.join("."),
            message: "No matching anyOf variant".to_owned(),
        }
        .fail()
    }

    fn schema_matches_value_type(&self, schema: &Value, value: &Value) -> bool {
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
            && let Some(schema) = self.definition(reference)
        {
            return self.schema_matches_value_type(schema, value);
        }

        if let Some(schema_type) = schema.get("type") {
            return match schema_type {
                Value::String(schema_type) => value_matches_type(value, schema_type),
                Value::Array(schema_types) => schema_types
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|schema_type| value_matches_type(value, schema_type)),
                _ => false,
            };
        }

        // Generated unit variants can constrain values without an explicit `type`.
        // Rank their shape here; coercion still checks the actual allowed value.
        if let Some(constant) = schema.get("const") {
            return get_json_type(constant) == get_json_type(value);
        }
        if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
            return allowed
                .iter()
                .any(|allowed| get_json_type(allowed) == get_json_type(value));
        }

        ["allOf", "anyOf", "oneOf"].iter().any(|keyword| {
            schema
                .get(keyword)
                .and_then(Value::as_array)
                .is_some_and(|schemas| {
                    schemas
                        .iter()
                        .any(|schema| self.schema_matches_value_type(schema, value))
                })
        })
    }

    fn coerce_bool(&mut self, value: &mut Value) -> Result<(), Error> {
        match value {
            Value::Bool(_) => Ok(()),
            Value::String(s) => match parse_bool(s) {
                Some(b) => {
                    *value = Value::Bool(b);
                    Ok(())
                }
                _ => fail_expected!(Bool, value, self.path),
            },
            _ => fail_expected!(Bool, value, self.path),
        }
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::cast_possible_truncation,
        reason = "Bounds and overflow audit deferred"
    )]
    #[allow(
        clippy::cast_precision_loss,
        reason = "Numeric precision audit deferred"
    )]
    #[allow(clippy::cast_sign_loss, reason = "Input bounds audit deferred")]
    fn coerce_integer(&mut self, value: &mut Value) -> Result<(), Error> {
        if let Value::Number(n) = value {
            if n.is_i64() || n.is_u64() {
                return Ok(());
            }

            if let Some(f) = n.as_f64()
                && f.fract() == 0.0
            {
                if ((i64::MIN as f64)..0.0).contains(&f) {
                    *value = Value::Number(Number::from(f as i64));
                    return Ok(());
                }
                // u64::MAX rounds up to 2^64 as f64. Exclude that boundary
                // before casting so out-of-range floats cannot saturate.
                if (0.0..(u64::MAX as f64)).contains(&f) {
                    *value = Value::Number(Number::from(f as u64));
                    return Ok(());
                }
            }
        } else if let Value::String(s) = value
            && let Some(n) = parse_integer(s)
        {
            *value = Value::Number(n);
            return Ok(());
        }

        fail_expected!(Integer, value, self.path)
    }

    fn coerce_number(&mut self, value: &mut Value) -> Result<(), Error> {
        if let Value::Number(_) = value {
            return Ok(());
        }

        if let Value::String(s) = value
            && let Some(n) = parse_number(s)
        {
            *value = Value::Number(n);
            return Ok(());
        }

        fail_expected!(Number, value, self.path)
    }

    fn coerce_null(&mut self, value: &mut Value) -> Result<(), Error> {
        match value {
            Value::Null => Ok(()),
            Value::String(s) if is_null_literal(s) => {
                *value = Value::Null;
                Ok(())
            }
            _ => fail_expected!(Null, value, self.path),
        }
    }

    fn coerce_string(&mut self, value: &mut Value) -> Result<(), Error> {
        match value {
            Value::String(_) => Ok(()),
            Value::Bool(_) | Value::Number(_) => {
                *value = Value::String(value.to_string());
                Ok(())
            }
            _ => fail_expected!(String, value, self.path),
        }
    }
}

fn value_matches_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_number()
            .is_some_and(|number| number.is_i64() || number.is_u64()),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }
}

fn is_null_literal(input: &str) -> bool {
    // YAML also represents null with a tilde or an empty scalar.
    let input = input.trim();
    matches!(input, "" | "~") || input.eq_ignore_ascii_case("null")
}

fn parse_bool(input: &str) -> Option<bool> {
    // Preserve YAML's boolean spellings when substitution happens after parsing.
    match input.trim() {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn remove_digit_separators(input: &str, radix: u32) -> Option<String> {
    // TOML permits underscores only between digits, including exponent digits.
    let is_digit = |byte: u8| char::from(byte).is_digit(radix);
    if input.starts_with('_')
        || input.ends_with('_')
        || input
            .as_bytes()
            .windows(3)
            .any(|bytes| bytes[1] == b'_' && (!is_digit(bytes[0]) || !is_digit(bytes[2])))
    {
        return None;
    }
    Some(input.replace('_', ""))
}

fn parse_integer(input: &str) -> Option<Number> {
    let input = input.trim();
    let (sign, digits) = if let Some(digits) = input.strip_prefix('-') {
        ("-", digits)
    } else {
        ("", input.strip_prefix('+').unwrap_or(input))
    };
    let (digits, radix) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| digits.strip_prefix(prefix).map(|digits| (digits, radix)))
        .unwrap_or((digits, 10));
    if digits.starts_with(['+', '-']) {
        return None;
    }
    // Keep the sign for checked parsing, including i64::MIN, without accepting
    // arrays, maps, tags, or other configuration syntax from substituted values.
    let digits = remove_digit_separators(digits, radix)?;
    let number = format!("{sign}{digits}");
    i64::from_str_radix(&number, radix)
        .map(Number::from)
        .or_else(|_| u64::from_str_radix(&number, radix).map(Number::from))
        .ok()
}

fn parse_number(input: &str) -> Option<Number> {
    if let Some(num) = parse_integer(input) {
        return Some(num);
    }
    if let Ok(f) = remove_digit_separators(input.trim(), 10)?.parse::<f64>() {
        return Number::from_f64(f);
    }
    None
}

const fn get_json_type(val: &Value) -> &'static str {
    match val {
        Value::Null => NULL_JSON_TYPE,
        Value::Bool(_) => BOOL_JSON_TYPE,
        Value::Number(_) => NUMBER_JSON_TYPE,
        Value::String(_) => STRING_JSON_TYPE,
        Value::Array(_) => ARRAY_JSON_TYPE,
        Value::Object(_) => OBJECT_JSON_TYPE,
    }
}

#[cfg(test)]
mod tests;
