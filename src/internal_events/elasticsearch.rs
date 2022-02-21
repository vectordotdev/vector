use super::prelude::{error_stage, http_error_code};
use http::Response;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ElasticSearchEventEncoded {
    pub byte_size: usize,
    pub index: String,
}

impl InternalEvent for ElasticSearchEventEncoded {
    fn emit_logs(&self) {
        trace!(message = "Inserting event.", index = %self.index);
    }

    fn emit_metrics(&self) {
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct ElasticSearchResponseError<'a> {
    pub response: &'a Response<bytes::Bytes>,
    pub message: &'static str,
}

impl<'a> ElasticSearchResponseError<'a> {
    fn error_code(&self) -> String {
        http_error_code(self.response.status().as_u16())
    }
}

impl<'a> InternalEvent for ElasticSearchResponseError<'a> {
    fn emit_logs(&self) {
        error!(
            message = %self.message,
            error_code = %self.error_code(),
            error_type = "failed_request",
            stage = error_stage::SENDING,
            response = ?self.response,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_errors_total", 1,
            "error_code" => self.error_code(),
            "error_type" => "failed_request",
            "stage" => error_stage::SENDING,
        );
    }
}
