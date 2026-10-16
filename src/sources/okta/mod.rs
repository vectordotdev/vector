#![warn(clippy::pedantic)]
#[cfg(feature = "sources-okta")]
pub mod client;

#[cfg(test)]
mod tests;

pub use client::OktaConfig;
