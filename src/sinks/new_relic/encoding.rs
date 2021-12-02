use crate::{
    sinks::util::encoding::{
        Encoder, EncodingConfigFixed
    }
};
use serde::{
    Deserialize, Serialize
};
use std::{
    fmt::Debug,
    io
};
use super::NewRelicApiModel;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

impl Encoder<Result<NewRelicApiModel, &'static str>> for EncodingConfigFixed<Encoding> {
    fn encode_input(&self, input: Result<NewRelicApiModel, &'static str>, writer: &mut dyn io::Write) -> io::Result<usize> {
        if let Ok(api_model) = input {
            let json = match api_model {
                NewRelicApiModel::Events(ev_api_model) => to_json(&ev_api_model),
                NewRelicApiModel::Metrics(met_api_model) => to_json(&met_api_model),
                NewRelicApiModel::Logs(log_api_model) => to_json(&log_api_model),
            };
            if let Some(json) = json {
                let size = writer.write(&json)?;
                io::Result::Ok(size)
            }
            else {
                io::Result::Ok(0)
            }
        }
        else {
            io::Result::Ok(0)
        }
    }
}

pub fn to_json<T: Serialize>(model: &T) -> Option<Vec<u8>> {
    match serde_json::to_vec(model) {
        Ok(mut json) => {
            json.push(b'\n');
            Some(json)
        },
        Err(error) => {
            error!(message = "Failed generating JSON.", %error);
            None
        }
    }
}