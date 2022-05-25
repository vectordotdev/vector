use metrics::counter;
use vector_common::internal_event::InternalEvent;

pub struct AwsBytesSent<'a> {
    pub byte_size: usize,
    pub region: Option<aws_types::region::Region>,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for AwsBytesSent<'a> {
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
            endpoint = %self.endpoint,
        );
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "https",
            "region" => region,
            "endpoint" => self.endpoint.to_string(),
        );
    }
}
