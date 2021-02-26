use shared::TimeZone;

mod cri;
mod docker;
mod picker;
mod test_util;

/// Parser for any log format supported by `kubelet`.
pub(crate) type Parser = picker::Picker;

/// Build a parser for any log format supported by `kubelet`.
pub(crate) fn build(timezone: TimeZone) -> Parser {
    picker::Picker::new(timezone)
}
