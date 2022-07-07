#![deny(
    warnings,
    clippy::all,
    clippy::pedantic,
    unreachable_pub,
    unused_allocation,
    unused_extern_crates,
    unused_assignments,
    unused_comparisons
)]
#![allow(
    clippy::missing_errors_doc, // allowed in initial deny commit
    clippy::module_name_repetitions, // allowed in initial deny commit
)]

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

use vector_common::TimeZone;

pub struct Context<'a> {
    pub target: &'a mut dyn Target,
    pub timezone: &'a TimeZone,
}
