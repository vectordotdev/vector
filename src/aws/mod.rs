pub mod auth;
pub mod region;

#[cfg(feature = "rusoto_core")]
pub mod rusoto;

#[cfg(feature = "aws-config")]
pub mod aws_sdk;
