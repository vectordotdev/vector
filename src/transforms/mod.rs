use snafu::Snafu;

pub mod add_fields;
pub mod add_tags;
pub mod aggregate;
pub mod ansi_stripper;
pub mod aws_cloudwatch_logs_subscription_parser;
pub mod aws_ec2_metadata;
pub mod coercer;
pub mod compound;
pub mod concat;
pub mod dedupe;
pub mod field_filter;
pub mod filter;
pub mod geoip;
pub mod grok_parser;
pub mod json_parser;
pub mod key_value_parser;
pub mod log_to_metric;
pub mod logfmt_parser;
pub mod lua;
pub mod merge;
pub mod metric_to_log;
pub mod noop;
pub mod pipelines;
pub mod reduce;
pub mod regex_parser;
pub mod remap;
pub mod remove_fields;
pub mod remove_tags;
pub mod rename_fields;
pub mod route;
pub mod sample;
pub mod split;
pub mod tag_cardinality_limit;
pub mod throttle;
pub mod tokenizer;

pub use vector_core::transform::{
    FunctionTransform, OutputBuffer, SyncTransform, TaskTransform, Transform, TransformOutputs,
    TransformOutputsBuf,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}

#[cfg(test)]
mod test {
    use vector_core::transform::FunctionTransform;

    use crate::{event::Event, transforms::OutputBuffer};

    /// Transform a single `Event` through the `FunctionTransform`
    ///
    /// # Panics
    ///
    /// If `ft` attempts to emit more than one `Event` on transform this
    /// function will panic.
    // We allow dead_code here to avoid unused warnings when we compile our
    // benchmarks as tests. It's a valid warning -- the benchmarks don't use
    // this function -- but flagging this function off for bench flags will
    // issue a unused warnings about the import above.
    #[allow(dead_code)]
    pub fn transform_one(ft: &mut dyn FunctionTransform, event: Event) -> Option<Event> {
        let mut buf = OutputBuffer::with_capacity(1);
        ft.transform(&mut buf, event);
        assert!(buf.len() <= 1);
        buf.into_events().next()
    }
}
