use crate::record::Record;

pub mod field_filter;
pub mod regex_parser;
pub mod sampler;

pub trait Transform: Sync + Send {
    fn transform(&self, record: Record) -> Option<Record>;
}
