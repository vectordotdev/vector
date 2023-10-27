use metrics::counter;
use vector_lib::internal_event::InternalEvent;

pub struct AwsBytesSent {
    pub byte_size: usize,
    pub region: Option<aws_types::region::Region>,
}

impl InternalEvent for AwsBytesSent {
    fn emit(self) {
        let region = self
            .region
            .as_ref()
            .map_or(String::new(), |r| r.as_ref().to_string());
        trace!(
            message = "Bytes sent.",
            protocol = "https",
            byte_size = %self.byte_size,
            region = ?self.region,
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "https",
            "region" => region,
        );
    }
}
