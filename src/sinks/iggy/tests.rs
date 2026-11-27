use std::sync::Arc;

use iggy::prelude::IggyError as Sdk;

use super::{IggyError, config::IggySinkConfig, sink::IggyRetryLogic};
use crate::common::iggy::retained_operation;
use crate::sinks::util::retries::RetryLogic;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<IggySinkConfig>();
}

#[test]
fn transient_http_failures_are_retriable() {
    let logic = IggyRetryLogic;
    let retriable = |source| logic.is_retriable_error(&IggyError::Producer { source });

    assert!(retriable(Sdk::InvalidHttpRequest));
    for status in [408, 429, 500, 503, 599] {
        assert!(
            retriable(Sdk::HttpResponseError(status, String::new())),
            "status {status} must be retried"
        );
    }
    assert!(!retriable(Sdk::HttpResponseError(404, String::new())));
}

#[test]
fn send_failure_names_the_retained_operation() {
    let blocked = Sdk::ProducerSendFailed {
        cause: Box::new(Sdk::LaneBlocked(7)),
        failed: Arc::new(Vec::new()),
        uncertain: 0,
        committed: Arc::new(Vec::new()),
        stream_name: "stream".into(),
        topic_name: "topic".into(),
    };
    assert_eq!(retained_operation(&blocked), Some(7));
    assert_eq!(retained_operation(&Sdk::NotConnected), None);
    assert_eq!(retained_operation(&Sdk::OperationUncertain(7)), Some(7));
}
