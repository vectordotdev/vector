use super::{Error, ValueCoercer};
use serde_json::{Value, json};

struct CoercionCase {
    name: &'static str,
    schema: Value,
    input: Value,
    expected: Value,
}

impl CoercionCase {
    fn check(self) {
        let mut value = self.input;
        ValueCoercer::new(&self.schema)
            .coerce(&mut value)
            .unwrap_or_else(|error| panic!("{} (schema {}): {error}", self.name, self.schema));
        assert_eq!(
            value, self.expected,
            "{} (schema {})",
            self.name, self.schema
        );
    }
}

/// A rejected input that must remain unchanged.
struct RejectionCase {
    name: &'static str,
    schema: Value,
    input: Value,
    expected_error: Error,
}

impl RejectionCase {
    fn check(self) {
        let mut value = self.input.clone();
        let error = ValueCoercer::new(&self.schema)
            .coerce(&mut value)
            .expect_err(self.name);
        assert_eq!(
            std::mem::discriminant(&error),
            std::mem::discriminant(&self.expected_error),
            "{}: {error}",
            self.name
        );
        assert_eq!(
            error.to_string(),
            self.expected_error.to_string(),
            "{}",
            self.name
        );
        assert_eq!(
            value, self.input,
            "{}: input changed on rejection",
            self.name
        );
    }
}

#[test]
fn generated_optional_sensitive_string_preserves_literal_null() {
    use vector_common::sensitive_string::SensitiveString;

    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<Option<SensitiveString>>().unwrap(),
    )
    .unwrap();
    for mut value in [json!("null"), json!("NULL"), json!(null)] {
        let expected = serde_json::from_value::<Option<SensitiveString>>(value.clone()).unwrap();
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(
            serde_json::from_value::<Option<SensitiveString>>(value).unwrap(),
            expected
        );
    }
}

/// Aliases for externally tagged enum variants.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
enum ExternalAliases {
    /// A named mode.
    #[serde(rename = "unit", alias = "legacy_unit")]
    Unit,
    /// A scalar payload.
    #[serde(rename = "number", alias = "legacy_number")]
    Number(u64),
    /// A structured payload.
    #[serde(rename = "object", alias = "legacy_object")]
    Object {
        /// Number of items.
        count: u64,
    },
}

/// Aliases for internally tagged enum variants.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
#[serde(tag = "type")]
enum InternalAliases {
    /// A counted mode.
    #[serde(rename = "counted", alias = "legacy", alias = "older")]
    Counted {
        /// Number of items.
        count: u64,
    },
}

/// Aliases for adjacently tagged enum variants.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
#[serde(tag = "mode", content = "options")]
enum AdjacentAliases {
    /// A counted mode.
    #[serde(rename = "counted", alias = "legacy")]
    Counted(u64),
}

#[test]
fn generated_enum_aliases_preserve_spelling_and_coerce_payloads() {
    fn check<T: vector_config::Configurable + serde::de::DeserializeOwned + 'static>(
        input: serde_json::Value,
        expected: serde_json::Value,
    ) {
        let schema =
            serde_json::to_value(vector_config::schema::generate_root_schema::<T>().unwrap())
                .unwrap();
        let mut value = input;
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value, expected);
        serde_json::from_value::<T>(value).unwrap();
    }
    check::<ExternalAliases>(json!("legacy_unit"), json!("legacy_unit"));
    check::<ExternalAliases>(json!({"legacy_number": "42"}), json!({"legacy_number": 42}));
    check::<ExternalAliases>(
        json!({"legacy_object": {"count": "42"}}),
        json!({"legacy_object": {"count": 42}}),
    );
    for tag in ["counted", "legacy", "older"] {
        check::<InternalAliases>(
            json!({"type": tag, "count": "42"}),
            json!({"type": tag, "count": 42}),
        );
    }
    check::<AdjacentAliases>(
        json!({"mode": "legacy", "options": "42"}),
        json!({"mode": "legacy", "options": 42}),
    );

    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<InternalAliases>().unwrap(),
    )
    .unwrap();
    for mut invalid in [
        json!({"type": "legacy", "count": "bad"}),
        json!({"type": "unknown", "count": "42"}),
        // A variant alias is a tag value, never an alias for the tag key itself.
        json!({"legacy": "counted", "count": "42"}),
    ] {
        assert!(ValueCoercer::new(&schema).coerce(&mut invalid).is_err());
    }
}

