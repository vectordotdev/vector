use std::net::SocketAddr;

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct ApiStarted {
    pub(crate) addr: SocketAddr,
    pub(crate) playground: bool,
}

impl InternalEvent for ApiStarted {
    fn emit_logs(&self) {
        let playground = &*format!("http://{}:{}/playground", self.addr.ip(), self.addr.port());
        info!(
            message="API server running.",
            address = ?self.addr,
            playground = %if self.playground { playground } else { "off" }
        );
    }

    fn emit_metrics(&self) {
        counter!("api_started_total", 1);
    }
}
