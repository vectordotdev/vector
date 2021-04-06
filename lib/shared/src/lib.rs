#[cfg(feature = "aws_cloudwatch_logs_subscription")]
pub mod aws_cloudwatch_logs_subscription;

#[cfg(feature = "btreemap")]
pub mod btreemap;

#[cfg(feature = "conversion")]
pub mod conversion;

#[cfg(feature = "conversion")]
pub mod datetime;

#[cfg(feature = "conversion")]
pub use datetime::TimeZone;

#[cfg(feature = "tokenize")]
pub mod tokenize;

#[cfg(feature = "event")]
pub mod event;

#[cfg(feature = "event")]
pub mod lookup;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

pub type Result<T> = std::result::Result<T, Error>;

pub mod event_data_eq;
#[cfg(test)]
pub mod test;
pub use event_data_eq::EventDataEq;
pub use event::metadata::EventMetadata;
