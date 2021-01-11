pub mod cmd;
#[cfg(feature = "repl")]
mod repl;

pub use cmd::{cmd, Opts};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("remap error: {0}")]
    Remap(#[from] remap::RemapError),

    #[error("json error")]
    Json(#[from] serde_json::Error),

    #[cfg(not(feature = "repl"))]
    #[error("repl feature disabled, program input required")]
    ReplFeature,
}
