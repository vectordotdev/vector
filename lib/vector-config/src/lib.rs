use std::{time::Duration, stream::Stream, collections::HashMap};

use serde::{Serialize, Deserialize};
use serde_json::Value;

/// The shape of the field.
/// 
/// This maps similiar to the concept of JSON's data types, where types are generalized and have
/// generalized representations.  This allows us to provide general-but-relevant mappings to core
/// types, such as integers and strings and so on, while providing escape hatches for customized
/// types that may be encoded and decoded via "normal" types but otherwise have specific rules or
/// requirements.
/// 
/// Additionally, the shape of a field can encode some basic properties about the field to which it
/// is attached.  For example, numbers can be bounded on or the lower or upper end, while strings
/// could define a minimum length, or even an allowed pattern via regular expressions.
/// 
/// In this way, they describe a more complete shape of the field than simply the data type alone.
pub enum Shape {
    Boolean,
    String(StringShape),
    Number(NumberShape),
    Array(ArrayShape),
    Map(MapShape),
    Composite(Vec<Shape>),
}

pub struct StringShape {
    minimum_length: Option<usize>,
    maximum_length: Option<usize>,
    allowed_pattern: Option<&'static str>,
}

pub enum NumberShape {
    Unsigned {
        theoretical_upper_bound: u128,
	    effective_lower_bound: u128,
	    effective_upper_bound: u128,
    },
    Signed {
        theoretical_upper_bound: i128,
	    effective_lower_bound: i128,
	    effective_upper_bound: i128,
    },
    FloatingPoint {
        theoretical_upper_bound: f64,
	    effective_lower_bound: f64,
	    effective_upper_bound: f64,
    }
}

pub struct ArrayShape {
    element_shape: Shape,
    minimum_length: Option<usize>,
    maximum_length: Option<usize>,
}

pub enum MapShape {
    Fixed(HashMap<String, Shape>),
    Dynamic(Shape),
}

pub struct Field {
    name: &'static str,
    description: &'static str,
    shape: Shape,
    fields: Vec<Field>,
    metadata: HashMap<String, String>,
    default: Option<Value>,
}

impl Field {
    fn new<'de, T: Configurable<'de> + Clone>(
        name: &'static str,
        description: &'static str,
        shape: Shape,
    ) -> Self {
        Self {
            name,
            description,
            shape,
            fields: Vec::new(),
            metadata: HashMap::new(),
            default: None,
        }
    }
}

pub trait Configurable<'de>: Serialize + Deserialize<'de> + Sized 
where
    Self: Clone,
{
    /// Gets the human-readable description of this value, if any.
    ///
    /// For standard types, this will be `None`.  Commonly, custom types would implement this
    /// directly, while fields using standard types would provide a field-specific description that
    /// would be used instead of the default descrption.
    fn description() -> Option<&'static str>;

    /// Gets the shape of this value.
    fn shape() -> Shape;

    /// Gets the metadata for this value.
    fn metadata() -> Option<HashMap<String, String>>;

	/// The fields for this value, if any.
    fn fields() -> Option<Vec<Field>>;
}
#[derive(Serialize, Deserialize, Clone)]
struct SinkConfig {
    url: String,
    // default[some_fn_that_returns_a_method]
    batch: BatchConfig,
}

impl<'de> Configurable<'de> for SinkConfig {
    fn description() -> Option<&'static str> {
        Some("config for the XYZ sink")
    }

    fn shape() -> Shape {
        Shape::Map
    }

    // TODO: bring back typedmetadata, i think it's a much cleaner solution to shoving in generic
    // typed data that we need for field generation and generic metadata K/V pairs

    fn metadata() -> Option<Vec<TypedMetadata<Self>>> {
        Some(vec![
            TypedMetadata::DefaultValue(SinkConfig {
                url: String::from("foo"),
                batch: BatchConfig::default(), 
            })
        ])
    }

    fn fields(overrides: Option<Vec<TypedMetadata<Self>>>) -> Option<Vec<Field>> {
        let base_metadata = <Self as Configurable>::metadata();
        let merged_metadata = merge_metadata_overrides(base_metadata, overrides);

        let url_override_metadata = [
            try_derive_field_default_from_self(&merged_metadata, |default: &Self| {
                default.url.clone()
            }),
        ]
        .into_iter()
        .flatten()
        .collect();

        let batch_override_metadata = [
            try_derive_field_default_from_self(&merged_metadata, |default: &Self| {
                default.batch.clone()
            }),
        ]
        .into_iter()
        .flatten()
        .collect();

        Some(vec![Field::new::<String>(
            "url",
            "the endpoint to send requests to",
            Shape::String,
            merge_metadata_overrides(<String as Configurable>::metadata(), Some(url_override_metadata)),
        ),
        Field::new::<BatchConfig>(
            "batch",
            <BatchConfig as Configurable>::description().expect("BatchConfig has no defined description, and an override description was not provided"),
            Shape::Map,
            merge_metadata_overrides(<BatchConfig as Configurable>::metadata(), Some(batch_override_metadata)),
        )])
    }
}


