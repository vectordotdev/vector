use serde_json::json;
use strum::{AsRefStr, Display, EnumIter};
use vector_config::configurable_component;

use super::metric_tags::{
    COMPONENT_RECEIVED_EVENTS_TAGS, COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS, COMPONENT_TAGS,
    COMPONENT_TAGS_ERROR_TYPE_STAGE, COMPONENT_TAGS_GRPC_ALL, COMPONENT_TAGS_GRPC_METHOD_SERVICE,
    COMPONENT_TAGS_HTTP_ALL, COMPONENT_TAGS_HTTP_METHOD, COMPONENT_TAGS_HTTP_METHOD_PATH,
    COMPONENT_TAGS_HTTP_STATUS, COMPONENT_TAGS_OUTPUT, INTERNAL_METRICS_TAGS,
    INTERNAL_METRICS_TAGS_FILE, INTERNAL_METRICS_TAGS_REASON, S3_OBJECT_PROCESSING_TAGS,
    merge_lazy,
};

/// Canonical list of all per-component internal counter metric names emitted by Vector.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, AsRefStr, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CounterName {
    /// The number of events accepted by this component either from tagged
    /// origins like file and uri, or cumulatively from other origins.
    #[configurable(metadata(docs::tags = &*COMPONENT_RECEIVED_EVENTS_TOTAL_TAGS))]
    ComponentReceivedEventsTotal,

    /// The number of event bytes accepted by this component either from
    /// tagged origins like file and uri, or cumulatively from other origins.
    #[configurable(metadata(docs::tags = &*COMPONENT_RECEIVED_EVENTS_TAGS))]
    ComponentReceivedEventBytesTotal,

    /// The number of raw bytes accepted by this component from source origins.
    #[configurable(metadata(docs::tags = &*COMPONENT_RECEIVED_EVENTS_TAGS))]
    ComponentReceivedBytesTotal,

    /// The total number of events emitted by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    ComponentSentEventsTotal,

    /// The total number of event bytes emitted by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    ComponentSentEventBytesTotal,

    /// The number of raw bytes sent by this component to destination sinks.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "endpoint": {"description": "The endpoint to which the bytes were sent. For HTTP, this will be the host and path only, excluding the query string.", "required": false},
        "file": {"description": "The absolute path of the destination file.", "required": false},
        "protocol": {"description": "The protocol used to send the bytes.", "required": true},
        "region": {"description": "The AWS region name to which the bytes were sent. In some configurations, this may be a literal hostname.", "required": false}
    }))))]
    ComponentSentBytesTotal,

    /// The number of events dropped by this component.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "intentional": {"description": "True if the events were discarded intentionally, like a `filter` transform, or false if due to an error.", "required": true}
    }))))]
    ComponentDiscardedEventsTotal,

    /// The total number of errors encountered by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_ERROR_TYPE_STAGE))]
    ComponentErrorsTotal,

    /// The total number of events for which this source responded with a timeout error.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ComponentTimedOutEventsTotal,

    /// The total number of requests for which this source responded with a timeout error.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ComponentTimedOutRequestsTotal,

    /// The number of events received by this buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferReceivedEventsTotal,

    /// The number of bytes received by this buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferReceivedBytesTotal,

    /// The number of events sent by this buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferSentEventsTotal,

    /// The number of bytes sent by this buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferSentBytesTotal,

    /// The number of events dropped by this non-blocking buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferDiscardedEventsTotal,

    /// The number of bytes dropped by this non-blocking buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferDiscardedBytesTotal,

    /// The total number of buffer errors encountered.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferErrorsTotal,

    // Internal events from src/internal_events/
    /// The number of events recorded by the aggregate transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AggregateEventsRecordedTotal,

    /// The number of failed metric updates, `incremental` adds, encountered by the aggregate transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AggregateFailedUpdates,

    /// The number of flushes done by the aggregate transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AggregateFlushesTotal,

    /// The number of times the Vector API has been started.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ApiStartedTotal,

    /// The total number of files checkpointed.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    CheckpointsTotal,

    /// The total number of errors identifying files via checksum.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_FILE))]
    ChecksumErrorsTotal,

    /// The total number of metrics collections completed for this component.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    CollectCompletedTotal,

    /// The total number of times a command has been executed.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    CommandExecutedTotal,

    /// The total number of times a connection has been established.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ConnectionEstablishedTotal,

    /// The total number of errors sending data via the connection.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ConnectionSendErrorsTotal,

    /// The total number of times the connection has been shut down.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ConnectionShutdownTotal,

    /// The total number of container events processed.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ContainerProcessedEventsTotal,

    /// The total number of times Vector stopped watching for container logs.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ContainersUnwatchedTotal,

    /// The total number of times Vector started watching for container logs.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ContainersWatchedTotal,

    /// The total number of byte order marks (BOM) removed from incoming data.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    DecoderBomRemovalsTotal,

    /// The total number of warnings when replacing malformed characters during decoding.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    DecoderMalformedReplacementWarningsTotal,

    /// The total number of bytes loaded into Doris.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    DorisBytesLoadedTotal,

    /// The total number of rows filtered by Doris during stream load.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    DorisRowsFilteredTotal,

    /// The total number of rows successfully loaded into Doris.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    DorisRowsLoadedTotal,

    /// The total number of warnings when replacing unmappable characters during encoding.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    EncoderUnmappableReplacementWarningsTotal,

    /// The total number of events discarded by this component.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_REASON))]
    EventsDiscardedTotal,

    /// The total number of files Vector has found to watch.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_FILE))]
    FilesAddedTotal,

    /// The total number of files deleted.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_FILE))]
    FilesDeletedTotal,

    /// The total number of times Vector has resumed watching a file.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_FILE))]
    FilesResumedTotal,

    /// The total number of times Vector has stopped watching a file.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_FILE))]
    FilesUnwatchedTotal,

    /// The total number of gRPC messages received.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_GRPC_METHOD_SERVICE))]
    GrpcServerMessagesReceivedTotal,

    /// The total number of gRPC messages sent.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_GRPC_ALL))]
    GrpcServerMessagesSentTotal,

    /// The total number of HTTP client errors encountered.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    HttpClientErrorsTotal,

    /// The total number of sent HTTP requests, tagged with the request method.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_METHOD))]
    HttpClientRequestsSentTotal,

    /// The total number of HTTP requests, tagged with the response code.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_STATUS))]
    HttpClientResponsesTotal,

    /// The total number of HTTP requests received.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_METHOD_PATH))]
    HttpServerRequestsReceivedTotal,

    /// The total number of HTTP responses sent.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_ALL))]
    HttpServerResponsesSentTotal,

    /// Total number of message bytes (including framing) received from Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaConsumedMessagesBytesTotal,

    /// Total number of messages consumed, not including ignored messages (due to offset, etc), from Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaConsumedMessagesTotal,

    /// Total number of message bytes (including framing, such as per-Message framing and MessageSet/batch framing) transmitted to Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaProducedMessagesBytesTotal,

    /// Total number of messages transmitted (produced) to Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaProducedMessagesTotal,

    /// Total number of bytes transmitted to Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaRequestsBytesTotal,

    /// Total number of requests sent to Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaRequestsTotal,

    /// Total number of bytes received from Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaResponsesBytesTotal,

    /// Total number of responses received from Kafka brokers.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaResponsesTotal,

    /// The total number of failed efforts to refresh AWS EC2 metadata.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MetadataRefreshFailedTotal,

    /// The total number of AWS EC2 metadata refreshes.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MetadataRefreshSuccessfulTotal,

    /// The total number of errors encountered while parsing.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ParseErrorsTotal,

    /// The total number of times the Vector instance has quit.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    QuitTotal,

    /// The total number of times the Vector instance has been reloaded.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ReloadedTotal,

    /// The total number of events with rewrapped timestamps.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    RewrittenTimestampEventsTotal,

    /// The total number of successful deferrals of SQS messages.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SqsMessageDeferSucceededTotal,

    /// The total number of successful deletions of SQS messages.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SqsMessageDeleteSucceededTotal,

    /// The total number of SQS messages successfully processed.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SqsMessageProcessingSucceededTotal,

    /// The total number of times successfully receiving SQS messages.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SqsMessageReceiveSucceededTotal,

    /// The total number of received SQS messages.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SqsMessageReceivedMessagesTotal,

    /// The number of stale events that Vector has flushed.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    StaleEventsFlushedTotal,

    /// The total number of times the Vector instance has been started.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    StartedTotal,

    /// The total number of times the Vector instance has been stopped.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    StoppedTotal,

    /// The total number of events that contained a tag which exceeded the configured cardinality limit.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    TagCardinalityUntrackedEventsTotal,

    /// The total number of events discarded because the tag has been rejected after hitting the configured `value_limit`.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "metric_name": {"description": "The name of the metric whose tag value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.", "required": false},
        "tag_key": {"description": "The key of the tag whose value limit was exceeded. Only present when `internal_metrics.include_extended_tags` is enabled.", "required": false}
    }))))]
    TagValueLimitExceededTotal,

    /// The total number of times the value limit was reached.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ValueLimitReachedTotal,

    /// The total number of bytes sent over WebSocket connections.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    WebsocketBytesSentTotal,

    /// The total number of messages sent over WebSocket connections.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    WebsocketMessagesSentTotal,

    /// The total number of times the Windows service has been installed.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    WindowsServiceInstallTotal,

    /// The total number of times the Windows service has been restarted.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    WindowsServiceRestartTotal,

    /// The total number of times the Windows service has been started.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    WindowsServiceStartTotal,

    /// The total number of times the Windows service has been stopped.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    WindowsServiceStopTotal,

    /// The total number of times the Windows service has been uninstalled.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    WindowsServiceUninstallTotal,

    /// The total number of failures to annotate Kubernetes events with namespace metadata.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    K8sEventNamespaceAnnotationFailuresTotal,

    /// The total number of failures to annotate Kubernetes events with node metadata.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    K8sEventNodeAnnotationFailuresTotal,

    /// The total number of edge cases encountered while picking format of the Kubernetes log message.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    K8sFormatPickerEdgeCasesTotal,

    /// The total number of failures to parse a message as a JSON object.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    K8sDockerFormatParseFailuresTotal,

    /// The total number of times an S3 record in an SQS message was ignored.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "ignore_type": {
            "description": "The reason for ignoring the S3 record",
            "required": true,
            "enum": {"invalid_event_kind": "The kind of invalid event."}
        }
    }))))]
    SqsS3EventRecordIgnoredTotal,

    /// The total number of bytes allocated by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ComponentAllocatedBytesTotal,

    /// The total number of bytes deallocated by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ComponentDeallocatedBytesTotal,

    /// The total number of failed insertions into the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableFailedInsertions,

    /// The total number of failed reads from the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableFailedReads,

    /// The total number of flushes of the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableFlushesTotal,

    /// The total number of successful insertions into the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableInsertionsTotal,

    /// The total number of successful reads from the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableReadsTotal,

    /// The total number of entries evicted from the in-memory enrichment table due to TTL expiration.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableTtlExpirations,

    /// The total number of errors reading datagram.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "mode": {"description": "", "required": true, "enum": {"udp": "User Datagram Protocol"}}
    }))))]
    ConnectionReadErrorsTotal,

    /// The total number of metrics emitted from the internal metrics registry. This metric is deprecated in favor of `internal_metrics_cardinality`.
    #[configurable(metadata(docs::tags = "{}"))]
    InternalMetricsCardinalityTotal,

    /// Number of configuration reload attempts that were rejected.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS_REASON))]
    ConfigReloadRejected,

    /// The total number of errors converting bytes to a UTF-8 string in UDP mode.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "mode": {"description": "The connection mode used by the component.", "required": true, "enum": {"udp": "User Datagram Protocol"}}
    }))))]
    Utf8ConvertErrorsTotal,
}

