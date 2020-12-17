#[cfg(feature = "conversion")]
pub mod conversion;

#[cfg(feature = "tokenize")]
pub mod tokenize;

#[cfg(feature = "event")]
pub mod lookup;
#[cfg(feature = "event")]
pub mod event;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T> = std::result::Result<T, Error>;
