use crate::{
    event::Event,
    topology::fanout::{self, Fanout},
};
use futures::SinkExt;
use snafu::Snafu;
use std::collections::HashMap;
use vector_core::ByteSizeOf;

#[cfg(feature = "transforms-add_fields")]
pub mod add_fields;
#[cfg(feature = "transforms-add_tags")]
pub mod add_tags;
#[cfg(feature = "transforms-aggregate")]
pub mod aggregate;
#[cfg(feature = "transforms-ansi_stripper")]
pub mod ansi_stripper;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-coercer")]
pub mod coercer;
#[cfg(feature = "transforms-compound")]
pub mod compound;
#[cfg(feature = "transforms-concat")]
pub mod concat;
#[cfg(feature = "transforms-dedupe")]
pub mod dedupe;
#[cfg(feature = "transforms-field_filter")]
pub mod field_filter;
#[cfg(feature = "transforms-filter")]
pub mod filter;
#[cfg(feature = "transforms-geoip")]
pub mod geoip;
#[cfg(feature = "transforms-grok_parser")]
pub mod grok_parser;
#[cfg(feature = "transforms-json_parser")]
pub mod json_parser;
#[cfg(feature = "transforms-key_value_parser")]
pub mod key_value_parser;
#[cfg(feature = "transforms-log_to_metric")]
pub mod log_to_metric;
#[cfg(feature = "transforms-logfmt_parser")]
pub mod logfmt_parser;
#[cfg(feature = "transforms-lua")]
pub mod lua;
#[cfg(feature = "transforms-merge")]
pub mod merge;
#[cfg(feature = "transforms-metric_to_log")]
pub mod metric_to_log;
pub mod noop;
#[cfg(feature = "transforms-pipelines")]
pub mod pipelines;
#[cfg(feature = "transforms-reduce")]
pub mod reduce;
#[cfg(feature = "transforms-regex_parser")]
pub mod regex_parser;
#[cfg(feature = "transforms-remap")]
pub mod remap;
#[cfg(feature = "transforms-remove_fields")]
pub mod remove_fields;
#[cfg(feature = "transforms-remove_tags")]
pub mod remove_tags;
#[cfg(feature = "transforms-rename_fields")]
pub mod rename_fields;
#[cfg(feature = "transforms-route")]
pub mod route;
#[cfg(feature = "transforms-sample")]
pub mod sample;
#[cfg(feature = "transforms-split")]
pub mod split;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-throttle")]
pub mod throttle;
#[cfg(feature = "transforms-tokenizer")]
pub mod tokenizer;

pub use vector_core::transform::{
    FallibleFunctionTransform, FunctionTransform, TaskTransform, Transform,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}

/// This currently a topology-focused trait that unifies function and fallible function transforms.
/// Eventually it (or something very similar) should be able to replace both entirely. That will
/// likely involve it not being batch-focused anymore, and since we'll then be able to have
/// a single implementation of these loops that apply across all sync transforms.
pub trait SyncTransform: Send + Sync {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs);
}

impl SyncTransform for Box<dyn FallibleFunctionTransform> {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs) {
        let mut buf = Vec::with_capacity(1);
        let mut err_buf = Vec::with_capacity(1);

        for v in events {
            self.transform(&mut buf, &mut err_buf, v);
            outputs.append(&mut buf);
            // TODO: this is a regession in the number of places that we hardcode this name, but it
            // is temporary because we're quite close to being able to remove the overly-specific
            // `FallibleFunctionTransform` trait entirely.
            outputs.append_named("dropped", &mut err_buf);
        }
    }
}

impl SyncTransform for Box<dyn FunctionTransform> {
    fn run(&mut self, events: Vec<Event>, outputs: &mut TransformOutputs) {
        let mut buf = Vec::with_capacity(4); // also an arbitrary,
                                             // smallish constant
        for v in events {
            self.transform(&mut buf, v);
            outputs.append(&mut buf);
        }
    }
}

/// This struct manages collecting and forwarding the various outputs of transforms. It's designed
/// to unify the interface for transforms that may or may not have more than one possible output
/// path. It's currently batch-focused for use in topology-level tasks, but can easily be extended
/// to be used directly by transforms via a new, simpler trait interface.
pub struct TransformOutputs {
    primary_buffer: Vec<Event>,
    named_buffers: HashMap<String, Vec<Event>>,
    primary_output: Fanout,
    named_outputs: HashMap<String, Fanout>,
}

impl TransformOutputs {
    pub fn new_with_capacity(
        named_outputs_in: Vec<String>,
        capacity: usize,
    ) -> (Self, HashMap<Option<String>, fanout::ControlChannel>) {
        let mut named_buffers = HashMap::new();
        let mut named_outputs = HashMap::new();
        let mut controls = HashMap::new();

        for name in named_outputs_in {
            let (fanout, control) = Fanout::new();
            named_outputs.insert(name.clone(), fanout);
            controls.insert(Some(name.clone()), control);
            named_buffers.insert(name.clone(), Vec::new());
        }

        let (primary_output, control) = Fanout::new();
        let me = Self {
            primary_buffer: Vec::with_capacity(capacity),
            named_buffers,
            primary_output,
            named_outputs,
        };
        controls.insert(None, control);

        (me, controls)
    }

    pub fn append(&mut self, slice: &mut Vec<Event>) {
        self.primary_buffer.append(slice)
    }

    pub fn append_named(&mut self, name: &str, slice: &mut Vec<Event>) {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .append(slice)
    }

    pub fn len(&self) -> usize {
        self.primary_buffer.len()
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.len())
                .sum::<usize>()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub async fn flush(&mut self) {
        flush_inner(&mut self.primary_buffer, &mut self.primary_output).await;
        for (key, buf) in self.named_buffers.iter_mut() {
            flush_inner(
                buf,
                self.named_outputs.get_mut(key).expect("unknown output"),
            )
            .await;
        }
    }
}

async fn flush_inner(buf: &mut Vec<Event>, output: &mut Fanout) {
    for event in buf.drain(..) {
        output.feed(event).await.expect("unit error")
    }
}

impl ByteSizeOf for TransformOutputs {
    fn allocated_bytes(&self) -> usize {
        self.primary_buffer.size_of()
            + self
                .named_buffers
                .iter()
                .map(|(_, buf)| buf.size_of())
                .sum::<usize>()
    }
}

#[cfg(test)]
mod test {
    use crate::event::Event;
    use vector_core::transform::FunctionTransform;

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
        let mut buf = Vec::with_capacity(1);
        ft.transform(&mut buf, event);
        assert!(buf.len() < 2);
        buf.into_iter().next()
    }
}
