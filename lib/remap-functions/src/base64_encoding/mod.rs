mod decode_base64;
mod encode_base64;

pub use decode_base64::DecodeBase64;
pub use encode_base64::EncodeBase64;

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    Standard,
    UrlSafe,
}

impl Default for Charset {
    fn default() -> Self {
        Self::Standard
    }
}

impl Into<base64::CharacterSet> for Charset {
    fn into(self) -> base64::CharacterSet {
        use Charset::*;

        match self {
            Standard => base64::CharacterSet::Standard,
            UrlSafe => base64::CharacterSet::UrlSafe,
        }
    }
}

impl FromStr for Charset {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Charset::*;

        match s {
            "standard" => Ok(Standard),
            "url_safe" => Ok(UrlSafe),
            _ => Err("unknown charset"),
        }
    }
}