/// Canonical list of all per-component internal histogram metric names emitted by Vector.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, AsRefStr, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum HistogramName {
    /// A histogram of the number of events passed in each internal batch in Vector's internal topology.
    ///
    /// Note that this is separate than sink-level batching. It is mostly useful for low level
    /// debugging performance issues in Vector due to small internal batches.
    #[configurable(metadata(docs::tags = &*COMPONENT_RECEIVED_EVENTS_TAGS))]
    ComponentReceivedEventsCount,

    /// The size in bytes of each event received by the source.
    #[configurable(metadata(docs::tags = &*COMPONENT_RECEIVED_EVENTS_TAGS))]
    ComponentReceivedBytes,

    /// The duration spent sending a payload to this buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferSendDurationSeconds,

    /// The elapsed time, in fractional seconds, that an event spends in a single transform.
    ///
    /// This includes both the time spent queued in the transform's input buffer and the time spent
    /// executing the transform itself.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ComponentLatencySeconds,

    /// The difference between the timestamp recorded in each event and the time when it was ingested, expressed as fractional seconds.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SourceLagTimeSeconds,

    /// The time elapsed blocking on the downstream channel to accept a single chunk from a batch of events received at the source.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SourceSendLatencySeconds,

    /// The time elapsed blocking on the downstream channel to accept an entire batch of events received at the source.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SourceSendBatchLatencySeconds,

    /// The average round-trip time (RTT) for the current window.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyAveragedRtt,

    /// The amount of back pressure on the current component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyBackPressure,

    /// The number of outbound requests currently awaiting a response.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyInFlight,

    /// The concurrency limit that the adaptive concurrency feature has decided on for this current window.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyLimit,

    /// The observed round-trip time (RTT) for requests.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyObservedRtt,

    /// The mean round-trip time (RTT) for the current window.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyPastRttMean,

    /// The number of times the concurrency limit was reached.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    AdaptiveConcurrencyReachedLimit,

    /// The time taken to process an S3 object that succeeded, in seconds.
    #[configurable(metadata(docs::tags = &*S3_OBJECT_PROCESSING_TAGS))]
    S3ObjectProcessingSucceededDurationSeconds,

    /// The time taken to process an S3 object that failed, in seconds.
    #[configurable(metadata(docs::tags = &*S3_OBJECT_PROCESSING_TAGS))]
    S3ObjectProcessingFailedDurationSeconds,

    /// The duration spent collecting metrics for this component.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    CollectDurationSeconds,

    /// The command execution duration in seconds.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    CommandExecutionDurationSeconds,

    /// The duration spent handling a gRPC request.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_GRPC_ALL))]
    GrpcServerHandlerDurationSeconds,

    /// The duration spent handling an HTTP request.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_ALL))]
    HttpServerHandlerDurationSeconds,

    /// The round-trip time (RTT) of HTTP requests.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    HttpClientRttSeconds,

    /// The round-trip time (RTT) of HTTP requests, tagged with the response code.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_HTTP_STATUS))]
    HttpClientResponseRttSeconds,

    /// The round-trip time (RTT) of HTTP requests that resulted in an error.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    HttpClientErrorRttSeconds,

    /// The utilization of the source buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferUtilization,

    /// The utilization of the buffer that feeds into a transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferUtilization,
}

