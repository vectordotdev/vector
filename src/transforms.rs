use crate::Event;

pub mod add_fields;
pub mod field_filter;
pub mod json_parser;
pub mod log_to_metric;
pub mod lua;
pub mod regex_parser;
pub mod remove_fields;
pub mod sampler;

pub trait Transform: Send {
    fn transform(&self, event: Event) -> Option<Event>;
}
