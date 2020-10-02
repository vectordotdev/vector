mod cri;
mod docker;
mod picker;
mod test_util;

/// Parser for any log format supported by `kubelet`.
pub type Parser = picker::Picker;

/// Build a parser for any log format supported by `kubelet`.
pub fn build() -> Parser {
    picker::Picker::new()
}
