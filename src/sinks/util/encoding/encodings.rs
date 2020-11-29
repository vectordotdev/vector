/// Enums representing different ways to encode events as they are sent into a Sink.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum EncodingText {
    #[derivative(Default)]
    Text,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum EncodingJson {
    #[derivative(Default)]
    Json,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingTextJson {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Eq, PartialEq, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum EncodingTextJsonDefaultJson {
    Text,
    #[derivative(Default)]
    Json,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingTextNdjson {
    Text,
    Ndjson,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodingTextJsonNdjson {
    Text,
    Json,
    Ndjson,
}

impl From<EncodingJson> for EncodingTextJson {
    fn from(encoding: EncodingJson) -> Self {
        match encoding {
            EncodingJson::Json => Self::Json,
        }
    }
}

impl From<EncodingJson> for EncodingTextJsonNdjson {
    fn from(encoding: EncodingJson) -> Self {
        match encoding {
            EncodingJson::Json => Self::Json,
        }
    }
}

impl From<EncodingTextJsonDefaultJson> for EncodingTextJson {
    fn from(encoding: EncodingTextJsonDefaultJson) -> Self {
        match encoding {
            EncodingTextJsonDefaultJson::Text => Self::Text,
            EncodingTextJsonDefaultJson::Json => Self::Json,
        }
    }
}
