use std::{io, sync::Arc};

use serde::Serialize;
use vector_lib::request_metadata::GroupedCountByteSize;
use vector_lib::{config::telemetry, event::Event, EstimatedJsonEncodedSizeOf};

use super::{
    EventsApiModel, LogsApiModel, MetricsApiModel, NewRelicApi, NewRelicApiModel,
    NewRelicCredentials, NewRelicSinkError,
};
use crate::sinks::{
    prelude::*,
    util::encoding::{as_tracked_write, Encoder},
};

pub struct NewRelicEncoder {
    pub(super) transformer: Transformer,
    pub(super) credentials: Arc<NewRelicCredentials>,
}

impl Encoder<Vec<Event>> for NewRelicEncoder {
    fn encode_input(
        &self,
        mut input: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut byte_size = telemetry().create_request_count_byte_size();

        for event in input.iter_mut() {
            self.transformer.transform(event);
            byte_size.add_event(event, event.estimated_json_encoded_size_of());
        }

        let api_model = match self.credentials.api {
            NewRelicApi::Events => NewRelicApiModel::Events(EventsApiModel::try_from(input)?),
            NewRelicApi::Metrics => NewRelicApiModel::Metrics(MetricsApiModel::try_from(input)?),
            NewRelicApi::Logs => NewRelicApiModel::Logs(LogsApiModel::try_from(input)?),
        };

        let json = match api_model {
            NewRelicApiModel::Events(ev_api_model) => to_json(&ev_api_model)?,
            NewRelicApiModel::Metrics(met_api_model) => to_json(&met_api_model)?,
            NewRelicApiModel::Logs(log_api_model) => to_json(&log_api_model)?,
        };

        let size = as_tracked_write::<_, _, io::Error>(writer, &json, |writer, json| {
            writer.write_all(json)?;
            Ok(())
        })?;

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
