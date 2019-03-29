use crate::record::Record;

pub mod add_fields;
pub mod field_filter;
pub mod json_parser;
pub mod regex_parser;
pub mod remove_fields;
pub mod sampler;

pub trait Transform: Sync + Send {
    fn transform(&self, record: Record) -> Option<Record>;
}
