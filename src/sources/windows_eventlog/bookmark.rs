//! Windows Event Log Bookmark Management
//!
//! This module provides bookmark-based checkpointing for Windows Event Log subscriptions.
//! Bookmarks are more robust than record IDs because they:
//! - Survive channel clears and log rotations
//! - Provide O(1) seeking instead of O(n)
//! - Are managed by Windows, eliminating edge cases
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Create a bookmark from checkpoint data
//! let bookmark = BookmarkManager::from_xml(checkpoint_xml)?;
//!
//! // Update bookmark after processing an event
//! bookmark.update(event_handle)?;
//!
//! // Serialize bookmark for checkpointing
//! let xml = bookmark.to_xml()?;
//! ```

use tracing::{debug, error, warn};
use windows::{
    Win32::System::EventLog::{
        EVT_HANDLE, EvtCreateBookmark, EvtRender, EvtRenderBookmark, EvtUpdateBookmark,
    },
    core::HSTRING,
};

use super::error::WindowsEventLogError;

/// Maximum size for rendered bookmark XML (1 MB should be more than enough)
const MAX_BOOKMARK_XML_SIZE: usize = 1024 * 1024;

/// Manages a Windows Event Log bookmark for checkpoint tracking
///
/// Bookmarks provide robust, Windows-managed position tracking in event logs.
/// They are opaque handles that can be serialized to XML for persistence.
#[derive(Debug)]
pub struct BookmarkManager {
    #[cfg(windows)]
    handle: EVT_HANDLE,
    #[cfg(not(windows))]
    handle: isize, // Stub for cross-platform compilation
}

impl BookmarkManager {
    /// Creates a new bookmark (not associated with any event yet)
    ///
    /// # Errors
    ///
    /// Returns an error if the Windows API fails to create the bookmark.
    #[cfg(windows)]
    pub fn new() -> Result<Self, WindowsEventLogError> {
        unsafe {
            let handle = EvtCreateBookmark(None).map_err(|e| {
                error!(message = "Failed to create bookmark", error = %e);
                WindowsEventLogError::CreateSubscriptionError { source: e }
            })?;

            debug!(message = "Created new bookmark", handle = ?handle);

            Ok(Self { handle })
        }
    }

    /// Creates a bookmark from serialized XML
    ///
    /// This is used when resuming from a checkpoint.
    ///
    /// # Arguments
    ///
    /// * `xml` - The XML string representation of a bookmark
    ///
    /// # Errors
    ///
    /// Returns an error if the XML is invalid or the Windows API fails.
    #[cfg(windows)]
    pub fn from_xml(xml: &str) -> Result<Self, WindowsEventLogError> {
        if xml.is_empty() {
            return Self::new(); // Empty XML = fresh bookmark
        }

        unsafe {
            let xml_hstring = HSTRING::from(xml);
            match EvtCreateBookmark(&xml_hstring) {
                Ok(handle) => {
                    debug!(message = "Created bookmark from XML", handle = ?handle);
                    Ok(Self { handle })
                }
                Err(e) => {
                    warn!(
                        message = "Corrupted bookmark XML detected, creating fresh bookmark to recover gracefully",
                        error = %e
                    );
                    // Gracefully degrade - start fresh rather than failing the entire subscription
                    Self::new()
                }
            }
        }
    }

    /// Updates the bookmark to point to the given event
    ///
    /// Call this after successfully processing an event to update the checkpoint position.
    ///
    /// # Arguments
    ///
    /// * `event_handle` - Handle to the event to bookmark
    ///
    /// # Errors
    ///
    /// Returns an error if the Windows API fails to update the bookmark.
    #[cfg(windows)]
    pub fn update(&mut self, event_handle: EVT_HANDLE) -> Result<(), WindowsEventLogError> {
        unsafe {
            EvtUpdateBookmark(self.handle, event_handle).map_err(|e| {
                error!(message = "Failed to update bookmark", error = %e);
                WindowsEventLogError::SubscriptionError { source: e }
            })?;

            debug!(message = "Updated bookmark", event_handle = ?event_handle);
            Ok(())
        }
    }

    /// Serializes the bookmark to XML for persistence
    ///
    /// The returned XML string can be saved to a checkpoint file and later
    /// restored using `from_xml()`.
    ///
    /// # Errors
    ///
    /// Returns an error if the Windows API fails to render the bookmark.
    ///
    /// Note: For lock-free serialization, prefer `serialize_handle()` which
    /// allows copying the handle out of a lock before serializing.
    #[cfg(windows)]
    #[allow(dead_code)] // Kept for API completeness; serialize_handle() preferred for performance
    pub fn to_xml(&self) -> Result<String, WindowsEventLogError> {
        unsafe {
            // First call to get required buffer size
            let mut buffer_size: u32 = 0;
            let mut buffer_used: u32 = 0;

            // First call with null buffer to get required size
            // ERROR_INSUFFICIENT_BUFFER (122 / 0x7A) is expected and means success
            let _ = EvtRender(
                None,
                self.handle,
                EvtRenderBookmark.0,
                0,
                None,
                &mut buffer_used,
                &mut buffer_size,
            );
            // Don't check error - ERROR_INSUFFICIENT_BUFFER is expected
            // We only care about buffer_size being set

            if buffer_size == 0 {
                // Bookmark hasn't been updated with any events yet - return empty string
                // This is normal for fresh bookmarks before first event
                debug!(message = "Bookmark not yet updated, skipping serialization");
                return Ok(String::new());
            }

            if buffer_size > MAX_BOOKMARK_XML_SIZE as u32 {
                return Err(WindowsEventLogError::RenderError {
                    message: format!("Bookmark buffer size too large: {}", buffer_size),
                });
            }

            // Allocate buffer and render bookmark XML
            let mut buffer = vec![0u16; (buffer_size / 2) as usize];

            EvtRender(
                None,
                self.handle,
                EvtRenderBookmark.0,
                buffer_size,
                Some(buffer.as_mut_ptr() as *mut _),
                &mut buffer_used,
                &mut buffer_size,
            )
            .map_err(|e| WindowsEventLogError::RenderError {
                message: format!("Failed to render bookmark XML: {}", e),
            })?;

            // Convert UTF-16 buffer to String
            let xml = String::from_utf16_lossy(&buffer[0..((buffer_used / 2) as usize)]);

            debug!(
                message = "Serialized bookmark to XML",
                xml_length = xml.len()
            );

            Ok(xml.trim_end_matches('\0').to_string())
        }
    }

