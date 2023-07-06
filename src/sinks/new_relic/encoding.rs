use std::io;

use serde::Serialize;
use vector_common::request_metadata::GroupedCountByteSize;
use vector_core::config::telemetry;

use super::{NewRelicApiModel, NewRelicSinkError};
use crate::sinks::util::encoding::{as_tracked_write, Encoder};

pub struct NewRelicEncoder;

impl Encoder<Result<NewRelicApiModel, NewRelicSinkError>> for NewRelicEncoder {
    fn encode_input(
        &self,
        input: Result<NewRelicApiModel, NewRelicSinkError>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let json = match input? {
            NewRelicApiModel::Events(ev_api_model) => to_json(&ev_api_model)?,
            NewRelicApiModel::Metrics(met_api_model) => to_json(&met_api_model)?,
            NewRelicApiModel::Logs(log_api_model) => to_json(&log_api_model)?,
        };
        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            writer.write_all(json)?;
            Ok(())
        })?;

        // TODO This should not be zero.
        let byte_size = telemetry().create_request_count_byte_size();

        io::Result::Ok((size, byte_size))
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
