pub(crate) mod auth;
pub(crate) mod region;

pub use auth::AwsAuthentication;
pub use region::RegionOrEndpoint;

#[cfg(feature = "rusoto_core")]
pub mod rusoto;

#[cfg(feature = "aws-config")]
pub mod aws_sdk;
