use vector_lib::{
    NamedInternalEvent, counter,
    internal_event::{CounterName, InternalEvent, error_stage, error_type},
};

#[derive(Debug, NamedInternalEvent)]
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
            CounterName::ComponentErrorsTotal,
            "error_code" => self.error_code,
            "error_type" => error_type::READER_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Emitted when the `redis` channel source fails to open a pub/sub connection or to
/// subscribe to its channel. The source backs off and retries, so this is a transient
/// error rather than a fatal one.
#[derive(Debug, NamedInternalEvent)]
pub struct RedisConnectionError {
    error: redis::RedisError,
    error_code: String,
}

impl From<redis::RedisError> for RedisConnectionError {
    fn from(error: redis::RedisError) -> Self {
        let error_code = error.code().unwrap_or("UNKNOWN").to_string();
        Self { error, error_code }
    }
}

impl InternalEvent for RedisConnectionError {
    fn emit(self) {
        error!(
            message = "Failed to establish Redis pub/sub subscription; will reconnect.",
            error = %self.error,
            error_code = %self.error_code,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => self.error_code,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Emitted when an established `redis` channel pub/sub connection drops unexpectedly. The
/// source reconnects automatically, but this records the drop as a component error so that
/// metric-based alerts still fire even when the following reconnect succeeds immediately.
#[derive(Debug, NamedInternalEvent)]
pub struct RedisConnectionDroppedError;

impl InternalEvent for RedisConnectionDroppedError {
    fn emit(self) {
        error!(
            message = "Redis pub/sub connection dropped; will reconnect.",
            error = "connection closed by server",
            error_code = "connection_dropped",
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
        );
        counter!(
            CounterName::ComponentErrorsTotal,
            "error_code" => "connection_dropped",
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        )
        .increment(1);
    }
}

/// Emitted when the `redis` channel source (re)establishes its pub/sub subscription.
/// `reconnect` distinguishes the first successful connect from a recovery after a drop.
#[derive(Debug, NamedInternalEvent)]
pub struct RedisConnectionEstablished {
    pub reconnect: bool,
}

impl InternalEvent for RedisConnectionEstablished {
    fn emit(self) {
        if self.reconnect {
            info!(message = "Redis pub/sub connection re-established and resubscribed.");
        } else {
            debug!(message = "Redis pub/sub connection established.");
        }
        counter!(CounterName::ConnectionEstablishedTotal, "mode" => "redis").increment(1);
    }
}