#[test]
fn component_boundaries_follow_root_maps_not_nested_unions() {
    for section in super::COMPONENT_MAPS {
        for referenced in [false, true] {
            let outer = json!({"oneOf": [{
                "type": "object",
                "properties": {
                    "type": {"const": "known"},
                    "count": {"type": "integer"},
                    "mode": {"oneOf": [{"properties": {"type": {"const": "valid"}}}]}
                }
            }]});
            let value_schema = if referenced {
                json!({"$ref": "#/definitions/outer"})
            } else {
                outer.clone()
            };
            let schema = json!({
                "allOf": [{"$ref": "#/definitions/config"}],
                "definitions": {
                    "config": {"properties": {
                        section: {"$ref": "#/definitions/map"},
                        "nested": {"properties": {section: {"$ref": "#/definitions/map"}}}
                    }},
                    "map": {"allOf": [{"type": "object", "additionalProperties": value_schema}]},
                    "outer": outer
                }
            });
            let mut coercer = ValueCoercer::new(&schema);
            let mut unknown = json!({section: {"example": {"type": "unknown", "count": "bad"}}});
            let original = unknown.clone();
            coercer.coerce(&mut unknown).unwrap();
            assert_eq!(unknown, original);
            let mut known = json!({section: {"example": {"type": "known", "count": "42"}}});
            coercer.coerce(&mut known).unwrap();
            assert_eq!(known[section]["example"]["count"], json!(42));
            for mut invalid in [
                json!({section: {"example": {"type": "known", "count": "bad"}}}),
                json!({section: {"example": {"type": "known", "mode": {"type": "unknown"}}}}),
                json!({"nested": {section: {"example": {"type": "unknown"}}}}),
            ] {
                assert!(coercer.coerce(&mut invalid).is_err());
            }
        }
    }
}

/// Object alternatives with different required fields.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
#[serde(untagged)]
enum RequiredFields {
    /// The older settings form.
    V1 {
        /// Required database name.
        database: String,
    },
    /// The newer settings form.
    V2 {
        /// Required organization name, including legacy spellings.
        #[serde(rename = "org", alias = "organization", alias = "tenant")]
        organization: String,
    },
}

#[test]
fn generated_object_union_checks_required_fields_and_aliases() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<RequiredFields>().unwrap(),
    )
    .unwrap();
    for key in ["org", "organization", "tenant"] {
        let mut value = json!({key: 42});
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value, json!({key: "42"}));
        assert_eq!(
            serde_json::from_value::<RequiredFields>(value).unwrap(),
            RequiredFields::V2 {
                organization: "42".into()
            }
        );
    }
    assert!(ValueCoercer::new(&schema).coerce(&mut json!({})).is_err());
}

#[test]
fn failed_nested_one_of_allows_the_next_any_of_candidate() {
    let schema = json!({"anyOf": [
        {"oneOf": [{"type": "object", "required": ["name"],
            "properties": {"name": {"type": "string"}}}]},
        {"type": "object", "required": ["count"],
            "properties": {"count": {"type": "integer"}}}
    ]});
    let mut value = json!({"count": "42"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!({"count": 42}));
    assert!(
        ValueCoercer::new(&schema)
            .coerce(&mut json!({"count": "bad"}))
            .is_err()
    );
}

/// An optional flattened tagged setting.
#[vector_config::configurable_component]
#[derive(Debug)]
struct OptionalMode {
    #[serde(flatten)]
    mode: Option<Mode>,
}

/// A mode whose payload needs coercion.
#[vector_config::configurable_component]
#[derive(Debug)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum Mode {
    /// Counted mode.
    Counted {
        /// Number of items.
        count: usize,
    },
}

