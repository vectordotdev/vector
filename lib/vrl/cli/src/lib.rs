pub mod cmd;
#[cfg(feature = "repl")]
mod repl;

pub use cmd::{cmd, Opts};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error: {}", .0)]
    Io(#[from] std::io::Error),

    #[error("{}",.0)]
    Parse(String),

    #[error(transparent)]
    Runtime(#[from] vrl::Terminate),

    #[error("input error: {}", .0)]
    Json(#[from] serde_json::Error),

    #[error("repl feature disabled, program input required")]
    ReplFeature,
}
