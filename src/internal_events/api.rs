use super::InternalEvent;
use metrics::counter;
use std::net::SocketAddr;

#[derive(Debug)]
pub struct ApiStarted {
    pub addr: SocketAddr,
    pub playground: bool,
}

impl InternalEvent for ApiStarted {
    fn emit_logs(&self) {
        let playground = &*format!("http://{}:{}/playground", self.addr.ip(), self.addr.port());
        info!(
            message="API server running.",
            bind = ?self.addr,
            playground = %if self.playground { playground } else { "off" }
        );
    }

    fn emit_metrics(&self) {
        counter!("api_started_total", 1);
    }
}
