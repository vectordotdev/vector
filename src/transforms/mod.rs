use crate::Event;
use snafu::Snafu;

pub mod add_fields;
pub mod add_tags;
pub mod ansi_stripper;
pub mod aws_ec2_metadata;
pub mod coercer;
pub mod concat;
pub mod field_filter;
pub mod geoip;
pub mod grok_parser;
pub mod json_parser;
pub mod kubernetes;
pub mod log_to_metric;
pub mod logfmt_parser;
pub mod lua;
pub mod merge;
pub mod regex_parser;
pub mod remove_fields;
pub mod remove_tags;
pub mod rename_fields;
pub mod sampler;
pub mod split;
pub mod swimlanes;
pub mod tokenizer;

use futures01::{sync::mpsc::Receiver, Stream};

pub trait Transform: Send {
    fn transform(&mut self, event: Event) -> Option<Event>;

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        if let Some(transformed) = self.transform(event) {
            output.push(transformed);
        }
    }

    fn transform_stream(
        self: Box<Self>,
        input_rx: Receiver<Event>,
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