impl HistogramName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ComponentReceivedEventsCount => "component_received_events_count",
            Self::ComponentReceivedBytes => "component_received_bytes",
            Self::BufferSendDurationSeconds => "buffer_send_duration_seconds",
            Self::ComponentLatencySeconds => "component_latency_seconds",
            Self::SourceLagTimeSeconds => "source_lag_time_seconds",
            Self::SourceSendLatencySeconds => "source_send_latency_seconds",
            Self::SourceSendBatchLatencySeconds => "source_send_batch_latency_seconds",
            Self::AdaptiveConcurrencyAveragedRtt => "adaptive_concurrency_averaged_rtt",
            Self::AdaptiveConcurrencyBackPressure => "adaptive_concurrency_back_pressure",
            Self::AdaptiveConcurrencyInFlight => "adaptive_concurrency_in_flight",
            Self::AdaptiveConcurrencyLimit => "adaptive_concurrency_limit",
            Self::AdaptiveConcurrencyObservedRtt => "adaptive_concurrency_observed_rtt",
            Self::AdaptiveConcurrencyPastRttMean => "adaptive_concurrency_past_rtt_mean",
            Self::AdaptiveConcurrencyReachedLimit => "adaptive_concurrency_reached_limit",
            Self::S3ObjectProcessingSucceededDurationSeconds => {
                "s3_object_processing_succeeded_duration_seconds"
            }
            Self::S3ObjectProcessingFailedDurationSeconds => {
                "s3_object_processing_failed_duration_seconds"
            }
            Self::CollectDurationSeconds => "collect_duration_seconds",
            Self::CommandExecutionDurationSeconds => "command_execution_duration_seconds",
            Self::GrpcServerHandlerDurationSeconds => "grpc_server_handler_duration_seconds",
            Self::HttpServerHandlerDurationSeconds => "http_server_handler_duration_seconds",
            Self::HttpClientRttSeconds => "http_client_rtt_seconds",
            Self::HttpClientResponseRttSeconds => "http_client_response_rtt_seconds",
            Self::HttpClientErrorRttSeconds => "http_client_error_rtt_seconds",
            Self::SourceBufferUtilization => "source_buffer_utilization",
            Self::TransformBufferUtilization => "transform_buffer_utilization",
        }
    }
}

