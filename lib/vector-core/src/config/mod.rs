mod global_options;
mod id;
mod log_schema;
pub mod proxy;

pub use global_options::GlobalOptions;
pub use id::ComponentKey;
pub use log_schema::{init_log_schema, log_schema, LogSchema};

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub port: Option<String>,
    pub ty: DataType,
}

impl Output {
    pub fn default(ty: DataType) -> Self {
        Self { port: None, ty }
    }
}

impl<T: Into<String>> From<(T, DataType)> for Output {
    fn from((name, ty): (T, DataType)) -> Self {
        Self {
            port: Some(name.into()),
            ty,
        }
    }
}
