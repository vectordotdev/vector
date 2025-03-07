mod config;
mod encoder;
mod service;

use futures::future::BoxFuture;
use vector_lib::sink::VectorSink;

pub use config::{
    AggregationTemporalityConfig, OpentelemetryMetricsDefaultBatchSettings,
    OpentelemetryMetricsSinkConfig, OpentelemetryMetricsTowerRequestConfigDefaults,
};
pub use service::OpentelemetryMetricsSvc;

type Healthcheck = BoxFuture<'static, crate::Result<()>>;
