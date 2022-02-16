use vector_common::TimeZone;

mod cri;
mod docker;
mod picker;
mod test_util;

/// Parser for any log format supported by `kubelet`.
pub(super) type Parser = picker::Picker;

/// Build a parser for any log format supported by `kubelet`.
pub(super) const fn build(timezone: TimeZone) -> Parser {
    picker::Picker::new(timezone)
}
