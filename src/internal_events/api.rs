use std::net::SocketAddr;

use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ApiStarted {
    pub addr: SocketAddr,
    pub playground: bool,
    pub graphql: bool,
}

impl InternalEvent for ApiStarted {
    fn emit(self) {
        let playground = &*format!("http://{}:{}/playground", self.addr.ip(), self.addr.port());
        let graphql = &*format!("http://{}:{}/graphql", self.addr.ip(), self.addr.port());
        info!(
            message="API server running.",
            address = ?self.addr,
            playground = %if self.playground { playground } else { "off" },
            graphql = %if self.graphql { graphql } else { "off" }

        );
        counter!("api_started_total", 1);
    }
}
