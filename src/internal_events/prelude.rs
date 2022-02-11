pub mod error_stage {
    pub const RECEIVING: &str = "receiving";
    pub const PROCESSING: &str = "processing";
    pub const SENDING: &str = "sending";
}

pub mod error_type {
    pub const ACKNOWLEDGMENT_FAILED: &str = "acknowledgment_failed";
    pub const CONDITION_FAILED: &str = "condition_failed";
    pub const CONFIGURATION_FAILED: &str = "configuration_failed";
    pub const CONNECTION_FAILED: &str = "connection_failed";
    pub const CONVERSION_FAILED: &str = "conversion_failed";
    pub const ENCODER_FAILED: &str = "encoder_failed";
    pub const INVALID_METRIC: &str = "invalid_metric";
    pub const PARSER_FAILED: &str = "parser_failed";
    pub const READER_FAILED: &str = "reader_failed";
    pub const REQUEST_FAILED: &str = "request_failed";
    pub const TEMPLATE_FAILED: &str = "template_failed";
    pub const TIMED_OUT: &str = "timed_out";
}
