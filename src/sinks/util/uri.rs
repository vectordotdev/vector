use http::Uri;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;

/// A wrapper for `http::Uri` that implements the serde traits.
#[derive(Default, Debug, Clone)]
pub struct UriSerde(Uri);

impl Serialize for UriSerde {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let uri = format!("{}", self.0);
        serializer.serialize_str(&uri)
    }
}

impl<'a> Deserialize<'a> for UriSerde {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        deserializer.deserialize_str(UriVisitor)
    }
}

impl fmt::Display for UriSerde {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

struct UriVisitor;

impl<'a> Visitor<'a> for UriVisitor {
    type Value = UriSerde;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a string containing a valid HTTP Uri")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        let uri = s.parse::<Uri>().map_err(Error::custom)?;
        Ok(UriSerde(uri))
    }
}

impl From<UriSerde> for Uri {
    fn from(t: UriSerde) -> Self {
        t.0
    }
}

impl From<Uri> for UriSerde {
    fn from(t: Uri) -> Self {
        Self(t)
    }
}

impl std::ops::Deref for UriSerde {
    type Target = Uri;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
