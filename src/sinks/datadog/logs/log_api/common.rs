use std::hash::Hasher;
use twox_hash::XxHash64;

#[inline]
pub(crate) fn hash(input: &str) -> u64 {
    let mut hasher = XxHash64::default();
    hasher.write(input.as_bytes());
    hasher.finish()
}
