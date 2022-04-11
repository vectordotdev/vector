#![allow(unused_variables)]

use std::{
    collections::{BTreeSet, HashMap},
    marker::PhantomData,
};

use indexmap::IndexMap;
use schema::{finalize_schema, generate_struct_schema};
use schemars::{
    gen::SchemaGenerator,
    schema::{NumberValidation, SchemaObject, StringValidation},
};
use serde::{Deserialize, Serialize};

mod schema;
mod stdlib;

const NUM_MANTISSA_BITS: u32 = 53;
const NUM_MAX_BOUND_UNSIGNED: u64 = 2u64.pow(NUM_MANTISSA_BITS);
const NUM_MIN_BOUND_SIGNED: i64 = -2i64.pow(NUM_MANTISSA_BITS);
const NUM_MAX_BOUND_SIGNED: i64 = 2i64.pow(NUM_MANTISSA_BITS);

/// The shape of the field.
///
/// This maps similiar to the concept of JSON's data types, where types are generalized and have
/// generalized representations. This allows us to provide general-but-relevant mappings to core
/// types, such as integers and strings and so on, while providing escape hatches for customized
/// types that may be encoded and decoded via "normal" types but otherwise have specific rules or
/// requirements.
///
/// Additionally, the shape of a field can encode some basic properties about the field to which it
/// is attached. For example, numbers can be bounded on or the lower or upper end, while strings
/// could define a minimum length, or even an allowed pattern via regular expressions.
///
/// In this way, they describe a more complete shape of the field than simply the data type alone.
#[derive(Clone)]
pub enum Shape {
    Boolean,
    String(StringShape),
    Number(NumberShape),
    Array(ArrayShape),
    Map(MapShape),
    Optional(Box<Shape>),
    Composite(Vec<Shape>),
}

impl Shape {
    pub fn is_optional(&self) -> bool {
        matches!(self, Self::Optional(..))
    }
}

#[derive(Clone, Default)]
pub struct StringShape {
    minimum_length: Option<u32>,
    maximum_length: Option<u32>,
    allowed_pattern: Option<&'static str>,
}

impl From<StringShape> for StringValidation {
    fn from(s: StringShape) -> Self {
        StringValidation {
            max_length: s.maximum_length,
            min_length: s.minimum_length,
            pattern: s.allowed_pattern.map(|s| s.to_string()),
        }
    }
}

#[derive(Clone)]
pub enum NumberShape {
    Unsigned { minimum: u64, maximum: u64 },
    Signed { minimum: i64, maximum: i64 },
    FloatingPoint { minimum: f64, maximum: f64 },
}

impl NumberShape {
    pub fn unsigned(upper: u64) -> Self {
        NumberShape::Unsigned {
            minimum: 0,
            maximum: NUM_MAX_BOUND_UNSIGNED.min(upper),
        }
    }

    pub fn signed(lower: i64, upper: i64) -> Self {
        NumberShape::Signed {
            minimum: NUM_MIN_BOUND_SIGNED.min(lower),
            maximum: NUM_MAX_BOUND_SIGNED.min(upper),
        }
    }
}