#[test]
fn generated_optional_flattened_enum_checks_tag_absence() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<OptionalMode>().unwrap(),
    )
    .unwrap();
    let mut absent = json!({});
    ValueCoercer::new(&schema).coerce(&mut absent).unwrap();
    assert!(
        serde_json::from_value::<OptionalMode>(absent)
            .unwrap()
            .mode
            .is_none()
    );
    let mut value = json!({"mode": "counted", "count": "42"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value["count"], json!(42));
    assert!(matches!(
        serde_json::from_value::<OptionalMode>(value).unwrap().mode,
        Some(Mode::Counted { count: 42 })
    ));
    for mut invalid in [
        json!({"mode": "counted", "count": "bad"}),
        json!({"mode": "unknown", "count": "42"}),
    ] {
        assert!(ValueCoercer::new(&schema).coerce(&mut invalid).is_err());
    }
}

#[test]
fn generated_nonzero_number_checks_negation_after_coercion() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<std::num::NonZeroI32>().unwrap(),
    )
    .unwrap();
    assert!(ValueCoercer::new(&schema).coerce(&mut json!("0")).is_err());
    let mut value = json!("42");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(42));
}

#[test]
fn negated_null_does_not_coerce_a_string_to_null() {
    let schema = json!({"not": {"type": "null"}});
    let mut value = json!("null");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!("null"));
    assert!(ValueCoercer::new(&schema).coerce(&mut json!(null)).is_err());
}

#[test]
fn structured_values_are_not_coerced_to_strings() {
    for case in [
        RejectionCase {
            name: "an object must not be serialized to a JSON string",
            schema: json!({"type": "string"}),
            input: json!({"source": "bad"}),
            expected_error: Error::ExpectedString {
                path: "".into(),
                actual: "object",
            },
        },
        RejectionCase {
            name: "an array must not be serialized to a JSON string",
            schema: json!({"type": "string"}),
            input: json!(["bad"]),
            expected_error: Error::ExpectedString {
                path: "".into(),
                actual: "array",
            },
        },
    ] {
        case.check();
    }
}

#[test]
fn failing_object_union_does_not_fall_back_to_json_string() {
    let schema = json!({"properties": {"condition": {"anyOf": [
        {"type": "string"},
        {"type": "object", "properties": {"count": {"type": "integer"}}}
    ]}}});
    let mut value = json!({"condition": {"count": "bad"}});
    let original = value.clone();
    let error = ValueCoercer::new(&schema).coerce(&mut value).unwrap_err();
    assert!(matches!(error, Error::Coerce { ref path, .. } if path == "condition"));
    assert_eq!(value, original);
}

#[test]
fn nullable_array_keeps_null_when_item_union_rejects_it() {
    let schema = json!({
        "type": ["array", "null"],
        "items": {"anyOf": [{"type": "string"}, {"type": "object"}]}
    });
    let mut value = json!(null);
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(null));
}

#[test]
fn rejected_union_is_not_a_successful_noop() {
    let schema = json!({"anyOf": [{"type": "string"}, {"type": "object"}]});
    let mut value = json!(null);
    assert!(ValueCoercer::new(&schema).coerce(&mut value).is_err());
    assert_eq!(value, json!(null));
}

#[test]
fn generated_test_output_preserves_null_conditions() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<crate::config::TestOutput<String>>().unwrap(),
    )
    .unwrap();
    let mut value = json!({"extract_from": ["transform"], "conditions": null});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value["conditions"], json!(null));
    let output: crate::config::TestOutput<String> = serde_json::from_value(value).unwrap();
    assert!(output.conditions.is_none());
}

