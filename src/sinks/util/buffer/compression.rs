use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Derivative, Copy, Clone, Eq, PartialEq)]
#[derivative(Default)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    #[derivative(Default)]
    None,
    Gzip,
}

impl Compression {
    pub fn default_gzip() -> Compression {
        Compression::Gzip
    }

    pub fn content_encoding(&self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Gzip => Some("gzip"),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::None => "log",
            Self::Gzip => "log.gz",
        }
    }
}

#[cfg(feature = "rusoto_core")]
impl From<Compression> for rusoto_core::encoding::ContentEncoding {
    fn from(compression: Compression) -> Self {
        match compression {
            Compression::None => rusoto_core::encoding::ContentEncoding::Identity,
            // 6 is default, add Gzip level support to vector in future
            Compression::Gzip => rusoto_core::encoding::ContentEncoding::Gzip(None, 6),
        }
    }
}
