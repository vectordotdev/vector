//! Shared helpers for checking generated JSON Schema against encoded values.
//!
//! Used by sibling modules of this `[[test]]` crate. rust-analyzer often type-checks this
//! file in isolation, so the helpers look unused unless dead_code is allowed here.

#![allow(dead_code)]

use std::fmt::Debug;

use proptest::prelude::*;
use serde::Serialize;
use vector_config::{Configurable, schema::generate_root_schema};
use vector_config_common::num::{NUMERIC_ENFORCED_LOWER_BOUND, NUMERIC_ENFORCED_UPPER_BOUND};

/// Asserts that `instance` is valid according to the generated schema for `T`.
pub fn assert_schema_allows<T>(instance: serde_json::Value)
where
    T: Configurable + 'static,
{
    let root = generate_root_schema::<T>().expect("should generate root schema");
    let schema = serde_json::to_value(root).expect("serialize schema to JSON");
    let validator = jsonschema::validator_for(&schema).expect("generated schema should compile");
    if let Err(error) = validator.validate(&instance) {
        panic!("expected {instance} to validate against generated schema: {error}");
    }
}

/// Encoded JSON for any `Configurable` value must be accepted by that type's generated schema.
pub fn encoded_value_validates_against_schema<T>(value: &T)
where
    T: Configurable + Serialize + Debug + 'static,
{
    let instance = serde_json::to_value(value)
        .unwrap_or_else(|error| panic!("failed to encode {value:?} as JSON: {error}"));
    assert_schema_allows::<T>(instance);
}

/// Schema generation currently clamps numeric `minimum`/`maximum` to the JSON-safe
/// integer range (`±2^53-1`), so values outside that range encode but do not validate.
pub fn json_schema_safe_u64() -> impl Strategy<Value = u64> {
    0..=(NUMERIC_ENFORCED_UPPER_BOUND as u64)
}

pub fn json_schema_safe_i64() -> impl Strategy<Value = i64> {
    (NUMERIC_ENFORCED_LOWER_BOUND as i64)..=(NUMERIC_ENFORCED_UPPER_BOUND as i64)
}

pub fn json_schema_safe_f64() -> impl Strategy<Value = f64> {
    NUMERIC_ENFORCED_LOWER_BOUND..=NUMERIC_ENFORCED_UPPER_BOUND
}
