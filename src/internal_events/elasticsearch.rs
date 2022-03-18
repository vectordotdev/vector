use super::prelude::{error_stage, error_type};
use http::Response;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ElasticsearchEventEncoded {
    pub byte_size: usize,
    pub index: String,
}

impl InternalEvent for ElasticsearchEventEncoded {
    fn emit_logs(&self) {
        trace!(message = "Inserting event.", index = %self.index);
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct ElasticsearchResponseError<'a> {
    response: &'a Response<bytes::Bytes>,
    message: &'static str,
    error_code: String,
}

#[cfg(feature = "sinks-elasticsearch")]
impl<'a> ElasticsearchResponseError<'a> {
    pub fn new(message: &'static str, response: &'a Response<bytes::Bytes>) -> Self {
        let error_code = super::prelude::http_error_code(response.status().as_u16());
        Self {
            message,
            response,
            error_code,
        }
    }
}

impl<'a> InternalEvent for ElasticsearchResponseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = %self.message,
            error_code = %self.error_code,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            response = ?self.response,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code.clone(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}
