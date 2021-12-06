mod config;
mod healthcheck;
mod request;
mod retry;
mod service;
mod sink;

mod integration_tests;
mod tests;

use self::config::CloudwatchLogsSinkConfig;
use crate::aws::rusoto::{self, AwsAuthentication, RegionOrEndpoint};
use crate::sinks::util::encoding::StandardEncodings;
use crate::{
    config::{
        log_schema, DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext, SinkDescription,
    },
    event::{Event, LogEvent, Value},
    internal_events::TemplateRenderingFailed,
    sinks::util::{
        batch::BatchConfig,
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::{FixedRetryPolicy, RetryLogic},
        Compression, EncodedEvent, EncodedLength, PartitionBatchSink, PartitionBuffer,
        PartitionInnerBuffer, TowerRequestConfig, TowerRequestSettings, VecBuffer,
    },
    template::Template,
};
use chrono::{Duration, Utc};
use futures::{future::BoxFuture, ready, stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use rusoto_core::{request::BufferedHttpResponse, RusotoError};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError,
    DescribeLogGroupsRequest, DescribeLogStreamsError, InputLogEvent, PutLogEventsError,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::HashMap,
    convert::TryInto,
    fmt,
    num::NonZeroU64,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower::{
    buffer::Buffer,
    limit::{concurrency::ConcurrencyLimit, rate::RateLimit},
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};
use vector_core::ByteSizeOf;

use super::util::SinkBatchSettings;

#[derive(Debug, Snafu)]
pub(self) enum CloudwatchLogsError {
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_credential::CredentialsError,
    },
    #[snafu(display("Encoded event is too long, length={}", length))]
    EventTooLong { length: usize },
}

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}
