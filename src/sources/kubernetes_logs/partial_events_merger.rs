#![deny(missing_docs)]

use super::FILE_KEY;
use crate::transforms::merge::{Merge, MergeConfig};
use crate::{event, transforms::util::optional::Optional};
use string_cache::Atom;

/// Partial event merger.
pub type PartialEventsMerger = Optional<Merge>;

pub fn build(enabled: bool) -> PartialEventsMerger {
    Optional(if enabled {
        Some(
            MergeConfig {
                partial_event_marker_field: event::PARTIAL.clone(),
                merge_fields: vec![event::log_schema().message_key().clone()],
                stream_discriminant_fields: vec![Atom::from(FILE_KEY)],
            }
            .into(),
        )
    } else {
        None
    })
}