#[test]
fn scalar_coercions() {
    for case in [
        CoercionCase {
            name: "string to integer",
            schema: json!({"type": "integer"}),
            input: json!("42"),
            expected: json!(42),
        },
        CoercionCase {
            name: "largest unsigned integer",
            schema: json!({"type": "integer"}),
            input: json!(u64::MAX.to_string()),
            expected: json!(u64::MAX),
        },
        CoercionCase {
            name: "smallest signed integer",
            schema: json!({"type": "integer"}),
            input: json!(i64::MIN.to_string()),
            expected: json!(i64::MIN),
        },
        CoercionCase {
            name: "float at 2^63 becomes unsigned without saturating to i64::MAX",
            schema: json!({"type": "integer"}),
            input: json!(i64::MAX as f64),
            expected: json!(1_u64 << 63),
        },
        CoercionCase {
            name: "largest float below 2^64",
            schema: json!({"type": "integer"}),
            input: json!((u64::MAX as f64).next_down()),
            expected: json!(u64::MAX - 2047),
        },
        CoercionCase {
            name: "float at the signed lower bound",
            schema: json!({"type": "integer"}),
            input: json!(i64::MIN as f64),
            expected: json!(i64::MIN),
        },
        CoercionCase {
            name: "negative integral float stays signed",
            schema: json!({"type": "integer"}),
            input: json!(-42.0),
            expected: json!(-42),
        },
        CoercionCase {
            name: "zero float becomes an integer",
            schema: json!({"type": "integer"}),
            input: json!(0.0),
            expected: json!(0),
        },
        CoercionCase {
            name: "string to fractional number",
            schema: json!({"type": "number"}),
            input: json!("1.5"),
            expected: json!(1.5),
        },
        CoercionCase {
            name: "string to boolean",
            schema: json!({"type": "boolean"}),
            input: json!("true"),
            expected: json!(true),
        },
        CoercionCase {
            name: "trimmed string to null",
            schema: json!({"type": "null"}),
            input: json!(" null "),
            expected: json!(null),
        },
        CoercionCase {
            name: "boolean to string",
            schema: json!({"type": "string"}),
            input: json!(false),
            expected: json!("false"),
        },
    ] {
        case.check();
    }
}

#[test]
fn invalid_scalars_do_not_saturate_or_become_nonfinite() {
    for case in [
        RejectionCase {
            name: "unsigned integer overflow",
            schema: json!({"type": "integer"}),
            input: json!("18446744073709551616"),
            expected_error: Error::ExpectedInteger {
                path: "".into(),
                actual: "string",
            },
        },
        RejectionCase {
            name: "signed integer underflow",
            schema: json!({"type": "integer"}),
            input: json!("-9223372036854775809"),
            expected_error: Error::ExpectedInteger {
                path: "".into(),
                actual: "string",
            },
        },
        RejectionCase {
            name: "rounded float boundary must not saturate to u64::MAX",
            schema: json!({"type": "integer"}),
            input: json!(u64::MAX as f64),
            expected_error: Error::ExpectedInteger {
                path: "".into(),
                actual: "number",
            },
        },
        RejectionCase {
            name: "float below the signed lower bound must not saturate",
            schema: json!({"type": "integer"}),
            input: json!((i64::MIN as f64).next_down()),
            expected_error: Error::ExpectedInteger {
                path: "".into(),
                actual: "number",
            },
        },
        RejectionCase {
            name: "fractional number is not an integer",
            schema: json!({"type": "integer"}),
            input: json!(1.5),
            expected_error: Error::ExpectedInteger {
                path: "".into(),
                actual: "number",
            },
        },
        RejectionCase {
            name: "NaN is not a JSON number",
            schema: json!({"type": "number"}),
            input: json!("NaN"),
            expected_error: Error::ExpectedNumber {
                path: "".into(),
                actual: "string",
            },
        },
        RejectionCase {
            name: "infinity is not a JSON number",
            schema: json!({"type": "number"}),
            input: json!("inf"),
            expected_error: Error::ExpectedNumber {
                path: "".into(),
                actual: "string",
            },
        },
        RejectionCase {
            name: "yes is not a boolean spelling",
            schema: json!({"type": "boolean"}),
            input: json!("yes"),
            expected_error: Error::ExpectedBool {
                path: "".into(),
                actual: "string",
            },
        },
        RejectionCase {
            name: "null is not stringified",
            schema: json!({"type": "string"}),
            input: json!(null),
            expected_error: Error::ExpectedString {
                path: "".into(),
                actual: "null",
            },
        },
    ] {
        case.check();
    }
}