/// Canonical list of all per-component internal gauge metric names emitted by Vector.
#[configurable_component]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, AsRefStr, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GaugeName {
    /// The mean elapsed time, in fractional seconds, that an event spends in a single transform.
    ///
    /// This includes both the time spent queued in the transform's input buffer and the time spent
    /// executing the transform itself. This value is smoothed over time using an exponentially
    /// weighted moving average (EWMA).
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    ComponentLatencyMeanSeconds,

    /// The maximum number of events the source buffer can hold.
    #[configurable(
        deprecated = "This metric has been deprecated in favor of `source_buffer_max_size_events`."
    )]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferMaxEventSize,

    /// The maximum number of bytes the source buffer can hold.
    #[configurable(
        deprecated = "This metric has been deprecated in favor of `source_buffer_max_size_bytes`."
    )]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferMaxByteSize,

    /// The maximum number of events the source buffer can hold.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferMaxSizeEvents,

    /// The maximum number of bytes the source buffer can hold.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferMaxSizeBytes,

    /// The current utilization level of the source buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferUtilizationLevel,

    /// The mean utilization of the source buffer, smoothed with an EWMA.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    SourceBufferUtilizationMean,

    /// The maximum number of events the buffer that feeds into a transform can hold.
    #[configurable(
        deprecated = "This metric has been deprecated in favor of `transform_buffer_max_size_events`."
    )]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferMaxEventSize,

    /// The maximum number of bytes the buffer that feeds into a transform can hold.
    #[configurable(
        deprecated = "This metric has been deprecated in favor of `transform_buffer_max_size_bytes`."
    )]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferMaxByteSize,

    /// The maximum number of events the buffer that feeds into a transform can hold.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferMaxSizeEvents,

    /// The maximum number of bytes the buffer that feeds into a transform can hold.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferMaxSizeBytes,

    /// The current utilization level of the buffer that feeds into a transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferUtilizationLevel,

    /// The mean utilization of the buffer that feeds into a transform, smoothed with an EWMA.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS_OUTPUT))]
    TransformBufferUtilizationMean,

    /// The maximum number of events in the buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferMaxSizeEvents,

    /// The maximum size in events that the buffer can store.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferMaxEventSize,

    /// The maximum number of bytes in the buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferMaxSizeBytes,

    /// The maximum size in bytes that the buffer can store.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferMaxByteSize,

    /// The number of events currently in the buffer.
    #[configurable(
        deprecated = "This metric has been deprecated in favor of `buffer_size_events`."
    )]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferEvents,

    /// The number of events currently in the buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferSizeEvents,

    /// The number of bytes currently in the buffer.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferSizeBytes,

    /// The number of bytes currently in the buffer.
    #[configurable(deprecated = "This metric has been deprecated in favor of `buffer_size_bytes`.")]
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    BufferByteSize,

    /// The current utilization of this component, expressed as a value from 0 to 1.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    Utilization,

    /// The number of bytes currently allocated by this component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ComponentAllocatedBytes,

    /// The total number of open files.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    OpenFiles,

    /// The number of seconds the Vector instance has been running.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    UptimeSeconds,

    /// Pseudo-metric that provides build information for the Vector instance.
    #[configurable(metadata(docs::tags = merge_lazy(&INTERNAL_METRICS_TAGS, json!({
        "debug": {"description": "Whether this is a debug build of Vector", "required": true},
        "version": {"description": "Vector version.", "required": true},
        "rust_version": {"description": "The Rust version from the package manifest.", "required": true},
        "arch": {"description": "The target architecture being compiled for. (e.g. x86_64)", "required": true},
        "revision": {"description": "Revision identifer, related to versioned releases.", "required": true}
    }))))]
    BuildInfo,

    /// Current number of messages in producer queues.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaQueueMessages,

    /// Current total size of messages in producer queues.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    KafkaQueueMessagesBytes,

    /// The Kafka consumer lag.
    #[configurable(metadata(docs::tags = merge_lazy(&COMPONENT_TAGS, json!({
        "topic_id": {"description": "The Kafka topic id.", "required": true},
        "partition_id": {"description": "The Kafka partition id.", "required": true}
    }))))]
    KafkaConsumerLag,

    /// The total memory currently being used by the Lua runtime.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    LuaMemoryUsedBytes,

    /// The number of current open connections to Vector.
    #[configurable(metadata(docs::tags = &*INTERNAL_METRICS_TAGS))]
    OpenConnections,

    /// The number of currently active endpoints.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ActiveEndpoints,

    /// The number of outstanding Splunk HEC indexer acknowledgement acks.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    SplunkPendingAcks,

    /// Number of clients attached to a component.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    ActiveClients,

    /// The number of objects currently stored in the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableObjectsCount,

    /// The total size in bytes of all objects stored in the in-memory enrichment table.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    MemoryEnrichmentTableByteSize,

    /// The number of tag keys currently being tracked by the tag cardinality limit transform.
    #[configurable(metadata(docs::tags = &*COMPONENT_TAGS))]
    TagCardinalityTrackedKeys,

    /// The total number of metrics emitted from the internal metrics registry.
    #[configurable(metadata(docs::tags = "{}"))]
    InternalMetricsCardinality,
}

