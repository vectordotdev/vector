pub mod cmd;
#[cfg(feature = "repl")]
mod repl;
#[cfg(feature = "tutorial")]
mod tutorial;

pub use cmd::{cmd, Opts};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("parse error")]
    Parse(String),

    #[error("runtime error")]
    Runtime(String),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[cfg(not(feature = "repl"))]
    #[error("repl feature disabled, program input required")]
    ReplFeature,
}

#[macro_export]
macro_rules! array {
    () => ({
        let vec: Vec<remap::Value> = ::std::vec::Vec::new();
        remap::Value::from(vec)
    });
    ($($v:expr),+ $(,)?) => ({
        let vec: Vec<remap::Value> = vec![$($v.into()),+];
        remap::Value::from(vec)
    })
}
