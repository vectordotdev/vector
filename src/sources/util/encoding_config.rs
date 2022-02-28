use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncodingConfig {
    pub(crate) charset: &'static encoding_rs::Encoding,
}
