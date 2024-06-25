use metrics::counter;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct AwsS3RequestEvent<'a> {
    pub bytes_size: usize,
    pub events: usize,
    pub bucket: &'a str,
    pub s3_key: &'a str,
}

impl InternalEvent for AwsS3RequestEvent<'_> {
    fn emit(self) {
        debug!(
            message = "Sending events.",
            bytes = self.bytes_size,
            events_len = self.events,
            s3_key = self.s3_key,
            bucket = self.bucket,
        );
        counter!("aws_s3_requests_sent_total", 1,
            "bucket" => self.bucket.to_string());
    }
}