#[test]
fn nested_reference_errors_have_paths_and_coercer_is_reusable() {
    let schema = json!({
        "type": "object",
        "properties": {"counts": {"type": "array", "items": {"$ref": "#/definitions/count"}}},
        "definitions": {"count": {"type": "integer"}}
    });
    let mut coercer = ValueCoercer::new(&schema);
    let error = coercer
        .coerce(&mut json!({"counts": ["1", "bad"]}))
        .unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "counts.1"));
    let mut value = json!({"counts": ["2"]});
    coercer.coerce(&mut value).unwrap();
    assert_eq!(value, json!({"counts": [2]}));
    let error = coercer.coerce(&mut json!({"counts": ["bad"]})).unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "counts.0"));
}

#[test]
fn reference_errors_are_explicit() {
    for case in [
        RejectionCase {
            name: "missing local definition",
            schema: json!({"properties": {"value": {"$ref": "#/definitions/missing"}}}),
            input: json!({"value": "1"}),
            expected_error: Error::SchemaReferenceNotFound {
                path: "value".into(),
                reference: "#/definitions/missing".into(),
            },
        },
        RejectionCase {
            name: "external schema references are unsupported",
            schema: json!({"properties": {"value": {"$ref": "https://example.com/schema"}}}),
            input: json!({"value": "1"}),
            expected_error: Error::UnsupportedSchemaReference {
                path: "value".into(),
                reference: "https://example.com/schema".into(),
            },
        },
    ] {
        case.check();
    }
}

#[test]
fn arrays_and_additional_properties() {
    let schema = json!({
        "type": "object",
        "properties": {"tuple": {"type": "array", "items": [{"type": "integer"}, {"type": "boolean"}], "additionalItems": false}},
        "additionalProperties": {"type": "number"}
    });
    let mut value = json!({"tuple": ["2", "false"], "extra": "1.5"});
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!({"tuple": [2, false], "extra": 1.5}));
    let error = ValueCoercer::new(&schema)
        .coerce(&mut json!({"tuple": ["2", "false", 3]}))
        .unwrap_err();
    assert!(
        matches!(error, Error::UnexpectedArrayElement { ref path, index: 2 } if path == "tuple")
    );
    let schema = json!({"type": "array", "items": {"type": "integer"}});
    let mut value = json!("2");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!([2]));
}

#[test]
fn enum_const_and_boolean_schemas() {
    for case in [
        CoercionCase {
            name: "coerce to an allowed enum value",
            schema: json!({"enum": [1, 2]}),
            input: json!("2"),
            expected: json!(2),
        },
        CoercionCase {
            name: "coerce to a boolean constant",
            schema: json!({"const": false}),
            input: json!("false"),
            expected: json!(false),
        },
        CoercionCase {
            name: "true schema preserves the input",
            schema: json!(true),
            input: json!({"untouched": "2"}),
            expected: json!({"untouched": "2"}),
        },
        CoercionCase {
            name: "exact enum match wins over coercion to null",
            schema: json!({"enum": [null, "null"]}),
            input: json!("null"),
            expected: json!("null"),
        },
    ] {
        case.check();
    }
    for case in [
        RejectionCase {
            name: "value outside the enum",
            schema: json!({"enum": [1, 2]}),
            input: json!("bad"),
            expected_error: Error::InvalidEnumValue { path: "".into() },
        },
        RejectionCase {
            name: "value does not match the constant",
            schema: json!({"const": false}),
            input: json!("bad"),
            expected_error: Error::InvalidConst {
                path: "".into(),
                expected: "false".into(),
            },
        },
        RejectionCase {
            name: "false schema rejects every value",
            schema: json!(false),
            input: json!("bad"),
            expected_error: Error::DisallowedProperty { path: "".into() },
        },
    ] {
        case.check();
    }
}

