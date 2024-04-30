use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AzureBlobRequestEvent<'a> {
    pub bytes_size: usize,
    pub events: usize,
    pub container_name: &'a str,
    pub partition_key: &'a str,
}

impl InternalEvent for AzureBlobRequestEvent<'_> {
    fn emit(self) {
        debug!(
            message = "Sending events.",
            bytes = self.bytes_size,
            events_len = self.events,
            blob = self.partition_key,
            container = self.container_name,
        );
        counter!("azure_blob_requests_sent_total", 1,
            "container_name" => self.container_name.to_string());
    }
}
