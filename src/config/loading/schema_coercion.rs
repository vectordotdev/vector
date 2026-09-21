//! Coercion for Vector's generated configuration schemas.
//!
//! This is a preparation pass, not a general JSON Schema validator. Serde remains
//! authoritative for required fields, aliases, and component validation.
//! The loader does not invoke this pass yet.

use serde_json::{Number, Value};
use snafu::{OptionExt, Snafu};
use std::collections::HashSet;

const NULL_JSON_TYPE: &str = "null";
const BOOL_JSON_TYPE: &str = "boolean";
const NUMBER_JSON_TYPE: &str = "number";
const STRING_JSON_TYPE: &str = "string";
const ARRAY_JSON_TYPE: &str = "array";
const OBJECT_JSON_TYPE: &str = "object";
const DEFINITION_PREFIX: &str = "#/definitions/";

#[derive(Debug, Snafu)]
pub enum Error {
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
    definitions: Option<&'a Value>,
    path: Vec<String>,
}

impl<'a> ValueCoercer<'a> {
    /// Creates a coercer for a root schema and its local definitions.
    pub fn new(schema: &'a Value) -> Self {
        Self {
            schema,
            definitions: schema.get("definitions"),
            path: Vec::new(),
        }
    }

    /// Coerces a value in place. On error, some fields may already be coerced.
    pub fn coerce(&mut self, value: &mut Value) -> Result<(), Error> {
        self.path.clear();
        self.coerce_value(value, self.schema)
    }

