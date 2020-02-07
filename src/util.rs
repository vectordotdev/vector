// converts an iterator over key-value pairs into an iterator with values sorted by keys
// has O(n) space and O(n log(n)) time complexity
pub fn sort_kv_iter<K, V, I>(iter: I) -> impl Iterator<Item = (K, V)>
where
    K: Ord,
    I: Iterator<Item = (K, V)>,
{
    let mut collected: Vec<_> = iter.collect();
    collected.sort_by(|(k1, _), (k2, _)| k1.cmp(&k2));
    collected.into_iter()
}
