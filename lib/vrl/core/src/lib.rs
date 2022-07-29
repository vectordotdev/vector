#![deny(warnings)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

mod arithmetic;
mod convert;
mod error;
mod expression;
mod r#macro;
mod target;

pub use arithmetic::VrlValueArithmetic;
pub use convert::VrlValueConvert;
pub use diagnostic::{Label, Span};
pub use error::Error;
pub use expression::{ExpressionError, Resolved};
pub use lookup::LookupBuf;
pub use target::{MetadataTarget, SecretTarget, Target, TargetValue, TargetValueRef};
pub use value::{kind, Kind, Value, ValueRegex};

pub struct Context<'a> {
    pub target: &'a mut dyn Target,
    pub timezone: &'a vector_common::TimeZone,
}
