pub mod acknowledgements;
pub mod request;
pub mod response;
pub mod service;
pub mod util;

pub use util::*;

pub use crate::sinks::util::splunk_hec::EndpointTarget;

pub(super) const SOURCE_FIELD: &str = "source";
pub(super) const SOURCETYPE_FIELD: &str = "sourcetype";
pub(super) const INDEX_FIELD: &str = "index";
pub(super) const HOST_FIELD: &str = "host";
pub(super) const AUTO_EXTRACT_TIMESTAMP_FIELD: &str = "auto_extract_timestamp";