impl GaugeName {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ComponentLatencyMeanSeconds => "component_latency_mean_seconds",
            Self::SourceBufferMaxSizeEvents => "source_buffer_max_size_events",
            Self::SourceBufferMaxSizeBytes => "source_buffer_max_size_bytes",
            Self::SourceBufferMaxEventSize => "source_buffer_max_event_size",
            Self::SourceBufferMaxByteSize => "source_buffer_max_byte_size",
            Self::SourceBufferUtilizationLevel => "source_buffer_utilization_level",
            Self::SourceBufferUtilizationMean => "source_buffer_utilization_mean",
            Self::TransformBufferMaxSizeEvents => "transform_buffer_max_size_events",
            Self::TransformBufferMaxSizeBytes => "transform_buffer_max_size_bytes",
            Self::TransformBufferMaxEventSize => "transform_buffer_max_event_size",
            Self::TransformBufferMaxByteSize => "transform_buffer_max_byte_size",
            Self::TransformBufferUtilizationLevel => "transform_buffer_utilization_level",
            Self::TransformBufferUtilizationMean => "transform_buffer_utilization_mean",
            Self::BufferMaxSizeEvents => "buffer_max_size_events",
            Self::BufferMaxEventSize => "buffer_max_event_size",
            Self::BufferMaxSizeBytes => "buffer_max_size_bytes",
            Self::BufferMaxByteSize => "buffer_max_byte_size",
            Self::BufferEvents => "buffer_events",
            Self::BufferSizeEvents => "buffer_size_events",
            Self::BufferSizeBytes => "buffer_size_bytes",
            Self::BufferByteSize => "buffer_byte_size",
            Self::Utilization => "utilization",
            Self::ComponentAllocatedBytes => "component_allocated_bytes",
            Self::OpenFiles => "open_files",
            Self::UptimeSeconds => "uptime_seconds",
            Self::BuildInfo => "build_info",
            Self::KafkaQueueMessages => "kafka_queue_messages",
            Self::KafkaQueueMessagesBytes => "kafka_queue_messages_bytes",
            Self::KafkaConsumerLag => "kafka_consumer_lag",
            Self::LuaMemoryUsedBytes => "lua_memory_used_bytes",
            Self::OpenConnections => "open_connections",
            Self::ActiveEndpoints => "active_endpoints",
            Self::SplunkPendingAcks => "splunk_pending_acks",
            Self::ActiveClients => "active_clients",
            Self::MemoryEnrichmentTableObjectsCount => "memory_enrichment_table_objects_count",
            Self::MemoryEnrichmentTableByteSize => "memory_enrichment_table_byte_size",
            Self::TagCardinalityTrackedKeys => "tag_cardinality_tracked_keys",
            Self::InternalMetricsCardinality => "internal_metrics_cardinality",
        }
    }
}

