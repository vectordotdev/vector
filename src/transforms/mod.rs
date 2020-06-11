use crate::Event;
use snafu::Snafu;

mod util;

#[cfg(feature = "transforms-add_fields")]
pub mod add_fields;
#[cfg(feature = "transforms-add_tags")]
pub mod add_tags;
#[cfg(feature = "transforms-ansi_stripper")]
pub mod ansi_stripper;
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
#[cfg(feature = "transforms-regex_parser")]
pub mod regex_parser;
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

use futures01::Stream;

pub trait Transform: Send {
    fn transform(&mut self, event: Event) -> Option<Event>;

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        if let Some(transformed) = self.transform(event) {
            output.push(transformed);
        }
    }

    fn transform_stream(
        self: Box<Self>,
        input_rx: Box<dyn Stream<Item = Event, Error = ()> + Send>,
    ) -> Box<dyn Stream<Item = Event, Error = ()> + Send>
    where
        Self: 'static,
    {
        let mut me = self;
        Box::new(
            input_rx
                .map(move |event| {
                    let mut output = Vec::with_capacity(1);
                    me.transform_into(&mut output, event);
                    futures01::stream::iter_ok(output.into_iter())
                })
                .flatten(),
        )
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}
