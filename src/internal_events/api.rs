use std::net::SocketAddr;

use vector_lib::internal_event::{CounterName, InternalEvent};
use vector_lib::{NamedInternalEvent, counter};

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
        counter!(CounterName::ApiStartedTotal).increment(1);
    }
}
