use super::{NewRelicApiModel, NewRelicSinkError};
use crate::sinks::util::encoding::{as_tracked_write, Encoder, EncodingConfigFixed};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, io};

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

impl Encoder<Result<NewRelicApiModel, NewRelicSinkError>> for EncodingConfigFixed<Encoding> {
    fn encode_input(
        &self,
        input: Result<NewRelicApiModel, NewRelicSinkError>,
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        let json = match input? {
            NewRelicApiModel::Events(ev_api_model) => to_json(&ev_api_model)?,
            NewRelicApiModel::Metrics(met_api_model) => to_json(&met_api_model)?,
            NewRelicApiModel::Logs(log_api_model) => to_json(&log_api_model)?,
        };
        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            writer.write_all(json)?;
            Ok(())
        })?;
        io::Result::Ok(size)
    }
}

pub fn to_json<T: Serialize>(model: &T) -> Result<Vec<u8>, NewRelicSinkError> {
    match serde_json::to_vec(model) {
        Ok(mut json) => {
            json.push(b'\n');
            Ok(json)
        }
        Err(error) => Err(NewRelicSinkError::new(&format!(
            "Failed generating JSON: {}",
            error
        ))),
    }
}
