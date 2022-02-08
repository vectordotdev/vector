#![deny(missing_docs)]

use super::{transform_utils::optional::Optional, FILE_KEY};
use crate::{
    event,
    transforms::merge::{Merge, MergeConfig},
};

/// Partial event merger.
pub type PartialEventsMerger = Optional<Merge>;

pub fn build(enabled: bool) -> PartialEventsMerger {
    Optional(if enabled {
        Some(
            MergeConfig {
                partial_event_marker_field: event::PARTIAL.to_string(),
                fields: vec![crate::config::log_schema().message_key().to_string()],
                stream_discriminant_fields: vec![(&*FILE_KEY).to_string()],
            }
            .into(),
        )
    } else {
        None
    })
}
