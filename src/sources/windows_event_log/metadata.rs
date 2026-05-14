use std::collections::HashMap;
use std::num::NonZeroUsize;

use lru::LruCache;
use metrics::Counter;
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EvtFormatMessage, EvtFormatMessageEvent, EvtFormatMessageKeyword,
    EvtFormatMessageOpcode, EvtFormatMessageTask, EvtOpenPublisherMetadata,
};
use windows::core::HSTRING;

use super::subscription::{FORMAT_CACHE_CAPACITY, PublisherHandle};

/// Resolves task, opcode, and keyword names from provider metadata via EvtFormatMessage.
pub fn resolve_event_metadata(
    publisher_cache: &mut LruCache<String, PublisherHandle>,
    format_cache: &mut HashMap<String, LruCache<(u32, u64), Option<String>>>,
    cache_hits_counter: &Counter,
    cache_misses_counter: &Counter,
    event_handle: EVT_HANDLE,
    provider_name: &str,
    task: u64,
    opcode: u64,
    keywords: u64,
) -> (Option<String>, Option<String>, Vec<String>) {
    let raw_handle = get_or_open_publisher(publisher_cache, provider_name);

    if raw_handle == 0 {
        return (None, None, Vec::new());
    }

    let metadata_handle = EVT_HANDLE(raw_handle);

    let task_flag = EvtFormatMessageTask.0 as u32;
    let opcode_flag = EvtFormatMessageOpcode.0 as u32;
    let keyword_flag = EvtFormatMessageKeyword.0 as u32;

    let task_name = cached_format(
        format_cache,
        cache_hits_counter,
        cache_misses_counter,
        metadata_handle,
        event_handle,
        provider_name,
        task_flag,
        task,
    );
    let opcode_name = cached_format(
        format_cache,
        cache_hits_counter,
        cache_misses_counter,
        metadata_handle,
        event_handle,
        provider_name,
        opcode_flag,
        opcode,
    );
    let keyword_str = cached_format(
        format_cache,
        cache_hits_counter,
        cache_misses_counter,
        metadata_handle,
        event_handle,
        provider_name,
        keyword_flag,
        keywords,
    );

    let keyword_names = keyword_str
        .map(|s| {
            s.split(';')
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();

    (task_name, opcode_name, keyword_names)
}

fn get_or_open_publisher(
    cache: &mut LruCache<String, PublisherHandle>,
    provider_name: &str,
) -> isize {
    if let Some(handle) = cache.get(provider_name) {
        return handle.0;
    }

    let provider_hstring = HSTRING::from(provider_name);
    let raw = unsafe {
        EvtOpenPublisherMetadata(None, &provider_hstring, None, 0, 0)
            .map(|h| h.0)
            .unwrap_or(0)
    };

    cache.put(provider_name.to_string(), PublisherHandle(raw));
    raw
}

/// Two-level cache lookup: outer HashMap keyed by `&str` (zero allocation),
/// inner LRU keyed by `(flag, field_value)`.
fn cached_format(
    cache: &mut HashMap<String, LruCache<(u32, u64), Option<String>>>,
    cache_hits_counter: &Counter,
    cache_misses_counter: &Counter,
    metadata_handle: EVT_HANDLE,
    event_handle: EVT_HANDLE,
    provider: &str,
    flag: u32,
    field_value: u64,
) -> Option<String> {
    let inner_key = (flag, field_value);

    // Fast path: borrowed &str lookup on outer HashMap — zero allocation.
    // peek() intentionally skips LRU promotion — get() requires &mut which
    // would need get_mut() on the outer HashMap. The put() on every miss
    // already handles insertion/promotion, so peek is correct here.
    if let Some(inner) = cache.get(provider) {
        if let Some(cached) = inner.peek(&inner_key) {
            cache_hits_counter.increment(1);
            return cached.clone();
        }
    }

    // Slow path: call API and populate cache
    cache_misses_counter.increment(1);
    let result = format_metadata_field(metadata_handle, event_handle, flag);
    let inner = cache
        .entry(provider.to_string())
        .or_insert_with(|| LruCache::new(NonZeroUsize::new(FORMAT_CACHE_CAPACITY).unwrap()));
    inner.put(inner_key, result.clone());
    result
}

fn format_metadata_field(
    metadata_handle: EVT_HANDLE,
    event_handle: EVT_HANDLE,
    flags: u32,
) -> Option<String> {
    let mut buffer_used: u32 = 0;
    let _ = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            flags,
            None,
            &mut buffer_used,
        )
    };

    if buffer_used == 0 || buffer_used > 4096 {
        return None;
    }

    let mut buffer = vec![0u16; buffer_used as usize];
    let mut actual_used: u32 = 0;
    let result = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            flags,
            Some(&mut buffer),
            &mut actual_used,
        )
    };

    if result.is_err() {
        return None;
    }

    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let s = String::from_utf16_lossy(&buffer[..len]);
    if s.is_empty() { None } else { Some(s) }
}

/// Renders a human-readable event message using the Windows EvtFormatMessage API.
pub fn format_event_message(
    publisher_cache: &mut LruCache<String, PublisherHandle>,
    event_handle: EVT_HANDLE,
    provider_name: &str,
) -> Option<String> {
    let raw_handle = get_or_open_publisher(publisher_cache, provider_name);

    if raw_handle == 0 {
        return None;
    }

    let metadata_handle = EVT_HANDLE(raw_handle);
    let flags = EvtFormatMessageEvent.0 as u32;
    let max_size = 64 * 1024;

    let mut buffer_used: u32 = 0;
    let _ = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            flags,
            None,
            &mut buffer_used,
        )
    };

    if buffer_used == 0 || buffer_used as usize > max_size {
        return None;
    }

    let mut buffer = vec![0u16; buffer_used as usize];
    let mut actual_used: u32 = 0;
    let result = unsafe {
        EvtFormatMessage(
            metadata_handle,
            event_handle,
            0,
            None,
            flags,
            Some(&mut buffer),
            &mut actual_used,
        )
    };

    if result.is_err() {
        return None;
    }

    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let s = String::from_utf16_lossy(&buffer[..len]);
    if s.is_empty() { None } else { Some(s) }
}