    fn definition(&self, reference: &str) -> Option<&'a Value> {
        let key = reference.strip_prefix(DEFINITION_PREFIX)?;
        self.definitions?.as_object()?.get(key)
    }

    fn coerce_value(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        self.handle_bool(schema)?;
        self.handle_ref(value, schema)?;
        self.handle_all_of(value, schema)?;
        self.handle_one_of(value, schema)?;
        self.handle_any_of(value, schema)?;
        self.handle_enum(value, schema)?;
        self.handle_const(value, schema)?;

        if let Some(type_spec) = schema.get("type") {
            if let Some(t) = type_spec.as_str() {
                return self.coerce_type(value, t, schema);
            } else if let Some(types) = type_spec.as_array() {
                let allowed: Vec<&str> = types.iter().filter_map(|t| t.as_str()).collect();
                return self.coerce_multiple_types(value, &allowed, schema);
            }
        }

        // Only fall back to object/array coercion if no type is specified.
        match value {
            Value::Object(_) => self.coerce_object(value, schema),
            Value::Array(_) => self.coerce_array(value, schema),
            _ => Ok(()),
        }
    }

    fn handle_ref(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
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

    fn handle_bool(&mut self, schema: &Value) -> Result<(), Error> {
        match schema.as_bool() {
            Some(true) | None => Ok(()),
            Some(false) => DisallowedPropertySnafu {
                path: self.path.join("."),
            }
            .fail(),
        }
    }

    fn handle_all_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
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
    fn handle_one_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
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
    fn handle_any_of(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(variants) = schema.get("anyOf").and_then(|v| v.as_array()) else {
            return Ok(());
        };
        self.coerce_any_of(value, variants)
    }

    fn handle_enum(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(enum_vals) = schema.get("enum").and_then(|v| v.as_array()) else {
            return Ok(());
        };

        // Exact match
        if enum_vals.iter().any(|opt| value == opt) {
            return Ok(());
        }

        // Try coercions from string
        if let Value::String(s) = value {
            let s_trimmed = s.trim();

            // String → Bool
            if let Ok(b) = s_trimmed.parse::<bool>()
                && enum_vals.iter().any(|opt| opt.as_bool() == Some(b))
            {
                *value = Value::Bool(b);
                return Ok(());
            }

            // String → Number
            if let Some(n) = parse_number(s_trimmed)
                && enum_vals.iter().any(|opt| opt.as_number() == Some(&n))
            {
                *value = Value::Number(n);
                return Ok(());
            }

            // String → Null
            if s_trimmed.eq_ignore_ascii_case("null") && enum_vals.iter().any(|opt| opt.is_null()) {
                *value = Value::Null;
                return Ok(());
            }
        }

        // Number → String
        if let Value::Number(n) = value {
            let val_str = n.to_string();
            if enum_vals
                .iter()
                .any(|opt| opt.as_str() == Some(val_str.as_str()))
            {
                *value = Value::String(val_str);
                return Ok(());
            }
        }

        // Bool → String
        if let Value::Bool(b) = value {
            let val_str = b.to_string();
            if enum_vals.iter().any(|opt| {
                opt.as_str()
                    .map(|s| s.eq_ignore_ascii_case(val_str.as_str()))
                    == Some(true)
            }) {
                *value = Value::String(val_str);
                return Ok(());
            }
        }

        InvalidEnumValueSnafu {
            path: self.path.join("."),
        }
        .fail()
    }

    fn handle_const(&mut self, value: &mut Value, schema: &Value) -> Result<(), Error> {
        let Some(const_val) = schema.get("const") else {
            return Ok(());
        };

        // Exact match
        if value == const_val {
            return Ok(());
        }

        // String input → try coercion
        if let Value::String(s) = value {
            let s_trimmed = s.trim();

            // String → Bool
            if let Ok(b) = s_trimmed.parse::<bool>()
                && const_val.as_bool() == Some(b)
            {
                *value = Value::Bool(b);
                return Ok(());
            }

            // String → Number
            if let Some(n) = parse_number(s_trimmed)
                && const_val.as_number() == Some(&n)
            {
                *value = Value::Number(n);
                return Ok(());
            }

            // String → Null
            if s_trimmed.eq_ignore_ascii_case("null") && const_val.is_null() {
                *value = Value::Null;
                return Ok(());
            }
        }

        // Number → String
        if let Value::Number(n) = value {
            let val_str = n.to_string();
            if const_val.as_str() == Some(val_str.as_str()) {
                *value = Value::String(val_str);
                return Ok(());
            }
        }

        // Bool → String
        if let Value::Bool(b) = value {
            let val_str = b.to_string();
            if const_val.as_str().map(|s| s.eq_ignore_ascii_case(&val_str)) == Some(true) {
                *value = Value::String(val_str);
                return Ok(());
            }
        }

        InvalidConstSnafu {
            path: self.path.join("."),
            expected: const_val.to_string(),
        }
        .fail()
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

            let result = self.try_coerce_to_allowed_type(&mut new_value, allowed_type, schema);

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
        self.try_coerce_to_allowed_type(value, expected_type, schema)
    }

    fn try_coerce_to_allowed_type(
        &mut self,
        value: &mut Value,
        allowed_type: &str,
        schema: &Value,
    ) -> Result<(), Error> {
        match allowed_type {
            "null" => self.coerce_null(value),
            "boolean" => self.coerce_bool(value),
            "integer" => self.coerce_integer(value),
            "number" => self.coerce_number(value),
            "string" => self.coerce_string(value),
            "object" => {
                if !value.is_object() {
                    return fail_expected!(Object, value, self.path);
                }
                self.coerce_object(value, schema)
            }
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

        // When unevaluatedProperties:false is set (used by Vector's component outer-wrapper schemas
        // like SourceOuter, SinkOuter, EnrichmentTableOuter), collect all properties declared
        // anywhere in the schema — including across allOf/$ref/oneOf — and flag any key not found.
        // This is the right level to do this check: the outer wrapper sees the full union of all
        // properties (component-specific fields + shared fields like `inputs`, `proxy`, `graph`).
        //
        // Guard: skip the check when the value's `type` discriminant is not recognized by any
        // compiled variant. In that case the component is simply not compiled in, and checking
        // would incorrectly flag its fields as unknown.
        let unevaluated_props_false = schema
            .get("unevaluatedProperties")
            .and_then(|v| v.as_bool())
            == Some(false);
        let known_for_unevaluated: Option<HashSet<String>> = if unevaluated_props_false {
            let type_val = obj.get("type").and_then(|t| t.as_str());
            let type_is_known = type_val
                .map(|tv| self.schema_contains_type_discriminant(schema, tv))
                .unwrap_or(false); // no `type` field → not a component config → skip check
            if type_is_known {
                let mut set = HashSet::new();
                // Filter `oneOf` variants by the value's discriminant so properties
                // valid in *other* variants (e.g. a Kafka-only field on an HTTP sink)
                // still trigger the unknown-field check.
                self.collect_known_properties(schema, type_val, &mut set);
                Some(set)
            } else {
                None
            }
        } else {
            None
        };

        for (key, field) in obj.iter_mut() {
            let key_str = key.as_str();
            let initial_len = self.path.len();
            self.path.push(key_str.to_string());

            let field_schema = properties.and_then(|props| props.get(key_str)).or_else(|| {
                additional_properties.and_then(|additional| {
                    if let Some(b) = additional.as_bool() {
                        if b {
                            None // allowed, no specific schema
                        } else {
                            Some(&Value::Bool(false)) // trigger error below
                        }
                    } else {
                        Some(additional) // schema for additional_properties
                    }
                })
            });

            if let Some(field_schema) = field_schema {
                if field_schema == &Value::Bool(false) {
                    self.path.truncate(initial_len);
                    return UnexpectedPropertySnafu {
                        path: self.path.join("."),
                        key: key_str.to_string(),
                    }
                    .fail();
                }

                let result = self.coerce_value(field, field_schema);
                self.path.truncate(initial_len);
                result?;
            } else {
                if let Some(ref known) = known_for_unevaluated
                    && !known.contains(key_str)
                {
                    // Vector's generated JSON Schema currently does not emit
                    // `#[serde(alias = "...")]` aliases (TODO in vector-config),
                    // so a key absent from the schema may still be a legitimate
                    // serde alias. Warn here for visibility; serde performs the
                    // authoritative unknown-field check downstream where it has
                    // alias information.
                    warn!(
                        message = "Unknown field in config, deferring to serde for alias resolution.",
                        path = %self.path.join("."),
                    );
                }
                self.path.truncate(initial_len);
            }
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

                    let schema = tuple_schemas.get(idx).or({
                        match additional_items {
                            Some(Value::Bool(false)) => Some(&Value::Bool(false)), // disallowed
                            Some(s) => Some(s), // additional schema
                            None => None,       // no schema → allow
                        }
                    });

                    if let Some(item_schema) = schema {
                        if item_schema == &Value::Bool(false) {
                            self.path.truncate(initial_len);
                            return UnexpectedArrayElementSnafu {
                                path: self.path.join("."),
                                index: idx,
                            }
                            .fail();
                        }

                        let result = self.coerce_value(item_val, item_schema);
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
                    let result = self.coerce_value(item_val, item_schema);
                    self.path.truncate(initial_len);
                    result?;
                }
            }
        }

        Ok(())
    }

    /// Try variants in order, leaving final validation to serde.
    fn coerce_one_of(&mut self, value: &mut Value, schemas: &[Value]) -> Result<(), Error> {
        let initial_len = self.path.len();
        let mut success: Option<(Value, &Value)> = None; // (coerced value, matched schema)

        for schema in schemas {
            self.path.truncate(initial_len);
            let mut candidate = value.clone();
            if self.coerce_value(&mut candidate, schema).is_ok() {
                self.path.truncate(initial_len);
                if success.is_some() {
                    // Multiple variants match — keep the first and move on.
                    break;
                }
                success = Some((candidate, schema));
            }
        }

        self.path.truncate(initial_len);

        if let Some((val, _matched_schema)) = success {
            *value = val;
            return Ok(());
        }

        // No variant succeeded. If the value carries a `type` discriminant that matches a known
        // variant, re-run coercion strictly against that variant so callers get a path-aware
        // coercion error (e.g. "expected integer at sources.my_source.count") rather than a
        // silent pass-through. Unknown-field detection is handled at the outer wrapper level
        // (see unevaluatedProperties handling in coerce_object).
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
                    return result.map(|_| {
                        *value = candidate;
                    });
                }
            }
        }

        Ok(())
    }

    /// Collect property names declared in `schema`, recursively through `$ref`, `allOf`,
    /// `anyOf`, and `oneOf`. When `discriminant` is `Some`, `oneOf` traversal is filtered
    /// to only the variant whose `properties.type.const` matches — so unknown-field
    /// detection on a tagged-union component doesn't accept fields that are valid only
    /// in *other* variants.
    fn collect_known_properties(
        &self,
        schema: &Value,
        discriminant: Option<&str>,
        out: &mut HashSet<String>,
    ) {
        let resolved = if let Some(ref_str) = schema.get("$ref").and_then(|r| r.as_str()) {
            match self.definition(ref_str) {
                Some(def) => def,
                None => return,
            }
        } else {
            schema
        };

        if let Some(props) = resolved.get("properties").and_then(|p| p.as_object()) {
            out.extend(props.keys().cloned());
        }

        for kw in ["allOf", "anyOf"] {
            if let Some(variants) = resolved.get(kw).and_then(|v| v.as_array()) {
                for sub in variants {
                    self.collect_known_properties(sub, discriminant, out);
                }
            }
        }

        if let Some(variants) = resolved.get("oneOf").and_then(|v| v.as_array()) {
            match discriminant {
                Some(disc) => {
                    // Only collect properties from the variant whose `type` const matches.
                    // If no variant matches (e.g. untagged or non-component oneOf), fall back
                    // to including all variants so callers don't get false positives.
                    let matched: Vec<&Value> = variants
                        .iter()
                        .filter(|v| self.schema_matches_type_discriminant(v, disc))
                        .collect();
                    if matched.is_empty() {
                        for sub in variants {
                            self.collect_known_properties(sub, discriminant, out);
                        }
                    } else {
                        for sub in matched {
                            self.collect_known_properties(sub, discriminant, out);
                        }
                    }
                }
                None => {
                    for sub in variants {
                        self.collect_known_properties(sub, discriminant, out);
                    }
                }
            }
        }
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
            .and_then(|t| t.get("const"))
            .and_then(|c| c.as_str())
            == Some(expected)
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

    /// Returns true if `schema`, after fully resolving `$ref`, `allOf`, and `oneOf`, contains
    /// any variant that claims `expected` as its `type` discriminant. Used to skip the
    /// unevaluatedProperties unknown-field check for components whose type is not compiled in.
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

        if let Some(all_of) = resolved.get("allOf").and_then(|v| v.as_array())
            && all_of
                .iter()
                .any(|sub| self.schema_contains_type_discriminant(sub, expected))
        {
            return true;
        }

        if let Some(one_of) = resolved.get("oneOf").and_then(|v| v.as_array())
            && one_of
                .iter()
                .any(|variant| self.schema_matches_type_discriminant(variant, expected))
        {
            return true;
        }

        false
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
                if self.coerce_value(&mut candidate, schema).is_ok() {
                    self.path.truncate(initial_len);
                    *value = candidate;
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
            Value::String(s) => match s.trim().parse::<bool>() {
                Ok(b) => {
                    *value = Value::Bool(b);
                    Ok(())
                }
                _ => fail_expected!(Bool, value, self.path),
            },
            _ => fail_expected!(Bool, value, self.path),
        }
    }

    fn coerce_integer(&mut self, value: &mut Value) -> Result<(), Error> {
        if let Value::Number(n) = value {
            if n.is_i64() || n.is_u64() {
                return Ok(());
            }

            if let Some(f) = n.as_f64()
                && f.fract() == 0.0
                && f >= (i64::MIN as f64)
                // i64::MAX rounds up to 2^63 as f64; accepting that bound saturates.
                && f < (i64::MAX as f64)
            {
                *value = Value::Number(Number::from(f as i64));
                return Ok(());
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
            Value::String(s) if s.trim().eq_ignore_ascii_case("null") => {
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

fn parse_integer(input: &str) -> Option<Number> {
    if let Ok(i) = input.trim().parse::<i64>() {
        Some(Number::from(i))
    } else if let Ok(u) = input.trim().parse::<u64>() {
        Some(Number::from(u))
    } else {
        None
    }
}

fn parse_number(input: &str) -> Option<Number> {
    if let Some(num) = parse_integer(input) {
        return Some(num);
    }
    if let Ok(f) = input.trim().parse::<f64>() {
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
