// TODO: In order to correctly assert component specification compliance, we would have to do some more advanced mocking
// off the endpoint, which would include also providing a mock OAuth2 endpoint to allow for generating a token from the
// mocked credentials. Let this TODO serve as a placeholder for doing that in the future.

use crate::sinks::prelude::*;

mod config;
mod request_builder;
mod sink;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default)]
pub struct StackdriverMetricsDefaultBatchSettings;

impl SinkBatchSettings for StackdriverMetricsDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}