#[test]
fn enum_and_const_share_scalar_conversions_but_keep_distinct_errors() {
    // Run each conversion against both a constant and a single-value enum.
    struct ConstraintCase {
        name: &'static str,
        allowed_value: Value,
        input: Value,
        expected: Value,
    }

    for case in [
        ConstraintCase {
            name: "trimmed string to boolean",
            allowed_value: json!(true),
            input: json!(" true "),
            expected: json!(true),
        },
        ConstraintCase {
            name: "trimmed string to integer",
            allowed_value: json!(42),
            input: json!(" 42 "),
            expected: json!(42),
        },
        ConstraintCase {
            name: "string to fractional number",
            allowed_value: json!(1.5),
            input: json!("1.5"),
            expected: json!(1.5),
        },
        ConstraintCase {
            name: "case-insensitive null with whitespace",
            allowed_value: json!(null),
            input: json!(" NULL "),
            expected: json!(null),
        },
        ConstraintCase {
            name: "number to string",
            allowed_value: json!("42"),
            input: json!(42),
            expected: json!("42"),
        },
        ConstraintCase {
            name: "boolean preserves the allowed uppercase spelling",
            allowed_value: json!("TRUE"),
            input: json!(true),
            expected: json!("TRUE"),
        },
        ConstraintCase {
            name: "false preserves the allowed mixed-case spelling",
            allowed_value: json!("False"),
            input: json!(false),
            expected: json!("False"),
        },
        ConstraintCase {
            name: "exact array match",
            allowed_value: json!([1]),
            input: json!([1]),
            expected: json!([1]),
        },
        ConstraintCase {
            name: "exact object match",
            allowed_value: json!({"count": 1}),
            input: json!({"count": 1}),
            expected: json!({"count": 1}),
        },
    ] {
        for schema in [
            json!({"enum": [case.allowed_value]}),
            json!({"const": case.allowed_value}),
        ] {
            CoercionCase {
                name: case.name,
                schema,
                input: case.input.clone(),
                expected: case.expected.clone(),
            }
            .check();
        }
    }

    for case in [
        RejectionCase {
            name: "enum rejects an unparseable string",
            schema: json!({"properties": {"count": {"enum": [1]}}}),
            input: json!({"count": "bad"}),
            expected_error: Error::InvalidEnumValue {
                path: "count".into(),
            },
        },
        RejectionCase {
            name: "enum rejects an array",
            schema: json!({"properties": {"count": {"enum": [1]}}}),
            input: json!({"count": [1]}),
            expected_error: Error::InvalidEnumValue {
                path: "count".into(),
            },
        },
        RejectionCase {
            name: "enum rejects an object",
            schema: json!({"properties": {"count": {"enum": [1]}}}),
            input: json!({"count": {"count": 1}}),
            expected_error: Error::InvalidEnumValue {
                path: "count".into(),
            },
        },
        RejectionCase {
            name: "const rejects an unparseable string",
            schema: json!({"properties": {"count": {"const": 1}}}),
            input: json!({"count": "bad"}),
            expected_error: Error::InvalidConst {
                path: "count".into(),
                expected: "1".into(),
            },
        },
        RejectionCase {
            name: "const rejects an array",
            schema: json!({"properties": {"count": {"const": 1}}}),
            input: json!({"count": [1]}),
            expected_error: Error::InvalidConst {
                path: "count".into(),
                expected: "1".into(),
            },
        },
        RejectionCase {
            name: "const rejects an object",
            schema: json!({"properties": {"count": {"const": 1}}}),
            input: json!({"count": {"count": 1}}),
            expected_error: Error::InvalidConst {
                path: "count".into(),
                expected: "1".into(),
            },
        },
    ] {
        case.check();
    }
}

#[test]
fn generated_unsigned_integer_accepts_integral_float_above_signed_range() {
    let schema =
        serde_json::to_value(vector_config::schema::generate_root_schema::<u64>().unwrap())
            .unwrap();
    let mut value: Value = serde_json::from_str("1e19").unwrap();
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(
        serde_json::from_value::<u64>(value).unwrap(),
        10_000_000_000_000_000_000
    );
}

#[test]
fn generated_signed_integer_leaves_unsigned_overflow_for_serde() {
    let schema =
        serde_json::to_value(vector_config::schema::generate_root_schema::<i64>().unwrap())
            .unwrap();
    let mut value = json!(i64::MAX as f64);
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(1_u64 << 63));
    assert!(serde_json::from_value::<i64>(value).is_err());
}

/// Boolean-like strings with case-sensitive serde spellings.
#[vector_config::configurable_component]
#[derive(Debug, PartialEq)]
enum BooleanSpelling {
    /// Enabled.
    #[serde(rename = "TRUE")]
    True,
    /// Disabled.
    #[serde(rename = "False")]
    False,
}

