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

pub use expression::{ExpressionError, Resolved};
pub use target::{MetadataTarget, SecretTarget, Target, TargetValue, TargetValueRef};
pub use value::Value;
