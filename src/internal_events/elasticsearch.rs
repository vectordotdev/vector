use super::prelude::{error_stage, error_type, http_error_code};
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
    pub response: &'a Response<bytes::Bytes>,
    pub message: &'static str,
}

impl<'a> ElasticsearchResponseError<'a> {
    fn error_code(&self) -> String {
        http_error_code(self.response.status().as_u16())
    }
}

impl<'a> InternalEvent for ElasticsearchResponseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = %self.message,
            error_code = %self.error_code(),
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::SENDING,
            response = ?self.response,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code(),
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::SENDING,
        );
    }
}
