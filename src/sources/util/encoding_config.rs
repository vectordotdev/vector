use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig {
    #[serde(default)]
    pub charset: Option<&'static encoding_rs::Encoding>,
}

impl EncodingConfig {
    pub fn charset(&self) -> &Option<&'static encoding_rs::Encoding> {
        &self.charset
    }
}
