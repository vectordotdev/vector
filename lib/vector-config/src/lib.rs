use std::collections::HashMap;

use schema::generate_number_schema;
use schemars::{
    schema::{NumberValidation, StringValidation, SchemaObject},
    JsonSchema, gen::SchemaGenerator,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub fn max_unsigned() -> Self {
        NumberShape::Unsigned {
            minimum: 0,
            maximum: NUM_MAX_BOUND_UNSIGNED,
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

pub struct Field {
    name: &'static str,
    description: &'static str,
    ref_name: Option<&'static str>,
    shape: Shape,
    fields: Option<HashMap<&'static str, Field>>,
    metadata: Metadata<Value>,
}

impl Field {
    pub fn new<'de, T>(name: &'static str, description: &'static str, metadata: Metadata<T>) -> Self
    where
        T: Configurable<'de>,
    {
        Self::with_reference(name, None, description, metadata)
    }

    pub fn referencable<'de, T>(name: &'static str, ref_name: &'static str, description: &'static str, metadata: Metadata<T>) -> Self
    where
        T: Configurable<'de>,
    {
        Self::with_reference(name, Some(ref_name), description, metadata)
    }

    fn with_reference<'de, T>(name: &'static str, ref_name: Option<&'static str>, description: &'static str, metadata: Metadata<T>) -> Self
    where
        T: Configurable<'de>,
    {
        let fields = T::fields(metadata.clone());
        let shape = T::shape();

        Self {
            name,
            description,
            ref_name,
            shape,
            fields,
            metadata: metadata.into_opaque(),
        }
    }
}

#[derive(Clone)]
pub struct Metadata<T: Serialize> {
    default: Option<T>,
    attributes: Vec<(String, String)>,
}

impl<T: Serialize> Metadata<T> {
    fn new(default: Option<T>, attributes: Vec<(String, String)>) -> Self {
        Self {
            default,
            attributes,
        }
    }

    fn map_default<F, U>(self, f: F) -> Metadata<U>
    where
        F: FnOnce(T) -> U,
        U: Serialize,
    {
        Self {
            default: self.default.map(f),
            attributes: self.attributes,
        }
    }

    fn merge(self, other: Metadata<T>) -> Self {
        // TODO: actually merge the attributes
        let merged_attributes = Vec::new();

        Self {
            default: other.default.or(self.default),
            attributes: merged_attributes,
        }
    }

    fn into_parts(self) -> (Option<T>, Vec<(String, String)>) {
        (self.default, self.attributes)
    }
}

impl<T: Serialize> Default for Metadata<T> {
    fn default() -> Self {
        Self {
            default: None,
            attributes: Vec::new(),
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
    fn metadata() -> Metadata<Self> {
        Metadata::default()
    }

    /// Generates the schema for this value.
    fn generate_schema(gen: &mut SchemaGenerator, overrides: Metadata<Self>) -> SchemaObject;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SpecialDuration(u64);

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
        todo!()
    }
}

impl<'de> Configurable<'de> for BatchConfig {
    fn fields(overrides: Metadata<BatchConfig>) -> Option<HashMap<&'static str, Field>> {
        let mut fields = HashMap::new();

        let merged_metadata = Self::metadata().merge(overrides);

        let max_events_desc_raw = Some("the maximum number of events per batch");
        let max_events_desc = max_events_desc_raw
            .or(<Option<u64> as Configurable<'de>>::description())
            .expect("no description present for `max_events`, and `Option<u64>` has no default description");
        let max_events_metadata = <Option<u64> as Configurable<'de>>::metadata().merge(
            merged_metadata
                .clone()
                .map_default(|batch| batch.max_events),
        );
        let max_events_field = Field::new("max_events", max_events_desc, max_events_metadata);
        fields.insert("max_events", max_events_field);

        let max_bytes_desc_raw = Some("the maximum number of bytes per batch");
        let max_bytes_desc = max_bytes_desc_raw
            .or(<Option<u64> as Configurable<'de>>::description())
            .expect("no description present for `max_bytes`, and `Option<u64>` has no default description");
        let max_bytes_metadata = <Option<u64> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|batch| batch.max_bytes));
        let max_bytes_field = Field::new("max_bytes", max_bytes_desc, max_bytes_metadata);
        fields.insert("max_bytes", max_bytes_field);

        let timeout_desc_raw = Some("the timeout before a batch is automatically flushed");
        let timeout_desc = timeout_desc_raw
            .or(<Option<u64> as Configurable<'de>>::description())
            .expect("no description present for `timeout`, and `Option<SpecialDuration>` has no default description");
        let timeout_metadata = <Option<SpecialDuration> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|batch| batch.timeout));
        let timeout_field = Field::new("timeout", timeout_desc, timeout_metadata);
        fields.insert("timeout", timeout_field);

        Some(fields)
    }

    fn shape() -> Shape {
        todo!()
    }
}

impl<'de> Configurable<'de> for SimpleSinkConfig {
    fn shape() -> Shape {
        todo!()
    }

    fn fields(overrides: Metadata<SimpleSinkConfig>) -> Option<HashMap<&'static str, Field>> {
        let mut fields = HashMap::new();

        let merged_metadata = Self::metadata().merge(overrides);

        let endpoint_desc_raw = Some("the endpoint to send the events to");
        let endpoint_desc = endpoint_desc_raw
            .or(<String as Configurable<'de>>::description())
            .expect(
                "no description present for `endpoint`, and `String` has no default description",
            );
        let endpoint_metadata = <String as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|sink| sink.endpoint));
        let endpoint_field = Field::new("endpoint", endpoint_desc, endpoint_metadata);
        fields.insert("endpoint", endpoint_field);

        let batch_desc_raw = None;
        let batch_desc = batch_desc_raw
            .or(<BatchConfig as Configurable<'de>>::description())
            .expect(
                "no description present for `batch`, and `BatchConfig` has no default description",
            );
        let batch_metadata = <BatchConfig as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|sink| sink.batch));
        let batch_field = Field::new("batch", batch_desc, batch_metadata);
        fields.insert("batch", batch_field);

        let tags_desc_raw = Some("the tags added to each event");
        let tags_desc = tags_desc_raw
            .or(<HashMap<String, String> as Configurable<'de>>::description())
            .expect("no description present for `tags`, and `HashMap<String, String>` has no default description");
        let tags_metadata = <HashMap<String, String> as Configurable<'de>>::metadata()
            .merge(merged_metadata.clone().map_default(|sink| sink.tags));
        let tags_field = Field::new("tags", tags_desc, tags_metadata);
        fields.insert("tags", tags_field);

        Some(fields)
    }
}

#[cfg(test)]
mod tests {
    use crate::{schema::generate_root_schema, SimpleSinkConfig};

    #[test]
    fn foo() {
        let schema = generate_root_schema::<SimpleSinkConfig>();
        dbg!(schema);
    }
}
