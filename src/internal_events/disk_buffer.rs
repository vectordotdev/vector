use std::path::{Path, PathBuf};

use vector_lib::{
    NamedInternalEvent,
    buffers::DiskBufferUsageSnapshot,
    config::ComponentKey,
    counter,
    internal_event::{CounterName, InternalEvent},
};

#[derive(Debug, NamedInternalEvent)]
pub struct TopologyDiskBufferRemoved<'a> {
    pub component_id: &'a ComponentKey,
    pub buffer_dirs: &'a [PathBuf],
    pub usage: Option<DiskBufferUsageSnapshot>,
    pub drain_in_background: bool,
}

impl InternalEvent for TopologyDiskBufferRemoved<'_> {
    fn emit(self) {
        let message = match (self.usage.is_some(), self.drain_in_background) {
            (true, true) => {
                "Removing sink with attached disk buffer containing buffered bytes. Its current instance will continue draining in the background until the buffer is empty or Vector shuts down."
            }
            (true, false) => {
                "Removing sink with attached disk buffer containing buffered bytes. Configuration reload will wait for its current instance to drain the buffer."
            }
            (false, true) => {
                "Removing sink with attached disk buffer whose buffered-byte count is unavailable. Its current instance will continue draining in the background until the buffer is empty or Vector shuts down."
            }
            (false, false) => {
                "Removing sink with attached disk buffer whose buffered-byte count is unavailable. Configuration reload will wait for its current instance to drain the buffer."
            }
        };
        if let Some(usage) = self.usage {
            info!(
                component_id = %self.component_id,
                buffer_dirs = ?self.buffer_dirs,
                buffered_events = usage.event_count,
                buffered_bytes = usage.byte_size,
                drain_in_background = self.drain_in_background,
                message,
            );
        } else {
            info!(
                component_id = %self.component_id,
                buffer_dirs = ?self.buffer_dirs,
                drain_in_background = self.drain_in_background,
                usage_unavailable = true,
                message,
            );
        }
        counter!(
            CounterName::DiskBufferLifecycleEventsTotal,
            "event" => "removed",
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct TopologyDiskBufferReplacementUnreferenced<'a> {
    pub component_id: &'a ComponentKey,
    pub buffer_dirs: &'a [PathBuf],
    pub new_buffer_dirs: &'a [PathBuf],
    pub usage: Option<DiskBufferUsageSnapshot>,
}

impl InternalEvent for TopologyDiskBufferReplacementUnreferenced<'_> {
    fn emit(self) {
        if let Some(usage) = self.usage {
            warn!(
                component_id = %self.component_id,
                buffer_dirs = ?self.buffer_dirs,
                new_buffer_dirs = ?self.new_buffer_dirs,
                buffered_events = usage.event_count,
                buffered_bytes = usage.byte_size,
                message = "Replacement sink does not use the previous disk buffer. If reload succeeds, its buffered bytes will be left unreferenced.",
            );
        } else {
            warn!(
                component_id = %self.component_id,
                buffer_dirs = ?self.buffer_dirs,
                new_buffer_dirs = ?self.new_buffer_dirs,
                usage_unavailable = true,
                message = "Replacement sink does not use the previous disk buffer, and its buffered-byte count is unavailable. If reload succeeds, any buffered bytes will be left unreferenced.",
            );
        }
        counter!(
            CounterName::DiskBufferLifecycleEventsTotal,
            "event" => "replacement_unreferenced",
        )
        .increment(1);
    }
}

#[derive(Debug, NamedInternalEvent)]
pub struct TopologyDiskBufferUnreferenced<'a> {
    pub buffer_id: &'a Path,
    pub buffer_dir: &'a Path,
}

impl InternalEvent for TopologyDiskBufferUnreferenced<'_> {
    fn emit(self) {
        warn!(
            buffer_id = self.buffer_id.to_string_lossy().as_ref(),
            buffer_dir = self.buffer_dir.to_string_lossy().as_ref(),
            message = "Found disk buffer not referenced by the current configuration. It may still be draining through a detached sink instance; otherwise its buffered bytes are not being processed.",
        );
        counter!(
            CounterName::DiskBufferLifecycleEventsTotal,
            "event" => "unreferenced",
        )
        .increment(1);
    }
}