    /// Returns the raw Windows handle for use with EvtSubscribe
    ///
    /// # Safety
    ///
    /// The returned handle is only valid as long as this BookmarkManager exists.
    #[cfg(windows)]
    pub fn as_handle(&self) -> EVT_HANDLE {
        self.handle
    }

    /// Serialize an EVT_HANDLE directly to XML without needing a BookmarkManager reference
    ///
    /// This is useful for serializing bookmarks outside of a lock - you can copy the handle
    /// (just an integer) while holding the lock, then call this method after releasing it.
    ///
    /// # Safety
    ///
    /// The handle must be a valid bookmark handle that hasn't been closed.
    /// Windows EVT_HANDLEs are thread-safe kernel objects, so concurrent
    /// EvtUpdateBookmark and EvtRender calls on the same handle are safe.
    #[cfg(windows)]
    pub fn serialize_handle(handle: EVT_HANDLE) -> Result<String, WindowsEventLogError> {
        unsafe {
            // First call to get required buffer size
            // EvtRender params: Context, Fragment, Flags, BufferSize, Buffer, BufferUsed, PropertyCount
            // BufferUsed (param 6) receives the required size when buffer is too small
            // PropertyCount (param 7) receives number of properties
            let mut buffer_used: u32 = 0;
            let mut property_count: u32 = 0;

            // First call with null buffer to get required size (ERROR_INSUFFICIENT_BUFFER expected)
            let _ = EvtRender(
                None,
                handle,
                EvtRenderBookmark.0,
                0,
                None,
                &mut buffer_used,
                &mut property_count,
            );

            // buffer_used now contains the required size in bytes
            if buffer_used == 0 {
                // Bookmark hasn't been updated with any events yet
                return Ok(String::new());
            }

            if buffer_used > MAX_BOOKMARK_XML_SIZE as u32 {
                return Err(WindowsEventLogError::RenderError {
                    message: format!("Bookmark buffer size too large: {}", buffer_used),
                });
            }

            // Allocate buffer (buffer_used is in bytes, UTF-16 chars are 2 bytes each)
            let mut buffer = vec![0u16; (buffer_used / 2) as usize + 1];

            let mut actual_used: u32 = 0;
            EvtRender(
                None,
                handle,
                EvtRenderBookmark.0,
                buffer_used,
                Some(buffer.as_mut_ptr() as *mut _),
                &mut actual_used,
                &mut property_count,
            )
            .map_err(|e| WindowsEventLogError::RenderError {
                message: format!("Failed to render bookmark XML: {}", e),
            })?;

            let xml = String::from_utf16_lossy(&buffer[0..((actual_used / 2) as usize)]);
            Ok(xml.trim_end_matches('\0').to_string())
        }
    }

    #[cfg(not(windows))]
    pub fn as_handle(&self) -> isize {
        self.handle
    }

    /// Closes the bookmark handle
    ///
    /// This is called automatically when the BookmarkManager is dropped.
    #[cfg(windows)]
    fn close(&mut self) {
        if self.handle.0 != 0 {
            unsafe {
                use windows::Win32::System::EventLog::EvtClose;
                let _ = EvtClose(self.handle);
                debug!(message = "Closed bookmark handle", handle = ?self.handle);
                self.handle = EVT_HANDLE(0);
            }
        }
    }
}

impl Drop for BookmarkManager {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.close();
    }
}

// Non-Windows stub implementation for cross-platform compilation
#[cfg(not(windows))]
impl BookmarkManager {
    pub fn new() -> Result<Self, WindowsEventLogError> {
        Ok(Self { handle: 0 })
    }

    pub fn from_xml(_xml: &str) -> Result<Self, WindowsEventLogError> {
        Ok(Self { handle: 0 })
    }

    pub fn update(&mut self, _event_handle: isize) -> Result<(), WindowsEventLogError> {
        Ok(())
    }

    pub fn to_xml(&self) -> Result<String, WindowsEventLogError> {
        Ok(String::new())
    }

    pub fn as_handle(&self) -> isize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn test_bookmark_lifecycle() {
        // Test creating a new bookmark
        let bookmark = BookmarkManager::new();
        assert!(bookmark.is_ok());

        // Test serialization (should work even without updating)
        let xml = bookmark.unwrap().to_xml();
        assert!(xml.is_ok());
    }

    #[test]
    #[cfg(windows)]
    fn test_bookmark_from_empty_xml() {
        // Empty XML should create a fresh bookmark
        let bookmark = BookmarkManager::from_xml("");
        assert!(bookmark.is_ok());
    }

    #[test]
    fn test_bookmark_handle() {
        let bookmark = BookmarkManager::new().unwrap();
        let handle = bookmark.as_handle();

        #[cfg(windows)]
        assert!(!handle.is_invalid(), "Bookmark handle should be valid");

        #[cfg(not(windows))]
        assert_eq!(handle, 0);
    }
}
