pub mod auth;
pub mod region;

pub use auth::AwsAuthentication;
pub use region::RegionOrEndpoint;

#[cfg(feature = "rusoto_core")]
pub mod rusoto;

#[cfg(feature = "aws-core")]
pub mod aws_sdk;
