use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct ChunkProcessed {
    pub byte_size: usize,
}

impl InternalEvent for ChunkProcessed {
    fn emit_metrics(&self) {
        counter!("vector_k8s_stream_chunks_processed_total", 1);
        counter!("vector_k8s_stream_processed_bytes_total", self.byte_size as u64);
    }
}
