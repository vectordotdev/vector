pub mod error_stage {
    pub const RECEIVING: &str = "receiving";
    pub const PROCESSING: &str = "processing";
    pub const SENDING: &str = "sending";
}

pub(crate) fn http_error_code(code: u16) -> String {
    format!("http_response_{}", code)
}
