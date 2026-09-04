//! Golden-file snapshots for schema-generation bugs.
//!
//! These types are intentionally minimal so that a later change to flatten-optional
//! handling or integer bound serialization produces a small, reviewable schema diff,
//! independent of the whole-config snapshot.
//!
//! Prefer `assert_schema_matches_snapshot` for new cases: a golden file pins the
//! whole schema shape, including `_metadata`. Hand-rolled walkers are only needed
//! when a property cannot appear in a snapshot (for example title/description,
//! which `prune_docs` strips).

#![allow(dead_code)]

use assert_json_diff::assert_json_eq;
use proptest::prelude::*;
use proptest_derive::Arbitrary;
use serde_json::json;
use vector_config::{Configurable, configurable_component, schema::generate_root_schema};
use vector_config_common::num::{NUMERIC_ENFORCED_LOWER_BOUND, NUMERIC_ENFORCED_UPPER_BOUND};

use super::schema_validation::{
    assert_schema_allows, encoded_value_validates_against_schema, json_schema_safe_f64,
    json_schema_safe_u64,
};

/// Internally tagged enum used as the flattened optional payload.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
#[serde(tag = "type")]
enum InternallyTaggedMode {
    /// First variant.
    Foo {
        /// A value for foo.
        value: String,
    },
    /// Second variant.
    Bar {
        /// A value for bar.
        count: u32,
    },
}

/// A required sibling plus a flattened optional internally-tagged enum.
///
/// Omitting the flattened block is valid for serde; the generated schema must
/// accept that document as well.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
struct FlattenedOptionalEnum {
    /// A required sibling so the parent is not entirely flattened.
    name: String,

    #[serde(flatten)]
    mode: Option<InternallyTaggedMode>,
}

/// Internally tagged enum with a trailing untagged object fallback.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
#[serde(tag = "type")]
enum InternallyTaggedModeWithFallback {
    /// Tagged variant.
    Foo {
        /// A value for foo.
        value: String,
    },
    /// Tagless object fallback.
    #[serde(untagged)]
    Extra {
        /// A fallback field.
        extra: String,
    },
}

/// Flattened `Option` of an internally tagged enum with an untagged object fallback.
///
/// `Some(Extra { .. })` has no tag field, so the absence branch and the enum
/// branch both match; the wrapper must allow that overlap.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
struct FlattenedOptionalEnumWithFallback {
    /// A required sibling so the parent is not entirely flattened.
    name: String,

    #[serde(flatten)]
    mode: Option<InternallyTaggedModeWithFallback>,
}

/// Same `Option<InternallyTaggedMode>` used twice: as a flattened field and as a
/// normal property. Flatten is generated from `T`, so the property keeps the
/// nullable `Option<T>` encoding (inlined when it is the only remaining use).
/// Field-specific metadata (`docs::hidden`) belongs on the flatten wrapper.
#[derive(Clone, Debug)]
#[configurable_component]
struct SharedFlattenedHiddenOptional {
    /// A required sibling so the parent is not entirely flattened.
    name: String,

    /// Hidden mode.
    ///
    /// Not shown in docs.
    #[serde(flatten)]
    #[configurable(metadata(docs::hidden))]
    mode: Option<InternallyTaggedMode>,

    /// A second use of the same optional type as a normal property.
    extra: Option<InternallyTaggedMode>,
}

/// A plain nullable property, used to prove the flatten fix does not change
/// ordinary `Option<T>` fields that really can be JSON `null`.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
struct PlainOptionalProperty {
    /// A required sibling.
    name: String,

    /// An optional enum as a normal property, not flattened.
    mode: Option<InternallyTaggedMode>,
}

/// Isolates how integer vs floating-point bounds are serialized into JSON Schema.
#[derive(Arbitrary, Clone, Debug)]
#[configurable_component]
struct IntegerBoundedFields {
    /// An unsigned 64-bit integer.
    #[proptest(strategy = "json_schema_safe_u64()")]
    unsigned: u64,
    /// A signed 16-bit integer.
    small: i16,
    /// A floating-point number.
    #[proptest(strategy = "json_schema_safe_f64()")]
    float: f64,
}

#[test]
fn flattened_optional_enum_schema_snapshot() {
    assert_schema_matches_snapshot::<FlattenedOptionalEnum>(include_str!(
        "snapshots/flattened_optional_enum.json"
    ));
}

#[test]
fn flattened_optional_enum_omitted_block_validates() {
    assert_schema_allows::<FlattenedOptionalEnum>(json!({ "name": "example" }));
}

#[test]
fn flattened_optional_enum_with_fallback_schema_snapshot() {
    assert_schema_matches_snapshot::<FlattenedOptionalEnumWithFallback>(include_str!(
        "snapshots/flattened_optional_enum_with_fallback.json"
    ));
}