#[test]
fn generated_boolean_spelling_deserializes_after_coercion() {
    let schema = serde_json::to_value(
        vector_config::schema::generate_root_schema::<BooleanSpelling>().unwrap(),
    )
    .unwrap();
    let mut value = json!(true);
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(
        serde_json::from_value::<BooleanSpelling>(value).unwrap(),
        BooleanSpelling::True
    );
}

#[test]
fn schema_constraints_are_applied_in_order() {
    let schema = json!({
        "$ref": "#/definitions/number",
        "definitions": {"number": {"type": "integer"}},
        "allOf": [{"type": "string"}],
        "oneOf": [{"type": "integer"}],
        "anyOf": [{"type": "string"}],
        "enum": [42],
        "const": "42",
        "type": "integer",
        "not": {"const": 0}
    });
    let mut value = json!("42");
    ValueCoercer::new(&schema).coerce(&mut value).unwrap();
    assert_eq!(value, json!(42));

    let mut forbidden = schema;
    forbidden["not"]["const"] = json!(42);
    assert!(
        ValueCoercer::new(&forbidden)
            .coerce(&mut json!("42"))
            .is_err()
    );
}

#[test]
fn tagged_union_preserves_specific_error_path() {
    let schema = json!({"properties": {"source": {"oneOf": [
        {"properties": {"type": {"const": "demo"}, "count": {"type": "integer"}}},
        {"properties": {"type": {"const": "other"}, "enabled": {"type": "boolean"}}}
    ]}}});
    let error = ValueCoercer::new(&schema)
        .coerce(&mut json!({"source": {"type": "demo", "count": "bad"}}))
        .unwrap_err();
    assert!(matches!(error, Error::ExpectedInteger { ref path, .. } if path == "source.count"));
}

#[test]
fn any_of_preserves_structurally_compatible_values() {
    let schema = json!({
        "anyOf": [
            { "type": "string" },
            { "$ref": "#/definitions/ConditionMap" }
        ],
        "definitions": {
            "ConditionMap": {
                "oneOf": [{
                    "type": "object",
                    "properties": {
                        "type": { "const": "vrl" },
                        "source": { "type": "string" }
                    }
                }]
            }
        }
    });
    for case in [
        CoercionCase {
            name: "object chooses the map branch, not the earlier string branch",
            schema: schema.clone(),
            input: json!({"type": "vrl", "source": ".status_code != 200"}),
            expected: json!({"type": "vrl", "source": ".status_code != 200"}),
        },
        CoercionCase {
            name: "string stays in the string branch",
            schema,
            input: json!(".status_code != 200"),
            expected: json!(".status_code != 200"),
        },
    ] {
        case.check();
    }
}

#[cfg(all(test, feature = "sources-demo_logs",))]
mod test {
    use crate::config::ConfigBuilder;
    use crate::config::loading::schema_coercion::ValueCoercer;
    use serde_json::json;
    use vector_config::schema::generate_root_schema;

    #[test]
    fn generated_demo_logs_variant_alias_deserializes() {
        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        for format in ["rfc5424", "rfc3164"] {
            let mut value = json!({"sources": {"demo": {"type": "demo_logs", "format": format}}});
            serde_json::from_value::<ConfigBuilder>(value.clone()).unwrap();
            ValueCoercer::new(&schema).coerce(&mut value).unwrap();
            assert_eq!(value["sources"]["demo"]["format"], json!(format));
            serde_json::from_value::<ConfigBuilder>(value).unwrap();
        }
    }

    #[test]
    fn generated_unknown_provider_and_secret_types_pass_through() {
        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        for mut value in [
            json!({"provider": {"type": "unknown", "poll_interval_secs": "bad"}}),
            json!({"secret": {"backend": {"type": "unknown", "timeout": "bad"}}}),
        ] {
            let expected = value.clone();
            ValueCoercer::new(&schema).coerce(&mut value).unwrap();
            assert_eq!(value, expected);
            assert!(serde_json::from_value::<ConfigBuilder>(value).is_err());
        }
        for mut value in [
            json!({"provider": {"type": "http", "poll_interval_secs": "bad"}}),
            json!({"secret": {"backend": {"type": "exec", "command": ["example"], "timeout": "bad"}}}),
        ] {
            assert!(ValueCoercer::new(&schema).coerce(&mut value).is_err());
        }
    }

