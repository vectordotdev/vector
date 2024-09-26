use metrics::{counter, gauge};
use vector_common::internal_event::InternalEvent;

#[derive(Debug)]
pub struct VrlCacheRead {
    pub cache: String,
    pub key: String,
}

impl InternalEvent for VrlCacheRead {
    fn emit(self) {
        counter!(
            "vrl_cache_reads_total",
            "cache" => self.cache,
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheRead")
    }
}

#[derive(Debug)]
pub struct VrlCacheInserted {
    pub cache: String,
    pub key: String,
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for VrlCacheInserted {
    fn emit(self) {
        counter!(
            "vrl_cache_insertions_total",
            "cache" => self.cache.clone(),
            "key" => self.key
        )
        .increment(1);
        gauge!(
            "vrl_cache_objects_count",
            "cache" => self.cache.clone()
        )
        .set(self.new_objects_count as f64);
        gauge!(
            "vrl_cache_byte_size",
            "cache" => self.cache
        )
        .set(self.new_byte_size as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheInserted")
    }
}

#[derive(Debug)]
pub struct VrlCacheDeleted {
    pub cache: String,
    pub key: String,
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for VrlCacheDeleted {
    fn emit(self) {
        counter!(
            "vrl_cache_deletions_total",
            "cache" => self.cache.clone(),
            "key" => self.key
        )
        .increment(1);
        gauge!(
            "vrl_cache_objects_count",
            "cache" => self.cache.clone()
        )
        .set(self.new_objects_count as f64);
        gauge!(
            "vrl_cache_byte_size",
            "cache" => self.cache
        )
        .set(self.new_byte_size as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheDeleted")
    }
}

#[derive(Debug)]
pub struct VrlCacheDeleteFailed {
    pub cache: String,
    pub key: String,
}

impl InternalEvent for VrlCacheDeleteFailed {
    fn emit(self) {
        counter!(
            "vrl_cache_failed_deletes",
            "cache" => self.cache,
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheDeleteFailed")
    }
}

#[derive(Debug)]
pub struct VrlCacheTtlExpired {
    pub cache: String,
    pub key: String,
    pub new_objects_count: usize,
    pub new_byte_size: usize,
}

impl InternalEvent for VrlCacheTtlExpired {
    fn emit(self) {
        counter!(
            "vrl_cache_ttl_expirations",
            "cache" => self.cache.clone(),
            "key" => self.key
        )
        .increment(1);
        gauge!(
            "vrl_cache_objects_count",
            "cache" => self.cache.clone()
        )
        .set(self.new_objects_count as f64);
        gauge!(
            "vrl_cache_byte_size",
            "cache" => self.cache
        )
        .set(self.new_byte_size as f64);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheTtlExpired")
    }
}

#[derive(Debug)]
pub struct VrlCacheReadFailed {
    pub cache: String,
    pub key: String,
}

impl InternalEvent for VrlCacheReadFailed {
    fn emit(self) {
        counter!(
            "vrl_cache_failed_reads",
            "cache" => self.cache,
            "key" => self.key
        )
        .increment(1);
    }

    fn name(&self) -> Option<&'static str> {
        Some("VrlCacheReadFailed")
    }
}
