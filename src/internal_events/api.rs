use std::net::SocketAddr;

use vector_common::counter;
use vector_lib::NamedInternalEvent;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug, NamedInternalEvent)]
pub struct ApiStarted {
    pub addr: SocketAddr,
}

impl InternalEvent for ApiStarted {
    fn emit(self) {
        info!(
            message = "API server running.",
            address = %self.addr,
        );
        counter!(MetricName::ApiStartedTotal).increment(1);
    }
}
