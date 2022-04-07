use std::collections::HashMap;

use crate::{Configurable, NumberShape, Shape, StringShape, ArrayShape, MapShape};

// Null and boolean.
impl<'de, T> Configurable<'de> for Option<T>
where
	T: Configurable<'de>,
{
	fn shape() -> Shape {
		Shape::Composite(vec![Shape::Null, T::shape()])
	}
}

impl<'de> Configurable<'de> for bool {
	fn shape() -> Shape {
		Shape::Boolean
	}
}

// Strings.
impl<'de> Configurable<'de> for String {
	fn shape() -> Shape {
		Shape::String(StringShape::default())
	}
}

// Numbers.
macro_rules! impl_configuable_unsigned {
	($($ty:ty),+) => {
		$(
			impl<'de> Configurable<'de> for $ty {
				fn shape() -> Shape {
					Shape::Number(NumberShape::Unsigned {
						effective_lower_bound: u128::from(<$ty>::MIN),
						effective_upper_bound: u128::from(<$ty>::MAX),
					})
				}
			}
		)+
	};
}

macro_rules! impl_configuable_signed {
	($($ty:ty),+) => {
		$(
			impl<'de> Configurable<'de> for $ty {
				fn shape() -> Shape {
					Shape::Number(NumberShape::Signed {
						effective_lower_bound: i128::from(<$ty>::MIN),
						effective_upper_bound: i128::from(<$ty>::MAX),
					})
				}
			}
		)+
	};
}

impl_configuable_unsigned!(u8, u16, u32, u64, u128);
impl_configuable_signed!(i8, i16, i32, i64, i128);

impl<'de> Configurable<'de> for f64 {
	fn shape() -> Shape {
		Shape::Number(NumberShape::FloatingPoint {
			effective_lower_bound: f64::MIN,
			effective_upper_bound: f64::MAX
		})
	}
}

impl<'de> Configurable<'de> for f32 {
	fn shape() -> Shape {
		Shape::Number(NumberShape::FloatingPoint {
			effective_lower_bound: f64::from(f32::MIN),
			effective_upper_bound: f64::from(f32::MAX),
		})
	}
}

// Arrays and maps.
impl<'de, T> Configurable<'de> for Vec<T>
where
	T: Configurable<'de>,
{
	fn shape() -> Shape {
		Shape::Array(ArrayShape {
			element_shape: Box::new(T::shape()),
			minimum_length: None,
			maximum_length: None,
		})
	}
}

impl<'de, V> Configurable<'de> for HashMap<String, V>
where
	V: Configurable<'de>,
{
	fn shape() -> Shape {
		Shape::Map(MapShape {
			required_fields: HashMap::new(),
			allowed_unknown_field_shape: Some(Box::new(V::shape())),
		})
	}
}
