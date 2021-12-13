#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![deny(unused_allocation)]
#![deny(unused_extern_crates)]
#![deny(unused_assignments)]
#![deny(unused_comparisons)]

mod expression;
mod r#macro;
mod target;

pub use expression::{ExpressionError, Resolved};
pub use target::Target;
pub use value::Value;

use vector_common::TimeZone;

pub struct Context<'a> {
    pub target: &'a mut dyn Target,
    pub timezone: &'a TimeZone,
}
