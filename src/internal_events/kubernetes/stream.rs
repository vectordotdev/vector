use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ChunkProcessed {
    pub byte_size: usize,
}

impl InternalEvent for ChunkProcessed {
    fn emit_metrics(&self) {
        counter!("k8s_stream_chunks_processed", 1);
        counter!("k8s_stream_bytes_processed", self.byte_size as u64);
    }
}
