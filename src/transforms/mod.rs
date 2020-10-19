use crate::Event;
use snafu::Snafu;
use futures::compat::Stream01CompatExt;

pub mod util;

#[cfg(feature = "transforms-add_fields")]
pub mod add_fields;
#[cfg(feature = "transforms-add_tags")]
pub mod add_tags;
#[cfg(feature = "transforms-ansi_stripper")]
pub mod ansi_stripper;
#[cfg(feature = "transforms-aws_cloudwatch_logs_subscription_parser")]
pub mod aws_cloudwatch_logs_subscription_parser;
#[cfg(feature = "transforms-aws_ec2_metadata")]
pub mod aws_ec2_metadata;
#[cfg(feature = "transforms-coercer")]
pub mod coercer;
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
#[cfg(feature = "transforms-sampler")]
pub mod sampler;
#[cfg(feature = "transforms-split")]
pub mod split;
#[cfg(feature = "transforms-swimlanes")]
pub mod swimlanes;
#[cfg(feature = "transforms-tag_cardinality_limit")]
pub mod tag_cardinality_limit;
#[cfg(feature = "transforms-tokenizer")]
pub mod tokenizer;
#[cfg(feature = "wasm")]
pub mod wasm;

pub enum Transform {
    Function(Box<dyn FunctionTransform>),
    Stream(Box<dyn StreamTransform>),
}

impl Transform {
    pub fn function(v: impl FunctionTransform + 'static) -> Self {
        Transform::Function(Box::new(v))
    }
    pub fn as_function(&mut self) -> &mut Box<dyn FunctionTransform> {
        match self {
            Transform::Function(t) => t,
            Transform::Stream(_) => panic!("Called `Transform::as_function` on something that was not a function variant."),
        }
    }
    pub fn stream(v: impl StreamTransform + 'static) -> Self {
        Transform::Stream(Box::new(v))
    }
    pub fn as_stream(&mut self) -> &mut Box<dyn StreamTransform> {
        match self {
            Transform::Function(_) => panic!("Called `Transform::as_stream` on something that was not a stream variant."),
            Transform::Stream(t) => t,
        }
    }
}

impl Transform {
    /// A handy test function that inputs and outputs only one event.
    ///
    /// In a prior time, Vector primarily used this API to handle events.
    /// However, it's now customary to only implement `transform` which handles multiple output events and can
    /// have it's allocation more effectively controlled.
    #[cfg_attr(not(test), deprecated = "Use `transform` and `output.extend(events)` or `output.push(event)`.")]
    pub fn transform_one(&mut self, event: Event) -> Option<Event> {
        #[allow(deprecated)]
        match self {
            Transform::Function(t) => t.transform_one(event),
            Transform::Stream(t) => t.transform_one(event),
        }
    }
}

pub trait FunctionTransform: Send {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event);

    /// A handy test function that inputs and outputs only one event.
    ///
    /// In a prior time, Vector primarily used this API to handle events.
    /// However, it's now customary to only implement `transform` which handles multiple output events and can
    /// have it's allocation more effectively controlled.
    #[cfg_attr(not(test), deprecated = "Use `transform` and `output.extend(events)` or `output.push(event)`.")]
    fn transform_one(&mut self, event: Event) -> Option<Event> {
        let buf = Vec::with_capacity(1);
        self.transform(&mut buf, event);
        buf.into_iter().next()
    }
}

pub trait StreamTransform: Send {
    fn transform(
        self: Box<Self>,
        stream: Box<dyn futures01::Stream<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn futures01::Stream<Item = Event, Error = ()> + Send>
    where
        Self: 'static;


    /// A handy test function that inputs and outputs only one event.
    ///
    /// In a prior time, Vector primarily used this API to handle events.
    /// However, it's now customary to only implement `transform` which handles multiple output events and can
    /// have it's allocation more effectively controlled.
    #[cfg_attr(not(test), deprecated = "Use `transform` and `output.extend(events)` or `output.push(event)`.")]
    fn transform_one(self: Box<Self>, event: Event) -> Option<Event>
    where Self: 'static {
        let mut in_buf = vec![event];
        let in_stream = futures01::stream::iter_ok(in_buf);

        let out_stream = self.transform(Box::new(in_stream)).compat();
        let out_iter = futures::executor::block_on_stream(out_stream);

        out_iter.next().transpose().ok().flatten()
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}
