use std::time::Duration;

use serde::{Serialize, Deserialize};
use serde_json::Value;

/// The shape of the field.
/// 
/// This maps similiar to the concept of JSON's data types, where types are generalized and have
/// generalized representations.  This allows us to provide general-but-relevant mappings to core
/// types, such as integers and strings and so on, while providing escape hatches for customized
/// types that may be encoded and decoded via "normal" types but otherwise have specific rules or
/// requirements.
pub enum Shape {
    Scalar(Scalar),
    List,
    Map,
    Custom,
}

/// A scalar, or single value.
/// 
/// Generally refers to anything that stands on its own: integer, string, boolean, and so on.
pub enum Scalar {
    Unsigned(UnsignedInteger),
    Duration,
}

pub struct UnsignedInteger {
	theoretical_upper_bound: u128,
	effective_lower_bound: u128,
	effective_upper_bound: u128,
}

pub enum Bounded {
    Unsigned(u128, u128),
}

pub enum Metadata {
    DefaultValue(Value),
    Bounded(Bounded),

}

pub struct Field {}

impl Field {
    fn with_description(
        name: &'static str,
        desc: &'static str,
        shape: Shape,
        metadata: Option<Metadata>,
        fields: Option<Vec<Field>>,
    ) -> Self {
        Self {}
    }
}

pub trait Configurable: Sized {
    /// Gets the human-readable description of this value, if any.
    ///
    /// For standard types, this will be `None`.  Commonly, custom types would implement this
    /// directly, while fields using standard types would provide a field-specific description that
    /// would be used instead of the default descrption.
    fn description(&self) -> Option<&'static str>;

    /// Gets the shape of this value.
    fn shape(&self) -> Shape;

    /// Gets the metadata for this value.
    fn metadata(&self) -> Option<Vec<Metadata>>;

	/// The fields for this value, if any.
    fn fields(&self) -> Option<Vec<Field>>;
}

struct SinkConfig {
    url: String,
    batch: BatchConfig,
}

#[derive(Serialize, Deserialize)]
struct BatchConfig {
    max_events: Option<u32>,
    max_bytes: Option<u32>,
    max_timeout: Option<Duration>,
}

impl Configurable for BatchConfig {
    fn description(&self) -> Option<&'static str> {
        Some("controls batching behavior i.e. maximum batch size, the maximum time before a batch is flushed, etc")
    }

    fn shape(&self) -> Shape {
        Shape::Map
    }

    fn metadata(&self) -> Option<Vec<Metadata>> {
        let default = BatchConfig {
            max_events: Some(1000),
            max_bytes: Some(1048576),
            max_timeout: Some(Duration::from_secs(60)),
        };
        let default = serde_json::to_value(default).expect("should not fail");

        Some(vec![
            Metadata::DefaultValue(default)
        ])
    }

    fn fields(&self) -> Option<Vec<Field>> {
        Some(vec![Field::with_description(
            "max_events",
            "maximum number of events per batch",
            Shape::Scalar(Scalar::Unsigned(UnsignedInteger {
                theoretical_upper_bound: u32::MAX.into(),
                effective_lower_bound: u32::MIN.into(),
                effective_upper_bound: u32::MAX.into(),
            })),
            None,
            None,
        ),
        Field::with_description(
            "max_bytes",
            "maximum number of bytes per batch",
            Shape::Scalar(Scalar::Unsigned(UnsignedInteger {
                theoretical_upper_bound: u32::MAX.into(),
                effective_lower_bound: u32::MIN.into(),
                effective_upper_bound: u32::MAX.into(),
            })),
            None,
            None,
        ),
        Field::with_description(
            "max_timeout",
            "maximum period of time a batch can exist before being forcibly flushed",
            Shape::Scalar(Scalar::Duration),
            None,
            None,
        )])
    }
}