#[test]
fn flattened_optional_enum_with_fallback_validates() {
    assert_schema_allows::<FlattenedOptionalEnumWithFallback>(json!({ "name": "example" }));
    assert_schema_allows::<FlattenedOptionalEnumWithFallback>(json!({
        "name": "example",
        "type": "Foo",
        "value": "ok",
    }));
    assert_schema_allows::<FlattenedOptionalEnumWithFallback>(json!({
        "name": "example",
        "extra": "ok",
    }));
}

#[test]
fn shared_flattened_hidden_optional_schema_snapshot() {
    assert_schema_matches_snapshot::<SharedFlattenedHiddenOptional>(include_str!(
        "snapshots/shared_flattened_hidden_optional.json"
    ));
}

#[test]
fn flattened_optional_field_docs_stay_on_wrapper() {
    let root = generate_root_schema::<SharedFlattenedHiddenOptional>().expect("should generate");
    let schema = serde_json::to_value(root).expect("serialize schema to JSON");
    let wrapper = schema
        .pointer("/allOf/1")
        .expect("flattened optional should be the second allOf member");

    assert_eq!(
        wrapper.get("title").and_then(serde_json::Value::as_str),
        Some("Hidden mode.")
    );
    assert_eq!(
        wrapper
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Not shown in docs.")
    );
    assert_eq!(wrapper["_metadata"]["docs::hidden"], true);
    assert_eq!(wrapper["_metadata"]["docs::optional"], true);
}

#[test]
fn plain_optional_property_still_accepts_null() {
    assert_schema_allows::<PlainOptionalProperty>(json!({
        "name": "example",
        "mode": null,
    }));
    assert_schema_allows::<PlainOptionalProperty>(json!({ "name": "example" }));
}

#[test]
fn integer_bounded_fields_schema_snapshot() {
    let actual = snapshot_json::<IntegerBoundedFields>();

    // `±(2^53-1)` is exactly representable as `f64` (and is what schema generation
    // emits) but serde_json's JSON text parser rounds those literals down by 1 ULP.
    // Pin the live bounds here, then overlay them on the parsed snapshot so the
    // rest of the file can still compare as `Value`, insensitive to the actual textual format.
    let upper = json!(NUMERIC_ENFORCED_UPPER_BOUND);
    let lower = json!(NUMERIC_ENFORCED_LOWER_BOUND);
    assert_eq!(&actual["properties"]["unsigned"]["maximum"], &upper);
    assert_eq!(&actual["properties"]["float"]["maximum"], &upper);
    assert_eq!(&actual["properties"]["float"]["minimum"], &lower);

    let mut expected: serde_json::Value =
        serde_json::from_str(include_str!("snapshots/integer_bounded_fields.json"))
            .expect("snapshot should be valid JSON");
    expected["properties"]["unsigned"]["maximum"] = upper.clone();
    expected["properties"]["float"]["maximum"] = upper;
    expected["properties"]["float"]["minimum"] = lower;
    assert_json_eq!(expected, actual);
}

proptest! {
    #[test]
    fn internally_tagged_mode_encoded_values_validate_schema(value: InternallyTaggedMode) {
        encoded_value_validates_against_schema(&value);
    }

    #[test]
    fn flattened_optional_enum_encoded_values_validate_schema(value: FlattenedOptionalEnum) {
        encoded_value_validates_against_schema(&value);
    }

    #[test]
    fn flattened_optional_enum_with_fallback_encoded_values_validate_schema(
        value: FlattenedOptionalEnumWithFallback,
    ) {
        encoded_value_validates_against_schema(&value);
    }

    #[test]
    fn plain_optional_property_encoded_values_validate_schema(value: PlainOptionalProperty) {
        encoded_value_validates_against_schema(&value);
    }

    #[test]
    fn integer_bounded_fields_encoded_values_validate_schema(value: IntegerBoundedFields) {
        encoded_value_validates_against_schema(&value);
    }
}

fn assert_schema_matches_snapshot<T>(expected: &str)
where
    T: Configurable + 'static,
{
    let actual = snapshot_json::<T>();
    let expected: serde_json::Value =
        serde_json::from_str(expected).expect("snapshot should be valid JSON");
    assert_json_eq!(expected, actual);
}

fn snapshot_json<T>() -> serde_json::Value
where
    T: Configurable + 'static,
{
    let root = generate_root_schema::<T>().expect("should generate root schema");
    let mut schema = serde_json::to_value(root).expect("serialize schema to JSON");
    prune_docs(&mut schema);
    schema
}

/// Drop documentation noise so later phases' structural diffs stay small.
///
/// Keeps `minimum`/`maximum`, `unevaluatedProperties`, and `_metadata` — those are
/// the fields later phases either fix or must not regress.
fn prune_docs(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => values.iter_mut().for_each(prune_docs),
        serde_json::Value::Object(values) => {
            values.retain(|key, _| !matches!(key.as_str(), "title" | "description"));
            values.values_mut().for_each(prune_docs);
        }
        _ => {}
    }
}
