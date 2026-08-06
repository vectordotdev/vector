use vector_common::constants::ZSTD_MAGIC;

pub fn is_zstd(payload: &[u8]) -> bool {
    payload.len() >= 4 && payload.starts_with(ZSTD_MAGIC)
}