impl From<NumberShape> for NumberValidation {
    fn from(s: NumberShape) -> Self {
        // SAFETY: Generally speaking, we don't like primitive casts -- `foo as ...` -- because they
        // can end up being silently lossy. That is certainly true here in the case of trying to
        // convert an i64 or u64 to f64.
        //
        // The reason it's (potentially) lossy is due to the internal layout of f64, where,
        // essentially, the mantissa is 53 bits, so it can precisely represent an integer up to 2^53
        // such that if you tried to convert 2^53 + 1 to an f64, and then back to an u64, you would
        // end up with a different value than 2^53 + 1.
        //
        // All of this is a long way of saying: we limit integers to 2^53 so that we can always be
        // sure that when we end up specifying their minimum/maximum in the schema, the values we
        // give can be represented concretely and losslessly. In turn, this makes the primitive
        // casts "safe", because we know we're not losing precision.
        let (minimum, maximum) = match s {
            NumberShape::Unsigned { minimum, maximum } => {
                if maximum > NUM_MAX_BOUND_UNSIGNED {
                    panic!(
                        "unsigned integers cannot have a maximum bound greater than 2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                (minimum as f64, maximum as f64)
            }
            NumberShape::Signed { minimum, maximum } => {
                if minimum > NUM_MIN_BOUND_SIGNED {
                    panic!(
                        "signed integers cannot have a minimum bound less than than -2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                if maximum > NUM_MAX_BOUND_SIGNED {
                    panic!(
                        "signed integers cannot have a maximum bound greater than 2^{}",
                        NUM_MANTISSA_BITS
                    );
                }

                (minimum as f64, maximum as f64)
            }
            NumberShape::FloatingPoint { minimum, maximum } => (minimum, maximum),
        };

        NumberValidation {
            minimum: Some(minimum),
            maximum: Some(maximum),
            ..Default::default()
        }
    }
}

#[derive(Clone)]
pub struct ArrayShape {
    element_shape: Box<Shape>,
    minimum_length: Option<u32>,
    maximum_length: Option<u32>,
}

#[derive(Clone)]
pub struct MapShape {
    required_fields: HashMap<&'static str, Shape>,
    allowed_unknown_field_shape: Option<Box<Shape>>,
}
#[derive(Clone)]
pub struct Metadata<'de, T: Configurable<'de>> {
    description: Option<&'static str>,
    default: Option<T>,
    attributes: Vec<(String, String)>,
    _de: PhantomData<&'de ()>,
}

impl<'de, T: Configurable<'de>> Metadata<'de, T> {
    pub fn new(
        description: Option<&'static str>,
        default: Option<T>,
        attributes: Vec<(String, String)>,
    ) -> Self {
        Self {
            description,
            default,
            attributes,
            _de: PhantomData,
        }
    }

    pub fn description(desc: &'static str) -> Self {
        Self {
            description: Some(desc),
            ..Default::default()
        }
    }

    pub fn map_default<F, U>(self, f: F) -> Metadata<'de, U>
    where
        F: FnOnce(T) -> U,
        U: Configurable<'de>,
    {
        Metadata {
            description: self.description,
            default: self.default.map(f),
            attributes: self.attributes,
            _de: PhantomData,
        }
    }

    pub fn merge(self, other: Metadata<'de, T>) -> Self {
        // TODO: actually merge the attributes
        let merged_attributes = Vec::new();

        Self {
            description: other.description.or(self.description),
            default: other.default.or(self.default),
            attributes: merged_attributes,
            _de: PhantomData,
        }
    }

    pub fn into_parts(self) -> (Option<&'static str>, Option<T>, Vec<(String, String)>) {
        (self.description, self.default, self.attributes)
    }
}

impl<'de, T: Configurable<'de>> Default for Metadata<'de, T> {
    fn default() -> Self {
        Self {
            description: None,
            default: None,
            attributes: Vec::new(),
            _de: PhantomData,
        }
    }
}

pub trait Configurable<'de>: Serialize + Deserialize<'de> + Sized
where
    Self: Clone,
{
    /// Gets the referencable name of this value, if any.
    ///
    /// When specified, this implies the value is both complex and standardized, and should be
    /// reused within any generated schema it is present in.
    fn referencable_name() -> Option<&'static str> {
        None
    }

    /// Gets the human-readable description of this value, if any.
    ///
    /// For standard types, this will be `None`. Commonly, custom types would implement this
    /// directly, while fields using standard types would provide a field-specific description that
    /// would be used instead of the default descrption.
    fn description() -> Option<&'static str> {
        None
    }

    /// Gets the shape for this value.
    fn shape() -> Shape;

    /// Gets the metadata for this value.
    fn metadata() -> Metadata<'de, Self> {
        Metadata::default()
    }

    /// Generates the schema for this value.
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject;
}

/// A period of time.
#[derive(Clone, Serialize, Deserialize)]
pub struct SpecialDuration(u64);

