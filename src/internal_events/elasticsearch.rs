use http::Response;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct BadElasticsearchResponse<'a> {
    response: &'a Response<bytes::Bytes>,
    message: &'static str,
    error_code: String,
}

#[cfg(feature = "sinks-elasticsearch")]
impl<'a> BadElasticsearchResponse<'a> {
    pub fn new(message: &'static str, response: &'a Response<bytes::Bytes>) -> Self {
        let error_code = super::prelude::http_error_code(response.status().as_u16());
        Self {
            message,
            response,
            error_code,
        }
    }
}

impl<'a> InternalEvent for BadElasticsearchResponse<'a> {
    fn emit(self) {
        // Emission of Error internal event and metrics is handled upstream by the caller.
        error!(
            message = %self.message,
            error_code = %self.error_code,
            response = ?self.response,
        );
    }
}
