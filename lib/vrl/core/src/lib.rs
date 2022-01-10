pub mod diagnostic;
mod target;
mod value;

pub use ordered_float::NotNan;
pub use target::Target;
pub use value::{kind, Error, Kind, Regex, Value};

pub type Resolved = Result<Value, diagnostic::ExpressionError>;
