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

mod expression;
mod r#macro;
mod target;

pub mod conversion;
mod datetime;
pub mod encode_key_value;
pub mod encode_logfmt;
pub mod tokenize;

pub use datetime::TimeZone;
pub use expression::{ExpressionError, Resolved};
pub use target::{SecretTarget, Target, TargetValue, TargetValueRef};
pub use value::Value;