impl CounterName {
    #[allow(clippy::too_many_lines)]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ComponentReceivedEventsTotal => "component_received_events_total",
            Self::ComponentReceivedEventBytesTotal => "component_received_event_bytes_total",
            Self::ComponentReceivedBytesTotal => "component_received_bytes_total",
            Self::ComponentSentEventsTotal => "component_sent_events_total",
            Self::ComponentSentEventBytesTotal => "component_sent_event_bytes_total",
            Self::ComponentSentBytesTotal => "component_sent_bytes_total",
            Self::ComponentDiscardedEventsTotal => "component_discarded_events_total",
            Self::ComponentErrorsTotal => "component_errors_total",
            Self::ComponentTimedOutEventsTotal => "component_timed_out_events_total",
            Self::ComponentTimedOutRequestsTotal => "component_timed_out_requests_total",
            Self::BufferReceivedEventsTotal => "buffer_received_events_total",
            Self::BufferReceivedBytesTotal => "buffer_received_bytes_total",
            Self::BufferSentEventsTotal => "buffer_sent_events_total",
            Self::BufferSentBytesTotal => "buffer_sent_bytes_total",
            Self::BufferDiscardedEventsTotal => "buffer_discarded_events_total",
            Self::BufferDiscardedBytesTotal => "buffer_discarded_bytes_total",
            Self::BufferErrorsTotal => "buffer_errors_total",
            Self::AggregateEventsRecordedTotal => "aggregate_events_recorded_total",
            Self::AggregateFailedUpdates => "aggregate_failed_updates",
            Self::AggregateFlushesTotal => "aggregate_flushes_total",
            Self::ApiStartedTotal => "api_started_total",
            Self::CheckpointsTotal => "checkpoints_total",
            Self::ChecksumErrorsTotal => "checksum_errors_total",
            Self::CollectCompletedTotal => "collect_completed_total",
            Self::CommandExecutedTotal => "command_executed_total",
            Self::ConnectionEstablishedTotal => "connection_established_total",
            Self::ConnectionSendErrorsTotal => "connection_send_errors_total",
            Self::ConnectionShutdownTotal => "connection_shutdown_total",
            Self::ContainerProcessedEventsTotal => "container_processed_events_total",
            Self::ContainersUnwatchedTotal => "containers_unwatched_total",
            Self::ContainersWatchedTotal => "containers_watched_total",
            Self::DecoderBomRemovalsTotal => "decoder_bom_removals_total",
            Self::DecoderMalformedReplacementWarningsTotal => {
                "decoder_malformed_replacement_warnings_total"
            }
            Self::DorisBytesLoadedTotal => "doris_bytes_loaded_total",
            Self::DorisRowsFilteredTotal => "doris_rows_filtered_total",
            Self::DorisRowsLoadedTotal => "doris_rows_loaded_total",
            Self::EncoderUnmappableReplacementWarningsTotal => {
                "encoder_unmappable_replacement_warnings_total"
            }
            Self::EventsDiscardedTotal => "events_discarded_total",
            Self::FilesAddedTotal => "files_added_total",
            Self::FilesDeletedTotal => "files_deleted_total",
            Self::FilesResumedTotal => "files_resumed_total",
            Self::FilesUnwatchedTotal => "files_unwatched_total",
            Self::GrpcServerMessagesReceivedTotal => "grpc_server_messages_received_total",
            Self::GrpcServerMessagesSentTotal => "grpc_server_messages_sent_total",
            Self::HttpClientErrorsTotal => "http_client_errors_total",
            Self::HttpClientRequestsSentTotal => "http_client_requests_sent_total",
            Self::HttpClientResponsesTotal => "http_client_responses_total",
            Self::HttpServerRequestsReceivedTotal => "http_server_requests_received_total",
            Self::HttpServerResponsesSentTotal => "http_server_responses_sent_total",
            Self::KafkaConsumedMessagesBytesTotal => "kafka_consumed_messages_bytes_total",
            Self::KafkaConsumedMessagesTotal => "kafka_consumed_messages_total",
            Self::KafkaProducedMessagesBytesTotal => "kafka_produced_messages_bytes_total",
            Self::KafkaProducedMessagesTotal => "kafka_produced_messages_total",
            Self::KafkaRequestsBytesTotal => "kafka_requests_bytes_total",
            Self::KafkaRequestsTotal => "kafka_requests_total",
            Self::KafkaResponsesBytesTotal => "kafka_responses_bytes_total",
            Self::KafkaResponsesTotal => "kafka_responses_total",
            Self::MetadataRefreshFailedTotal => "metadata_refresh_failed_total",
            Self::MetadataRefreshSuccessfulTotal => "metadata_refresh_successful_total",
            Self::ParseErrorsTotal => "parse_errors_total",
            Self::QuitTotal => "quit_total",
            Self::ReloadedTotal => "reloaded_total",
            Self::RewrittenTimestampEventsTotal => "rewritten_timestamp_events_total",
            Self::SqsMessageDeferSucceededTotal => "sqs_message_defer_succeeded_total",
            Self::SqsMessageDeleteSucceededTotal => "sqs_message_delete_succeeded_total",
            Self::SqsMessageProcessingSucceededTotal => "sqs_message_processing_succeeded_total",
            Self::SqsMessageReceiveSucceededTotal => "sqs_message_receive_succeeded_total",
            Self::SqsMessageReceivedMessagesTotal => "sqs_message_received_messages_total",
            Self::StaleEventsFlushedTotal => "stale_events_flushed_total",
            Self::StartedTotal => "started_total",
            Self::StoppedTotal => "stopped_total",
            Self::TagCardinalityUntrackedEventsTotal => "tag_cardinality_untracked_events_total",
            Self::TagValueLimitExceededTotal => "tag_value_limit_exceeded_total",
            Self::ValueLimitReachedTotal => "value_limit_reached_total",
            Self::WebsocketBytesSentTotal => "websocket_bytes_sent_total",
            Self::WebsocketMessagesSentTotal => "websocket_messages_sent_total",
            Self::WindowsServiceInstallTotal => "windows_service_install_total",
            Self::WindowsServiceRestartTotal => "windows_service_restart_total",
            Self::WindowsServiceStartTotal => "windows_service_start_total",
            Self::WindowsServiceStopTotal => "windows_service_stop_total",
            Self::WindowsServiceUninstallTotal => "windows_service_uninstall_total",
            Self::K8sEventNamespaceAnnotationFailuresTotal => {
                "k8s_event_namespace_annotation_failures_total"
            }
            Self::K8sEventNodeAnnotationFailuresTotal => "k8s_event_node_annotation_failures_total",
            Self::K8sFormatPickerEdgeCasesTotal => "k8s_format_picker_edge_cases_total",
            Self::K8sDockerFormatParseFailuresTotal => "k8s_docker_format_parse_failures_total",
            Self::SqsS3EventRecordIgnoredTotal => "sqs_s3_event_record_ignored_total",
            Self::ComponentAllocatedBytesTotal => "component_allocated_bytes_total",
            Self::ComponentDeallocatedBytesTotal => "component_deallocated_bytes_total",
            Self::MemoryEnrichmentTableFailedInsertions => {
                "memory_enrichment_table_failed_insertions"
            }
            Self::MemoryEnrichmentTableFailedReads => "memory_enrichment_table_failed_reads",
            Self::MemoryEnrichmentTableFlushesTotal => "memory_enrichment_table_flushes_total",
            Self::MemoryEnrichmentTableInsertionsTotal => {
                "memory_enrichment_table_insertions_total"
            }
            Self::MemoryEnrichmentTableReadsTotal => "memory_enrichment_table_reads_total",
            Self::MemoryEnrichmentTableTtlExpirations => "memory_enrichment_table_ttl_expirations",
            Self::ConnectionReadErrorsTotal => "connection_read_errors_total",
            Self::InternalMetricsCardinalityTotal => "internal_metrics_cardinality_total",
            Self::ConfigReloadRejected => "config_reload_rejected",
            Self::Utf8ConvertErrorsTotal => "utf8_convert_errors_total",
        }
    }
}
