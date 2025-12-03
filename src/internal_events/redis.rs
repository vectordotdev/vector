use metrics::counter;
use vector_lib::internal_event::{InternalEvent, error_stage, error_type};

#[derive(Debug)]
pub struct RedisReceiveEventError {
    error: redis::RedisError,
    error_code: String,
}

impl From<redis::RedisError> for RedisReceiveEventError {
    fn from(error: redis::RedisError) -> Self {
        let error_code = error.code().unwrap_or("UNKNOWN").to_string();
        Self { error, error_code }
    }
}

impl InternalEvent for RedisReceiveEventError {
    fn emit(self) {
        error!(
            message = "Failed to read message.",
            error = %self.error,
            error_code = %self.error_code,
            error_type = error_type::READER_FAILED,
            stage = error_stage::SENDING,
        );
        counter!(
            "component_errors_total",
            "error_code" => self.error_code,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct RedisTransformLookupError {
    pub error: String,
}

impl InternalEvent for RedisTransformLookupError {
    fn emit(self) {
        error!(
            message = "Redis lookup failed.",
            error = %self.error,
            error_type = error_type::REQUEST_FAILED,
            stage = error_stage::PROCESSING,
        );
        counter!(
            "component_errors_total",
            "error_type" => error_type::REQUEST_FAILED,
            "stage" => error_stage::PROCESSING,
        )
        .increment(1);
    }
}

#[derive(Debug)]
pub struct RedisTransformLruCacheHit;

impl InternalEvent for RedisTransformLruCacheHit {
    fn emit(self) {
        counter!("component_cache_hits_total", "cache" => "redis_transform").increment(1);
    }
}

#[derive(Debug)]
pub struct RedisTransformLruCacheMiss;

impl InternalEvent for RedisTransformLruCacheMiss {
    fn emit(self) {
        counter!("component_cache_misses_total", "cache" => "redis_transform").increment(1);
    }
}