#[derive(Serialize, Deserialize, Default, Clone)]
struct BatchConfig {
    max_events: u32,
    max_bytes: u32,
    max_timeout: Duration,
}

impl Configurable for BatchConfig {
    fn description() -> Option<&'static str> {
        Some("controls batching behavior i.e. maximum batch size, the maximum time before a batch is flushed, etc")
    }

    fn shape() -> Shape {
        Shape::Map
    }

    fn metadata() -> Option<Vec<TypedMetadata<Self>>> {
        Some(vec![
            TypedMetadata::DefaultValue(BatchConfig {
                max_events: 1000,
                max_bytes: 1048576,
                max_timeout: Duration::from_secs(60),
            })
        ])
    }

    fn fields(overrides: Option<Vec<TypedMetadata<Self>>>) -> Option<Vec<Field>> {
        let base_metadata = <Self as Configurable>::metadata();
        let merged_metadata = merge_metadata_overrides(base_metadata, overrides);

        let max_events_override_metadata = [
            try_derive_field_default_from_self(&merged_metadata, |default: &Self| {
                default.max_events
            }),
        ]
        .into_iter()
        .flatten()
        .collect();

        let max_bytes_override_metadata = [
            try_derive_field_default_from_self(&merged_metadata, |default: &Self| {
                default.max_bytes
            }),
        ]
        .into_iter()
        .flatten()
        .collect();

        let max_timeout_override_metadata = [
            try_derive_field_default_from_self(&merged_metadata, |default: &Self| {
                default.max_timeout
            }),
        ]
        .into_iter()
        .flatten()
        .collect();

        Some(vec![Field::new::<u32>(
            "max_events",
            "maximum number of events per batch",
            Shape::Number,
            merge_metadata_overrides(<u32 as Configurable>::metadata(), Some(max_events_override_metadata)),
        ),
        Field::new::<u32>(
            "max_bytes",
            "maximum number of bytes per batch",
            Shape::Number,
            merge_metadata_overrides(<u32 as Configurable>::metadata(), Some(max_bytes_override_metadata)),
        ),
        Field::new::<Duration>(
            "max_timeout",
            "maximum period of time a batch can exist before being forcibly flushed",
            Shape::Number,
            merge_metadata_overrides(<Duration as Configurable>::metadata(), Some(max_timeout_override_metadata)),
        )])
    }
}

impl Configurable for u32 {
    fn description() -> Option<&'static str> { None }

    fn shape() -> Shape {
        Shape::Number
    }

    fn metadata() -> Option<Vec<TypedMetadata<Self>>> {
        Some(vec![
            TypedMetadata::Bounded(Bounded::Unsigned {
                theoretical_upper_bound: u32::MAX.into(),
                effective_lower_bound: u32::MIN.into(),
                effective_upper_bound: u32::MAX.into(),
            })
        ])
    }

    fn fields(overrides: Option<Vec<TypedMetadata<Self>>>) -> Option<Vec<Field>> {
        None
    }
}

impl Configurable for String {
    fn description() -> Option<&'static str> { None }

    fn shape() -> Shape {
        Shape::String
    }

    fn metadata() -> Option<Vec<TypedMetadata<Self>>> {
        None
    }

    fn fields(overrides: Option<Vec<TypedMetadata<Self>>>) -> Option<Vec<Field>> {
        None
    }
}

impl Configurable for Duration {
    fn description() -> Option<&'static str> { None }

    fn shape() -> Shape {
        Shape::Number
    }

    fn metadata() -> Option<Vec<TypedMetadata<Self>>> {
        Some(vec![
            // Comment about imaginary serde impl that deals with raw Duration by only dealing with
            // the whole number of nanoseconds, etc.
            TypedMetadata::Bounded(Bounded::Unsigned {
                theoretical_upper_bound: u64::MAX.into(),
                effective_lower_bound: u64::MIN.into(),
                effective_upper_bound: u64::MAX.into(),
            })
        ])
    }

    fn fields(overrides: Option<Vec<TypedMetadata<Self>>>) -> Option<Vec<Field>> {
        None
    }
}

fn merge_metadata_overrides<T: Configurable + Clone>(base: Option<Vec<TypedMetadata<T>>>, overrides: Option<Vec<TypedMetadata<T>>>) -> Option<Vec<TypedMetadata<T>>> {
    None
}

fn try_derive_field_default_from_self<T, F, U>(metadata: &Option<Vec<TypedMetadata<T>>>, f: F) -> Option<TypedMetadata<U>>
where
    T: Configurable + Clone,
    F: Fn(&T) -> U,
    U: Configurable + Clone,
{
    metadata.as_ref()
        .and_then(|metadata| {
            for entry in metadata {
                if let TypedMetadata::DefaultValue(default) = entry {
                    let field_default = f(default);
                    return Some(TypedMetadata::DefaultValue(field_default))
                }
            }

            None
        })
}