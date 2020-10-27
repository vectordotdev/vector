#![deny(missing_docs)]

use super::transform_utils::optional::Optional;
use super::FILE_KEY;
use crate::event;
use crate::transforms::merge::{Merge, MergeConfig};

/// Partial event merger.
pub type PartialEventsMerger = Optional<Merge>;

pub fn build(enabled: bool) -> PartialEventsMerger {
    Optional(if enabled {
        Some(
            MergeConfig {
                partial_event_marker_field: event::PARTIAL.clone(),
                fields: vec![crate::config::log_schema().message_key().into_buf()],
                stream_discriminant_fields: vec![FILE_KEY.clone()],
            }
            .into(),
        )
    } else {
        None
    })
}