/// Controls the batching behavior of events.
#[derive(Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    max_events: Option<u64>,
    max_bytes: Option<u64>,
    timeout: Option<SpecialDuration>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimpleSinkConfig {
    endpoint: String,
    batch: BatchConfig,
    tags: HashMap<String, String>,
}

impl<'de> Configurable<'de> for SpecialDuration {
    fn shape() -> Shape {
        Shape::Number(NumberShape::unsigned(u64::MAX))
    }

    fn metadata() -> Metadata<'de, Self> {
        Metadata::description("A period of time.")
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let merged_metadata = Self::metadata().merge(overrides);

        // We generate the schema for the inner unnamed field, and then apply the metadata to it.
        let inner_metadata = <u64 as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|default| default.0));

        let mut inner_schema =
            <u64 as Configurable<'de>>::generate_schema(gen, inner_metadata.clone());
        finalize_schema(gen, &mut inner_schema, inner_metadata);

        inner_schema
    }
}

impl<'de> Configurable<'de> for BatchConfig {
    fn shape() -> Shape {
        Shape::Map(MapShape {
            required_fields: HashMap::new(),
            allowed_unknown_field_shape: None,
        })
    }

    fn metadata() -> Metadata<'de, Self> {
        Metadata::description("Controls the batching behavior of events.")
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let mut properties = IndexMap::new();
        let mut required = BTreeSet::new();

        let merged_metadata = Self::metadata().merge(overrides);

        // Schema for `max_events`:
        let max_events_metadata = <Option<u64> as Configurable<'de>>::metadata()
            .merge(
                merged_metadata
                    .clone()
                    .map_default(|batch| batch.max_events),
            )
            .merge(Metadata::description(
                "the maximum number of events per batch",
            ));
        let max_events_is_optional = <Option<u64> as Configurable<'de>>::shape().is_optional();
        let mut max_events_schema =
            <Option<u64> as Configurable<'de>>::generate_schema(gen, max_events_metadata.clone());
        finalize_schema(gen, &mut max_events_schema, max_events_metadata);

        if let Some(_) = properties.insert("max_events".to_string(), max_events_schema) {
            panic!(
                "schema properties already contained entry for `max_events`, this should not occur"
            );
        }

        if !max_events_is_optional {
            if !required.insert("max_events".to_string()) {
                panic!("schema properties already contained entry for `max_events`, this should not occur");
            }
        }

        // Schema for `max_bytes`:
        let max_bytes_metadata = <Option<u64> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|batch| batch.max_bytes))
            .merge(Metadata::description(
                "the maximum number of bytes per batch",
            ));
        let max_bytes_is_optional = <Option<u64> as Configurable<'de>>::shape().is_optional();
        let mut max_bytes_schema =
            <Option<u64> as Configurable<'de>>::generate_schema(gen, max_bytes_metadata.clone());
        finalize_schema(gen, &mut max_bytes_schema, max_bytes_metadata);

        if let Some(_) = properties.insert("max_bytes".to_string(), max_bytes_schema) {
            panic!(
                "schema properties already contained entry for `max_bytes`, this should not occur"
            );
        }

        if !max_bytes_is_optional {
            if !required.insert("max_bytes".to_string()) {
                panic!("schema properties already contained entry for `max_bytes`, this should not occur");
            }
        }

        // Schema for `timeout`
        let timeout_metadata = <Option<SpecialDuration> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|batch| batch.timeout))
            .merge(Metadata::description(
                "the timeout before a batch is automatically flushed",
            ));
        let timeout_is_optional =
            <Option<SpecialDuration> as Configurable<'de>>::shape().is_optional();
        let mut timeout_schema = <Option<SpecialDuration> as Configurable<'de>>::generate_schema(
            gen,
            timeout_metadata.clone(),
        );
        finalize_schema(gen, &mut timeout_schema, timeout_metadata);

        if let Some(_) = properties.insert("timeout".to_string(), timeout_schema) {
            panic!(
                "schema properties already contained entry for `timeout`, this should not occur"
            );
        }

        if !timeout_is_optional {
            if !required.insert("timeout".to_string()) {
                panic!("schema properties already contained entry for `timeout`, this should not occur");
            }
        }

        // Schema for `BatchConfig`:
        let additional_properties = None;
        let mut schema = generate_struct_schema(gen, properties, required, additional_properties);
        finalize_schema(gen, &mut schema, merged_metadata);

        schema
    }
}

