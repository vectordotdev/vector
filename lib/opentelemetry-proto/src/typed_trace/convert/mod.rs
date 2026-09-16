//! OTLP protobuf -> typed [`TraceEvent`] conversion.
//!
//! This is the only OTLP-to-typed mapping. The legacy converter in `legacy` reaches it by
//! rebuilding `ResourceSpans` from legacy layouts, so mapping rules belong here rather than in
//! the legacy parsers.
//!
//! Decode constructors are `into_typed` on the protobuf types. The preferred
//! `Resource::from_otlp` / `Span::from_otlp` form is blocked: those types are
//! defined in `vector_core`, so this crate cannot add inherent methods on them.

mod decode;
mod synthesize;

pub(super) use synthesize::chunk_has_bridge_keys;

use vector_core::event::typed_trace::{
    AttrMap, DroppedCount, TraceConversionIssue, TraceConversionReporter,
};

use super::keys::is_reserved_datadog_key;

fn malformed_bridge(
    dropped: &mut DroppedCount,
    key: &str,
    reporter: &mut impl TraceConversionReporter,
) {
    dropped.increment(1);
    reporter.report(TraceConversionIssue::InvalidReservedAttribute { key });
}

fn strip_reserved_map(
    map: &mut AttrMap,
    dropped: &mut DroppedCount,
    reporter: &mut impl TraceConversionReporter,
) {
    map.retain(|key, _| {
        let reserved = is_reserved_datadog_key(key);
        if reserved {
            malformed_bridge(dropped, key, reporter);
        }
        !reserved
    });
}
