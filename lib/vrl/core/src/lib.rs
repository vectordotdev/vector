mod context;
pub mod diagnostic;
mod ident;
mod runtime;
mod target;
mod value;

pub use context::Context;
pub use ident::Ident;
pub use ordered_float::NotNan;
pub use runtime::Runtime;
pub use target::Target;
pub use value::{kind, Error, Kind, Regex, Value};

pub type Resolved = Result<Value, diagnostic::ExpressionError>;
