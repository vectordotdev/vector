mod config;
mod encoding;
mod healthcheck;
mod model;
mod service;
mod sink;

use config::{NewRelicApi, NewRelicCredentials};
use encoding::NewRelicEncoder;
use model::{EventsApiModel, LogsApiModel, MetricsApiModel, NewRelicApiModel};
use service::{NewRelicApiRequest, NewRelicApiResponse, NewRelicApiService};
use sink::{NewRelicSink, NewRelicSinkError};

use super::{Healthcheck, VectorSink};

#[cfg(test)]
mod tests;
