use crate::proto::common::v1::InstrumentationScope;
use crate::proto::logs::v1::LogRecord;
use crate::proto::resource::v1::Resource;
use crate::proto::trace::v1::Span;

pub const SOURCE_NAME: &str = "opentelemetry";

pub const RESOURCE_KEY: &str = "resources";
pub const ATTRIBUTES_KEY: &str = "attributes";
pub const SCOPE_KEY: &str = "scope";
pub const NAME_KEY: &str = "name";
pub const VERSION_KEY: &str = "version";
pub const TRACE_ID_KEY: &str = "trace_id";
pub const SPAN_ID_KEY: &str = "span_id";
pub const SEVERITY_TEXT_KEY: &str = "severity_text";
pub const SEVERITY_NUMBER_KEY: &str = "severity_number";
pub const OBSERVED_TIMESTAMP_KEY: &str = "observed_timestamp";
pub const DROPPED_ATTRIBUTES_COUNT_KEY: &str = "dropped_attributes_count";
pub const FLAGS_KEY: &str = "flags";

pub struct ResourceLog {
    pub resource: Option<Resource>,
    pub scope: Option<InstrumentationScope>,
    pub log_record: LogRecord,
}

pub struct ResourceSpan {
    pub resource: Option<Resource>,
    pub span: Span,
}