    #[test]
    fn generated_demo_logs_alias_is_coerced_and_deserializes() {
        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let mut value = json!({"sources": {"demo": {
            "type": "demo_logs", "format": "json", "batch_interval": "1.5"
        }}});
        ValueCoercer::new(&schema).coerce(&mut value).unwrap();
        assert_eq!(value["sources"]["demo"]["batch_interval"], json!(1.5));
        serde_json::from_value::<ConfigBuilder>(value).unwrap();
    }

    #[test]
    fn test_coercion_with_array_support() {
        let mut input = json!({
            "proxy": {
                "enabled": true,
                "http": "http://example.com",
                "https": "https://example.com",
                "no_proxy": "no-proxy.com"
            },
            "enrichment_tables": {
                "memory_table": {
                    "type": "memory",
                    "ttl": 60,
                    "flush_interval": 5,
                    "inputs": ["s0"],
                },
            },
            "secret": {
                "backend_1": {
                    "type": "file",
                    "path": "some.json",
                },
            },
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": "100",
                    "format": "shuffle",
                    "lines": ["777", true, false, 0.1, 123, "some string"],
                    "interval": "1",
                },
            },
            "transforms": {
                "t0": {
                    "inputs": ["s0"],
                    "type": "remap",
                    "source": ".host = \"${HOSTNAME}\""
                },
            },
            "sinks": {
                "sink0": {
                    "inputs": ["t0"],
                    "type": "console",
                    "encoding": {
                        "codec": "json",
                    },
                },
            },
        });

        let demo_logs_schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        ValueCoercer::new(&demo_logs_schema)
            .coerce(&mut input)
            .unwrap();

        assert_eq!(
            input,
            json!({
              "proxy": {
                "enabled": true,
                "http": "http://example.com",
                "https": "https://example.com",
                "no_proxy": ["no-proxy.com"]
              },
              "enrichment_tables": {
                "memory_table": {
                  "type": "memory",
                  "ttl": 60,
                  "flush_interval": 5,
                  "inputs": [
                    "s0"
                  ]
                }
              },
              "secret": {
                "backend_1": {
                  "type": "file",
                  "path": "some.json"
                }
              },
              "sources": {
                "source0": {
                  "type": "demo_logs",
                  "count": 100,
                  "format": "shuffle",
                  "lines": [
                    "777",
                    "true",
                    "false",
                    "0.1",
                    "123",
                    "some string"
                  ],
                  "interval": 1
                }
              },
              "transforms": {
                "t0": {
                  "inputs": [
                    "s0"
                  ],
                  "type": "remap",
                  "source": ".host = \"${HOSTNAME}\""
                }
              },
              "sinks": {
                "sink0": {
                  "inputs": [
                    "t0"
                  ],
                  "type": "console",
                  "encoding": {
                    "codec": "json"
                  }
                }
              }
            })
        );
    }

    #[test]
    fn test_unknown_field_in_known_component_passes_through() {
        // Unknown fields remain non-fatal; serde validates them downstream.
        let mut input = json!({
            "sources": {
                "source0": {
                    "type": "demo_logs",
                    "count": 100,
                    "totally_unknown_field": "oops",
                    "format": "json",
                }
            }
        });

        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let result = ValueCoercer::new(&schema).coerce(&mut input);

        assert!(
            result.is_ok(),
            "unknown field should pass coercion (serde validates downstream), got: {result:?}"
        );
    }

    #[test]
    fn test_unknown_component_type_passes_through() {
        let mut input = json!({
            "sinks": {
                "s3_sink": {
                    "type": "aws_s3_totally_nonexistent",
                    "bucket": "my-bucket",
                }
            }
        });

        let schema =
            serde_json::to_value(generate_root_schema::<ConfigBuilder>().unwrap()).unwrap();
        let result = ValueCoercer::new(&schema).coerce(&mut input);

        assert!(
            result.is_ok(),
            "unknown component type should pass through coercion, got: {result:?}"
        );
    }
}
