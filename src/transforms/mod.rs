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
pub mod log_to_metric;
pub mod logfmt_parser;
pub mod lua;
pub mod regex_parser;
pub mod remove_fields;
pub mod remove_tags;
pub mod sampler;
pub mod split;
pub mod tokenizer;

pub trait Transform: Send {
    fn transform(&mut self, event: Event) -> Option<Event>;

    fn transform_into(&mut self, output: &mut Vec<Event>, event: Event) {
        if let Some(transformed) = self.transform(event) {
            output.push(transformed);
        }
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid regular expression: {}", source))]
    InvalidRegex { source: regex::Error },

    #[snafu(display("Invalid substring expression: {}", name))]
    InvalidSubstring { name: String },
}
