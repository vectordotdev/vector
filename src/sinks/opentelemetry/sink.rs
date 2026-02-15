//! Implementation of the OpenTelemetry sink with custom partitioning strategies.

use std::collections::BTreeMap;

use super::config::PartitionStrategy;
use crate::sinks::{
    http::{
        batch::HttpBatchSizer, request_builder::HttpRequestBuilder, sink::PartitionKey as HttpPartitionKey,
    },
    prelude::*,
    util::http::HttpRequest,
};

pub(super) struct OpenTelemetrySink<S> {
    service: S,
    uri: Template,
    headers: BTreeMap<String, Template>,
    batch_settings: BatcherSettings,
    request_builder: HttpRequestBuilder,
    partition_strategy: PartitionStrategy,
}

impl<S> OpenTelemetrySink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    /// Creates a new `OpenTelemetrySink`.
    pub(super) const fn new(
        service: S,
        uri: Template,
        headers: BTreeMap<String, Template>,
        batch_settings: BatcherSettings,
        request_builder: HttpRequestBuilder,
        partition_strategy: PartitionStrategy,
    ) -> Self {
        Self {
            service,
            uri,
            headers,
            batch_settings,
            request_builder,
            partition_strategy,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_sizer = HttpBatchSizer {
            encoder: self.request_builder.encoder.encoder.clone(),
        };
        
        let partitioner = OtelKeyPartitioner::new(
            self.uri,
            self.headers,
            self.partition_strategy,
        );
        
        input
            // Batch the input stream with size calculation based on the configured codec
            .batched_partitioned(partitioner, || {
                self.batch_settings.as_item_size_config(batch_sizer.clone())
            })
            .filter_map(|(key, batch)| async move { key.map(move |k| (k, batch)) })
            // Build requests with default concurrency limit.
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            // Filter out any errors that occurred in the request building.
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            // Generate the driver that will send requests and handle retries,
            // event finalization, and logging/internal metric reporting.
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for OpenTelemetrySink<S>
where
    S: Service<HttpRequest<PartitionKey>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Partition key for OpenTelemetry events.
/// 
/// This key type supports multiple partitioning strategies:
/// - `UriHeaders`: Partitions by URI and headers (legacy HTTP sink behavior)
/// - `InstrumentationScope`: Partitions by OTLP InstrumentationScope (name + version)
#[derive(Eq, PartialEq, Clone, Debug, Hash)]
pub enum PartitionKey {
    /// Partition by URI and headers (legacy behavior)
    UriHeaders(HttpPartitionKey),
    /// Partition by InstrumentationScope (name + version)
    InstrumentationScope {
        uri: String,
        headers: BTreeMap<String, String>,
        scope_name: String,
        scope_version: String,
    },
}

impl PartitionKey {
    /// Get the URI from the partition key
    pub fn uri(&self) -> &str {
        match self {
            PartitionKey::UriHeaders(key) => &key.uri,
            PartitionKey::InstrumentationScope { uri, .. } => uri,
        }
    }

    /// Get the headers from the partition key
    pub fn headers(&self) -> &BTreeMap<String, String> {
        match self {
            PartitionKey::UriHeaders(key) => &key.headers,
            PartitionKey::InstrumentationScope { headers, .. } => headers,
        }
    }
}

// Implement conversion to HttpPartitionKey for compatibility with HTTP request builder
impl From<PartitionKey> for HttpPartitionKey {
    fn from(key: PartitionKey) -> Self {
        match key {
            PartitionKey::UriHeaders(http_key) => http_key,
            PartitionKey::InstrumentationScope { uri, headers, .. } => {
                HttpPartitionKey { uri, headers }
            }
        }
    }
}

struct OtelKeyPartitioner {
    uri: Template,
    headers: BTreeMap<String, Template>,
    strategy: PartitionStrategy,
}

impl OtelKeyPartitioner {
    const fn new(
        uri: Template,
        headers: BTreeMap<String, Template>,
        strategy: PartitionStrategy,
    ) -> Self {
        Self {
            uri,
            headers,
            strategy,
        }
    }

    fn extract_scope_info(&self, event: &Event) -> Option<(String, String)> {
        // Extract instrumentation scope from event metadata
        // The scope is stored at metadata path: opentelemetry.scope.name and opentelemetry.scope.version
        match event {
            Event::Log(log) => {
                let scope_name = log
                    .metadata()
                    .value()
                    .get(path!("opentelemetry", "scope", "name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let scope_version = log
                    .metadata()
                    .value()
                    .get(path!("opentelemetry", "scope", "version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                Some((scope_name, scope_version))
            }
            Event::Trace(trace) => {
                let scope_name = trace
                    .metadata()
                    .value()
                    .get(path!("opentelemetry", "scope", "name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let scope_version = trace
                    .metadata()
                    .value()
                    .get(path!("opentelemetry", "scope", "version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                Some((scope_name, scope_version))
            }
            Event::Metric(_) => {
                // For metrics, check tags
                // OTLP metrics store scope in tags as "scope.name" and "scope.version"
                None // Will be handled separately for metrics
            }
        }
    }
}

impl Partitioner for OtelKeyPartitioner {
    type Item = Event;
    type Key = Option<PartitionKey>;

    fn partition(&self, event: &Event) -> Self::Key {
        // First, render URI and headers
        let uri = self
            .uri
            .render_string(event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("uri"),
                    drop_event: true,
                });
            })
            .ok()?;

        let mut headers = BTreeMap::new();
        for (name, template) in &self.headers {
            let value = template
                .render_string(event)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("headers"),
                        drop_event: true,
                    });
                })
                .ok()?;
            headers.insert(name.clone(), value);
        }

        // Choose partition strategy
        match self.strategy {
            PartitionStrategy::UriHeaders => Some(PartitionKey::UriHeaders(HttpPartitionKey {
                uri,
                headers,
            })),
            PartitionStrategy::InstrumentationScope => {
                let (scope_name, scope_version) = self.extract_scope_info(event)?;
                Some(PartitionKey::InstrumentationScope {
                    uri,
                    headers,
                    scope_name,
                    scope_version,
                })
            }
        }
    }
}
