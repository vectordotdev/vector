use std::sync::Arc;

pub fn tokenize_whitespace_count(
    content: &str,
    dst: &mut Vec<Arc<str>>,
    max_tokens: usize,
) -> usize {
    if content.is_empty() || max_tokens == 0 {
        return 0;
    }
    dst.clear();
    let bytes = content.as_bytes();
    let mut start = 0;
    let mut count = 1;
    for i in 0..bytes.len() {
        if bytes[i] != b' ' {
            continue;
        }
        dst.push(Arc::from(std::str::from_utf8(&bytes[start..i]).unwrap()));
        start = i + 1;
        if count >= max_tokens {
            return count + 1;
        }
        count += 1;
    }
    dst.push(Arc::from(std::str::from_utf8(&bytes[start..]).unwrap()));
    count
}

pub fn tokenize(
    content: &str,
    extra_delimiters: &[String],
    max_tokens: usize,
    dst: &mut Vec<Arc<str>>,
) {
    dst.clear();
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return;
    }

    // Fast path: no extra delimiters - use split_whitespace (zero allocation)
    if extra_delimiters.is_empty() {
        dst.reserve(16.min(max_tokens));
        for t in trimmed.split_whitespace().take(max_tokens) {
            dst.push(Arc::from(t));
        }
        return;
    }

    // Slow path: need delimiter replacement
    let mut s = trimmed.to_string();
    for d in extra_delimiters {
        if !d.is_empty() {
            s = s.replace(d, " ");
        }
    }
    for t in s.split(' ').filter(|t| !t.is_empty()).take(max_tokens) {
        dst.push(Arc::from(t));
    }
}

pub fn has_numbers(s: &str) -> bool {
    s.bytes().any(|b| b.is_ascii_digit())
}
