/// Set of `stage` tags to use when emiting error events.
pub(crate) mod error_stage {
    pub(crate) const RECEIVING: &str = "receiving";
    pub(crate) const PROCESSING: &str = "processing";
    pub(crate) const SENDING: &str = "sending";
}

/// Set of `error_type` tags to use when emiting error events.
pub(crate) mod error_type {
    // NOTE these constants are used in different subsets of our feature
    // permutations. We allow them to be unused to avoid cfg feature flag gore
    // here.

    /// When the event acknowledgment failed.
    #[allow(unused)]
    pub(crate) const ACKNOWLEDGMENT_FAILED: &str = "acknowledgment_failed";
    /// When the external command called by the component failed.
    #[allow(unused)]
    pub(crate) const COMMAND_FAILED: &str = "command_failed";
    /// When a condition for the event to be valid failed.
    /// This is used for example when a field is missing or should be a string.
    #[allow(unused)]
    pub(crate) const CONDITION_FAILED: &str = "condition_failed";
    /// When the component or the service on which it depends is not configured properly.
    #[allow(unused)]
    pub(crate) const CONFIGURATION_FAILED: &str = "configuration_failed";
    /// When the component failed to connect to an external service.
    #[allow(unused)]
    pub(crate) const CONNECTION_FAILED: &str = "connection_failed";
    /// When the component failed to convert a value.
    /// For example, when converting from string to float.
    #[allow(unused)]
    pub(crate) const CONVERSION_FAILED: &str = "conversion_failed";
    /// When the component failed to convert an event to a structure required
    /// by the external service the event should be sent to.
    #[allow(unused)]
    pub(crate) const ENCODER_FAILED: &str = "encoder_failed";
    /// When the received event has an unexpected metric.
    #[allow(unused)]
    pub(crate) const INVALID_METRIC: &str = "invalid_metric";
    /// When the component is unable to parse a message to build an event.
    #[allow(unused)]
    pub(crate) const PARSER_FAILED: &str = "parser_failed";
    /// When the component was unable to read from the source.
    #[allow(unused)]
    pub(crate) const READER_FAILED: &str = "reader_failed";
    /// When the component was unable to perform a request or the request failed.
    #[allow(unused)]
    pub(crate) const REQUEST_FAILED: &str = "request_failed";
    /// When the component was unable to build a template or interpolate it.
    #[allow(unused)]
    pub(crate) const TEMPLATE_FAILED: &str = "template_failed";
    /// When an execution took longer than expected and failed.
    #[allow(unused)]
    pub(crate) const TIMED_OUT: &str = "timed_out";
    /// When the component was unable to write some data.
    #[allow(unused)]
    pub(crate) const WRITER_FAILED: &str = "writer_failed";
}
