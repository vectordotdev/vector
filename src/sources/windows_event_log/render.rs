//! Event rendering and channel statistics helpers for Windows Event Log.
//!
//! Extracted from `subscription.rs` to keep that module focused on
//! subscription lifecycle and event pulling.

use metrics::Gauge;
use windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EVT_LOG_PROPERTY_ID, EvtClose, EvtGetLogInfo, EvtLogNumberOfLogRecords, EvtOpenLog,
    EvtRender, EvtRenderEventXml,
};
use windows::core::HSTRING;

use super::error::WindowsEventLogError;

/// Render an event handle to XML using reusable buffers.
pub(super) fn render_event_xml(
    render_buffer: &mut Vec<u8>,
    decode_buffer: &mut Vec<u16>,
    event_handle: EVT_HANDLE,
) -> Result<String, WindowsEventLogError> {
    const MAX_BUFFER_SIZE: u32 = 10 * 1024 * 1024; // 10MB limit

    let buffer_size = render_buffer.len() as u32;
    let mut buffer_used = 0u32;
    let mut property_count = 0u32;

    let result = unsafe {
        EvtRender(
            None,
            event_handle,
            EvtRenderEventXml.0,
            buffer_size,
            Some(render_buffer.as_mut_ptr() as *mut std::ffi::c_void),
            &mut buffer_used,
            &mut property_count,
        )
    };

    if let Err(e) = result {
        if e.code() == ERROR_INSUFFICIENT_BUFFER.into() {
            if buffer_used == 0 {
                return Ok(String::new());
            }
            if buffer_used > MAX_BUFFER_SIZE {
                return Err(WindowsEventLogError::ReadEventError { source: e });
            }

            // Grow the reusable buffer
            render_buffer.resize(buffer_used as usize, 0);
            let mut second_buffer_used = 0u32;
            let mut second_property_count = 0u32;

            unsafe {
                EvtRender(
                    None,
                    event_handle,
                    EvtRenderEventXml.0,
                    buffer_used,
                    Some(render_buffer.as_mut_ptr() as *mut std::ffi::c_void),
                    &mut second_buffer_used,
                    &mut second_property_count,
                )
            }
            .map_err(|e2| WindowsEventLogError::ReadEventError { source: e2 })?;

            let result = decode_utf16_buffer(render_buffer, second_buffer_used, decode_buffer);

            // Shrink if buffer grew very large (match normal-path threshold)
            const SHRINK_THRESHOLD: usize = 64 * 1024;
            if render_buffer.len() > SHRINK_THRESHOLD {
                render_buffer.resize(SHRINK_THRESHOLD, 0);
                render_buffer.shrink_to_fit();
            }

            return Ok(result);
        }
        return Err(WindowsEventLogError::ReadEventError { source: e });
    }

    let result = decode_utf16_buffer(render_buffer, buffer_used, decode_buffer);

    // Shrink the buffer back down if a large event caused it to grow.
    // 64 KB covers the vast majority of events without repeated reallocation.
    const SHRINK_THRESHOLD: usize = 64 * 1024;
    if render_buffer.len() > SHRINK_THRESHOLD {
        render_buffer.resize(SHRINK_THRESHOLD, 0);
        render_buffer.shrink_to_fit();
    }

    Ok(result)
}

/// Update the channel record count gauge using EvtGetLogInfo.
///
/// Reports total records in the channel. SOC teams compare this against
/// `rate(events_read_total)` to detect ingestion lag.
/// Best-effort: if any API call fails, the gauge is left unchanged.
pub(super) fn update_channel_records(channel: &str, gauge: &Gauge) {
    let channel_hstring = HSTRING::from(channel);
    let log_handle = unsafe {
        // EvtOpenChannelPath = 1
        match EvtOpenLog(None, &channel_hstring, 1) {
            Ok(h) => h,
            Err(_) => return,
        }
    };

    // EVT_VARIANT is 16 bytes: 8 bytes value + 4 bytes count + 4 bytes type
    let mut buffer = [0u8; 16];
    let mut buffer_used = 0u32;

    let result = unsafe {
        EvtGetLogInfo(
            log_handle,
            EVT_LOG_PROPERTY_ID(EvtLogNumberOfLogRecords.0),
            buffer.len() as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut buffer_used,
        )
    };

    unsafe {
        let _ = EvtClose(log_handle);
    }

    if result.is_ok() {
        // EVT_VARIANT for UInt64: first 8 bytes are the value (little-endian)
        let record_count = u64::from_le_bytes(buffer[..8].try_into().unwrap_or([0; 8]));
        gauge.set(record_count as f64);
    }
}

/// Decode a UTF-16LE buffer (as returned by Windows EvtRender) into a String.
///
/// Uses a reusable `Vec<u16>` decode buffer to avoid per-event heap allocations.
/// Copies byte pairs into the properly-aligned buffer instead of casting the
/// pointer, which would be undefined behavior when the source buffer is not
/// 2-byte aligned.
fn decode_utf16_buffer(buffer: &[u8], bytes_used: u32, decode_buf: &mut Vec<u16>) -> String {
    if bytes_used == 0 || bytes_used as usize > buffer.len() {
        return String::new();
    }
    if bytes_used < 2 || bytes_used % 2 != 0 {
        return String::new();
    }

    let u16_len = bytes_used as usize / 2;
    decode_buf.resize(u16_len, 0);
    for i in 0..u16_len {
        decode_buf[i] = u16::from_le_bytes([buffer[i * 2], buffer[i * 2 + 1]]);
    }

    // Strip trailing null terminator
    let xml_len = if !decode_buf.is_empty() && decode_buf[u16_len - 1] == 0 {
        u16_len - 1
    } else {
        u16_len
    };

    if xml_len == 0 {
        return String::new();
    }

    String::from_utf16_lossy(&decode_buf[..xml_len])
}
