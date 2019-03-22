use crate::record::Record;

pub mod add_fields;
pub mod field_filter;
pub mod regex_parser;
pub mod sampler;

pub trait Transform: Sync + Send {
    fn transform(&self, record: Record) -> Option<Record>;
}