impl<'de> Configurable<'de> for SimpleSinkConfig {
    fn shape() -> Shape {
        Shape::Map(MapShape {
            required_fields: HashMap::new(),
            allowed_unknown_field_shape: None,
        })
    }

    fn metadata() -> Metadata<'de, Self> {
        Metadata::description("A sink for sending events to the `simple` service.")
    }

    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<'de, Self>) -> SchemaObject {
        let mut properties = IndexMap::new();
        let mut required = BTreeSet::new();

        let merged_metadata = Self::metadata().merge(overrides);

        // Schema for `endpoint`:
        let endpoint_metadata = <String as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|sink| sink.endpoint))
            .merge(Metadata::description("the endpoint to send events to"));
        let endpoint_is_optional = <String as Configurable<'de>>::shape().is_optional();
        let mut endpoint_schema =
            <String as Configurable<'de>>::generate_schema(gen, endpoint_metadata.clone());
        finalize_schema(gen, &mut endpoint_schema, endpoint_metadata);

        if let Some(_) = properties.insert("endpoint".to_string(), endpoint_schema) {
            panic!(
                "schema properties already contained entry for `endpoint`, this should not occur"
            );
        }

        if !endpoint_is_optional {
            if !required.insert("endpoint".to_string()) {
                panic!("schema properties already contained entry for `endpoint`, this should not occur");
            }
        }

        // Schema for `batch`:
        let batch_metadata = <BatchConfig as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|sink| sink.batch));
        let batch_is_optional = <BatchConfig as Configurable<'de>>::shape().is_optional();
        let mut batch_schema =
            <BatchConfig as Configurable<'de>>::generate_schema(gen, batch_metadata.clone());
        finalize_schema(gen, &mut batch_schema, batch_metadata);

        if let Some(_) = properties.insert("batch".to_string(), batch_schema) {
            panic!("schema properties already contained entry for `batch`, this should not occur");
        }

        if !batch_is_optional {
            if !required.insert("batch".to_string()) {
                panic!(
                    "schema properties already contained entry for `batch`, this should not occur"
                );
            }
        }

        // Schema for `tags`:
        let tags_metadata = <HashMap<String, String> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|batch| batch.tags))
            .merge(Metadata::description(
                "the tags to additionally add to each event",
            ));
        let tags_is_optional =
            <HashMap<String, String> as Configurable<'de>>::shape().is_optional();
        let mut tags_schema = <HashMap<String, String> as Configurable<'de>>::generate_schema(
            gen,
            tags_metadata.clone(),
        );
        finalize_schema(gen, &mut tags_schema, tags_metadata);

        if let Some(_) = properties.insert("tags".to_string(), tags_schema) {
            panic!("schema properties already contained entry for `tags`, this should not occur");
        }

        if !tags_is_optional {
            if !required.insert("tags".to_string()) {
                panic!(
                    "schema properties already contained entry for `tags`, this should not occur"
                );
            }
        }

        // Schema for `SimpleSinkConfig`:
        let additional_properties = None;
        let mut schema = generate_struct_schema(gen, properties, required, additional_properties);
        finalize_schema(gen, &mut schema, merged_metadata);

        schema
    }
}

#[cfg(test)]
mod tests {
    use crate::{schema::generate_root_schema, SimpleSinkConfig};

    #[test]
    fn foo() {
        let schema = generate_root_schema::<SimpleSinkConfig>();
        let as_json =
            serde_json::to_string_pretty(&schema).expect("schema should not fail to serialize");
        println!("{}", as_json);
    }
}
