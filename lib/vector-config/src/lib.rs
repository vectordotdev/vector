use std::collections::HashMap;

use schemars::schema::{StringValidation, NumberValidation};
use serde::{Serialize, Deserialize};
use serde_json::Value;

mod schema;
mod stdlib;

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
    minimum_length: Option<usize>,
    maximum_length: Option<usize>,
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
    Unsigned {
        minimum: u128,
        maximum: u128,
    },
    Signed {
        minimum: i128,
        maximum: i128,
    },
    FloatingPoint {
        minimum: f64,
        maximum: f64,
    }
}

impl From<NumberShape> for NumberValidation {
    fn from(s: NumberShape) -> Self {
        let (minimum, maximum) = match s {
            NumberShape::Unsigned {
                minimum,
                maximum,
            } => (minimum.try_into().unwrap_or(f64::MAX), maximum.try_into().unwrap_or(f64::MAX)),
            NumberShape::Signed {
                minimum,
                maximum,
            } => (minimum.try_into().unwrap_or(f64::MAX), maximum.try_into().unwrap_or(f64::MAX)),
            NumberShape::FloatingPoint {
                minimum,
                maximum,
            } => (minimum, maximum)
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
    minimum_length: Option<usize>,
    maximum_length: Option<usize>,
}

#[derive(Clone)]
pub struct MapShape {
    required_fields: HashMap<&'static str, Shape>,
    allowed_unknown_field_shape: Option<Box<Shape>>,
}

pub struct Field {
    name: &'static str,
    description: &'static str,
    shape: Shape,
    fields: Vec<Field>,
    metadata: Metadata<Value>,
}

#[derive(Clone)]
pub struct Metadata<T: Serialize> {
    default: Option<T>,
    attributes: Vec<(String, String)>,
}

impl<T: Serialize> Metadata<T> {
    fn into_opaque(self) -> Metadata<Value> {
        Metadata {
            default: self.default
                .map(|v| serde_json::to_value(v).expect("default value should never fail to serialize")),
            attributes: self.attributes,
        }
    }
}

impl<T: Serialize> Default for Metadata<T> {
    fn default() -> Self {
        Self {
            default: None,
            attributes: Vec::new()
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
    /// reused within any generated schema it is present in.  It simultaneously specifies
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

    /// Gets the shape of this value.
    fn shape() -> Shape;

    /// Gets the metadata for this value.
    fn metadata() -> Metadata<Self> {
        Metadata::default()
    }

    /// The fields for this value, if any.
    fn fields(overrides: Metadata<Value>) -> Option<HashMap<&'static str, Field>> {
        None
    }
}
