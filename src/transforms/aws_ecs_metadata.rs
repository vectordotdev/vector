use std::{
    collections::HashSet,
    env, error, fmt,
    future::ready,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use arc_swap::ArcSwap;
use futures::{Stream, StreamExt};
use http::{Request, StatusCode};
use hyper::Body;
use serde_json::Value as JsonValue;
use serde_with::serde_as;
use snafu::ResultExt as _;
use tokio::time::{Duration, sleep};
use tracing::Instrument;
use vector_lib::{
    configurable::configurable_component,
    lookup::{
        OwnedTargetPath,
        lookup_v2::{OptionalTargetPath, OwnedSegment},
        owned_value_path,
    },
};
use vrl::value::{Kind, Value, kind::Collection};

use crate::{
    config::{
        DataType, Input, OutputId, ProxyConfig, TransformConfig, TransformContext, TransformOutput,
    },
    event::Event,
    http::HttpClient,
    internal_events::{AwsEcsMetadataRefreshError, AwsEcsMetadataRefreshSuccessful},
    schema,
    transforms::{TaskTransform, Transform},
};

const METADATA_URI_V4_ENV: &str = "ECS_CONTAINER_METADATA_URI_V4";

const CLUSTER_KEY: &str = "cluster";
const SERVICE_NAME_KEY: &str = "service-name";
const VPC_ID_KEY: &str = "vpc-id";
const TASK_ARN_KEY: &str = "task-arn";
const FAMILY_KEY: &str = "family";
const REVISION_KEY: &str = "revision";
const DESIRED_STATUS_KEY: &str = "desired-status";
const KNOWN_STATUS_KEY: &str = "known-status";
const PULL_STARTED_AT_KEY: &str = "pull-started-at";
const PULL_STOPPED_AT_KEY: &str = "pull-stopped-at";
const AVAILABILITY_ZONE_KEY: &str = "availability-zone";
const LAUNCH_TYPE_KEY: &str = "launch-type";
const EXECUTION_STOPPED_AT_KEY: &str = "execution-stopped-at";
const FAULT_INJECTION_ENABLED_KEY: &str = "fault-injection-enabled";
const CONTAINER_ID_KEY: &str = "container-id";
const CONTAINER_NAME_KEY: &str = "container-name";
const DOCKER_NAME_KEY: &str = "docker-name";
const CONTAINER_ARN_KEY: &str = "container-arn";
const IMAGE_KEY: &str = "image";
const IMAGE_ID_KEY: &str = "image-id";
const CONTAINER_DESIRED_STATUS_KEY: &str = "container-desired-status";
const CONTAINER_KNOWN_STATUS_KEY: &str = "container-known-status";
const CONTAINER_EXIT_CODE_KEY: &str = "container-exit-code";
const CONTAINER_CREATED_AT_KEY: &str = "container-created-at";
const CONTAINER_STARTED_AT_KEY: &str = "container-started-at";
const CONTAINER_FINISHED_AT_KEY: &str = "container-finished-at";
const CONTAINER_TYPE_KEY: &str = "container-type";
const LOG_DRIVER_KEY: &str = "log-driver";
const SNAPSHOTTER_KEY: &str = "snapshotter";
const RESTART_COUNT_KEY: &str = "restart-count";

static DEFAULT_FIELD_ALLOWLIST: &[&str] = &[
    CLUSTER_KEY,
    TASK_ARN_KEY,
    FAMILY_KEY,
    REVISION_KEY,
    SERVICE_NAME_KEY,
    LAUNCH_TYPE_KEY,
    AVAILABILITY_ZONE_KEY,
    CONTAINER_NAME_KEY,
    CONTAINER_ID_KEY,
    CONTAINER_ARN_KEY,
    IMAGE_KEY,
    IMAGE_ID_KEY,
];

static FIELD_CATALOG: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        CLUSTER_KEY,
        SERVICE_NAME_KEY,
        VPC_ID_KEY,
        TASK_ARN_KEY,
        FAMILY_KEY,
        REVISION_KEY,
        DESIRED_STATUS_KEY,
        KNOWN_STATUS_KEY,
        PULL_STARTED_AT_KEY,
        PULL_STOPPED_AT_KEY,
        AVAILABILITY_ZONE_KEY,
        LAUNCH_TYPE_KEY,
        EXECUTION_STOPPED_AT_KEY,
        FAULT_INJECTION_ENABLED_KEY,
        CONTAINER_ID_KEY,
        CONTAINER_NAME_KEY,
        DOCKER_NAME_KEY,
        CONTAINER_ARN_KEY,
        IMAGE_KEY,
        IMAGE_ID_KEY,
        CONTAINER_DESIRED_STATUS_KEY,
        CONTAINER_KNOWN_STATUS_KEY,
        CONTAINER_EXIT_CODE_KEY,
        CONTAINER_CREATED_AT_KEY,
        CONTAINER_STARTED_AT_KEY,
        CONTAINER_FINISHED_AT_KEY,
        CONTAINER_TYPE_KEY,
        LOG_DRIVER_KEY,
        SNAPSHOTTER_KEY,
        RESTART_COUNT_KEY,
    ]
    .into_iter()
    .collect()
});

/// Configuration for the `aws_ecs_metadata` transform.
#[serde_as]
#[configurable_component(transform(
    "aws_ecs_metadata",
    "Enrich events with AWS ECS task metadata."
))]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct EcsMetadata {
    /// Overrides the ECS task metadata endpoint.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "http://169.254.170.2/v4/example"))]
    endpoint: Option<String>,

    /// The name of the container to enrich events with.
    ///
    /// If unset, the transform uses the current container's name from the ECS task metadata endpoint.
    #[serde(default)]
    #[configurable(metadata(docs::examples = "vector", docs::examples = "app"))]
    container_name: Option<String>,

    /// Sets a prefix for all event fields added by the transform.
    #[serde(default = "default_namespace")]
    #[derivative(Default(value = "default_namespace()"))]
    #[configurable(metadata(
        docs::examples = "",
        docs::examples = "ecs",
        docs::examples = "aws.ecs",
    ))]
    namespace: Option<OptionalTargetPath>,

    /// The interval between querying for updated metadata, in seconds.
    #[serde(default = "default_refresh_interval_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[derivative(Default(value = "default_refresh_interval_secs()"))]
    refresh_interval_secs: Duration,

    /// A list of metadata fields to include in each transformed event.
    #[serde(default = "default_fields")]
    #[derivative(Default(value = "default_fields()"))]
    #[configurable(metadata(docs::examples = "task-arn", docs::examples = "container-name",))]
    fields: Vec<String>,

    /// The timeout for querying the ECS metadata endpoint, in seconds.
    #[serde(default = "default_refresh_timeout_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[derivative(Default(value = "default_refresh_timeout_secs()"))]
    refresh_timeout_secs: Duration,

    /// The number of initial metadata refresh attempts before the transform starts.
    #[serde(default = "default_initial_retry_attempts")]
    #[derivative(Default(value = "default_initial_retry_attempts()"))]
    initial_retry_attempts: usize,

    /// The delay between initial metadata refresh attempts, in seconds.
    #[serde(default = "default_initial_retry_backoff_secs")]
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[derivative(Default(value = "default_initial_retry_backoff_secs()"))]
    initial_retry_backoff_secs: Duration,

    #[configurable(derived)]
    #[serde(default, skip_serializing_if = "crate::serde::is_default")]
    proxy: ProxyConfig,

    /// Requires the transform to be able to successfully query the ECS metadata before starting to process the data.
    #[serde(default = "default_required")]
    #[derivative(Default(value = "default_required()"))]
    required: bool,
}

fn default_namespace() -> Option<OptionalTargetPath> {
    Some(OwnedTargetPath::event(owned_value_path!("aws", "ecs")).into())
}

const fn default_refresh_interval_secs() -> Duration {
    Duration::from_secs(10)
}

const fn default_refresh_timeout_secs() -> Duration {
    Duration::from_secs(1)
}

const fn default_initial_retry_attempts() -> usize {
    3
}

const fn default_initial_retry_backoff_secs() -> Duration {
    Duration::from_secs(1)
}

fn default_fields() -> Vec<String> {
    DEFAULT_FIELD_ALLOWLIST
        .iter()
        .map(|s| s.to_string())
        .collect()
}

const fn default_required() -> bool {
    true
}

#[derive(Clone, Debug)]
pub struct EcsMetadataTransform {
    state: Arc<ArcSwap<Vec<(MetadataKey, Value)>>>,
}

#[derive(Debug, Clone)]
struct MetadataKey {
    log_path: OwnedTargetPath,
    metric_tag: String,
}

#[derive(Debug)]
struct Keys {
    keys: Vec<(&'static str, MetadataKey)>,
}

impl_generate_config_from_default!(EcsMetadata);

#[async_trait::async_trait]
#[typetag::serde(name = "aws_ecs_metadata")]
impl TransformConfig for EcsMetadata {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        validate_fields(&self.fields)?;

        let state = Arc::new(ArcSwap::new(Arc::new(vec![])));
        let keys = Keys::new(self.namespace.clone(), &self.fields);
        let endpoint = resolve_endpoint(self.endpoint.clone());
        let refresh_interval = self.refresh_interval_secs;
        let refresh_timeout = self.refresh_timeout_secs;
        let initial_retry_attempts = self.initial_retry_attempts.max(1);
        let initial_retry_backoff = self.initial_retry_backoff_secs;
        let required = self.required;
        let container_name = self.container_name.clone();

        let proxy = ProxyConfig::merge_with_env(&context.globals.proxy, &self.proxy);
        let http_client = HttpClient::new(None, &proxy)?;

        let mut client = MetadataClient::new(
            http_client,
            endpoint,
            container_name,
            keys,
            Arc::clone(&state),
            refresh_interval,
            refresh_timeout,
        );

        if let Err(error) = client
            .refresh_metadata_with_retries(initial_retry_attempts, initial_retry_backoff)
            .await
        {
            if required {
                return Err(error);
            } else {
                emit!(AwsEcsMetadataRefreshError { error });
            }
        }

        tokio::spawn(
            async move {
                client.run().await;
            }
            .instrument(info_span!("aws_ecs_metadata: worker").or_current()),
        );

        Ok(Transform::event_task(EcsMetadataTransform { state }))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Metric | DataType::Log)
    }

    fn outputs(
        &self,
        _: &TransformContext,
        input_definitions: &[(OutputId, schema::Definition)],
    ) -> Vec<TransformOutput> {
        let added_keys = Keys::new(self.namespace.clone(), &self.fields);

        let schema_definition = input_definitions
            .iter()
            .map(|(output, definition)| {
                let mut schema_definition = definition.clone();

                if !schema_definition.event_kind().contains_object() {
                    *schema_definition.event_kind_mut() = Kind::object(Collection::empty());
                }

                for (_, key) in &added_keys.keys {
                    schema_definition = schema_definition.with_field(
                        &key.log_path,
                        Kind::bytes()
                            .or_integer()
                            .or_float()
                            .or_boolean()
                            .or_undefined(),
                        None,
                    );
                }

                (output.clone(), schema_definition)
            })
            .collect();

        vec![TransformOutput::new(
            DataType::Metric | DataType::Log,
            schema_definition,
        )]
    }
}

impl TaskTransform<Event> for EcsMetadataTransform {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut inner = self;
        Box::pin(task.filter_map(move |event| ready(Some(inner.transform_one(event)))))
    }
}

impl EcsMetadataTransform {
    fn transform_one(&mut self, mut event: Event) -> Event {
        let state = self.state.load();
        match event {
            Event::Log(ref mut log) => {
                state.iter().for_each(|(k, v)| {
                    log.insert(&k.log_path, v.clone());
                });
            }
            Event::Metric(ref mut metric) => {
                state.iter().for_each(|(k, v)| {
                    metric.replace_tag(k.metric_tag.clone(), value_to_metric_tag(v));
                });
            }
            Event::Trace(_) => panic!("Traces are not supported."),
        }
        event
    }
}

struct MetadataClient {
    client: HttpClient<Body>,
    endpoint: Option<String>,
    container_name: Option<String>,
    keys: Keys,
    state: Arc<ArcSwap<Vec<(MetadataKey, Value)>>>,
    refresh_interval: Duration,
    refresh_timeout: Duration,
}

impl MetadataClient {
    pub const fn new(
        client: HttpClient<Body>,
        endpoint: Option<String>,
        container_name: Option<String>,
        keys: Keys,
        state: Arc<ArcSwap<Vec<(MetadataKey, Value)>>>,
        refresh_interval: Duration,
        refresh_timeout: Duration,
    ) -> Self {
        Self {
            client,
            endpoint,
            container_name,
            keys,
            state,
            refresh_interval,
            refresh_timeout,
        }
    }

    async fn run(&mut self) {
        loop {
            match self.refresh_metadata().await {
                Ok(_) => {
                    emit!(AwsEcsMetadataRefreshSuccessful);
                }
                Err(error) => {
                    // Keep the last successful metadata snapshot on transient endpoint
                    // failures so enrichment does not disappear between refreshes.
                    emit!(AwsEcsMetadataRefreshError { error });
                }
            }

            sleep(self.refresh_interval).await;
        }
    }

    async fn refresh_metadata_with_retries(
        &mut self,
        attempts: usize,
        backoff: Duration,
    ) -> Result<(), crate::Error> {
        let mut remaining_attempts = attempts;
        loop {
            match self.refresh_metadata().await {
                Ok(()) => return Ok(()),
                Err(error) => {
                    remaining_attempts -= 1;
                    if remaining_attempts == 0 {
                        return Err(error);
                    }

                    emit!(AwsEcsMetadataRefreshError { error });
                    sleep(backoff).await;
                }
            }
        }
    }

    async fn refresh_metadata(&mut self) -> Result<(), crate::Error> {
        let task = self.get_metadata("/task").await?;

        let target_container_name = match self.container_name.clone() {
            Some(container_name) => container_name,
            None => {
                // `/task` supplies task-level fields, but the current container
                // endpoint is needed to select Vector's container by default.
                let current_container = self.get_metadata("").await?;
                json_string(&current_container, "Name")
                    .ok_or_else(|| crate::Error::from(MissingCurrentContainerNameError))?
            }
        };

        let container =
            find_container(&task, &target_container_name).ok_or_else(|| MissingContainerError {
                name: target_container_name.clone(),
            })?;

        let mut new_state = vec![];
        for (field, key) in &self.keys.keys {
            // ECS launch types and platform versions expose slightly different
            // v4 fields; configured fields that are valid but absent are omitted.
            if let Some(value) = extract_field(field, &task, container) {
                new_state.push((key.clone(), json_to_value(value)));
            }
        }

        self.state.store(Arc::new(new_state));
        Ok(())
    }

    async fn get_metadata(&self, suffix: &str) -> Result<JsonValue, crate::Error> {
        let endpoint = self.endpoint.as_deref().ok_or(MissingEndpointError)?;
        let url = format!("{}{}", endpoint.trim_end_matches('/'), suffix);

        debug!(message = "Sending ECS metadata request.", url);

        let req = Request::get(&url).body(Body::empty())?;

        // Headers can arrive while the response body stalls, so the refresh
        // timeout must cover sending, status validation, and body collection.
        let body = tokio::time::timeout(self.refresh_timeout, async {
            let res = self
                .client
                .send(req)
                .await
                .map_err(crate::Error::from)
                .and_then(|res| match res.status() {
                    StatusCode::OK => Ok(res),
                    status_code => Err(UnexpectedHttpStatusError {
                        status: status_code,
                    }
                    .into()),
                })?;

            let body = http_body::Body::collect(res.into_body()).await?.to_bytes();
            Ok::<_, crate::Error>(body)
        })
        .await??;

        serde_json::from_slice(&body[..])
            .context(ParseMetadataSnafu {})
            .map_err(Into::into)
    }
}

fn validate_fields(fields: &[String]) -> Result<(), crate::Error> {
    if let Some(field) = fields
        .iter()
        .find(|field| !FIELD_CATALOG.contains(field.as_str()))
    {
        return Err(UnknownFieldError {
            field: field.clone(),
        }
        .into());
    }

    Ok(())
}

fn resolve_endpoint(configured: Option<String>) -> Option<String> {
    configured.or_else(|| env::var(METADATA_URI_V4_ENV).ok())
}

fn find_container<'a>(task: &'a JsonValue, name: &str) -> Option<&'a JsonValue> {
    task.get("Containers")?
        .as_array()?
        .iter()
        .find(|container| json_string(container, "Name").as_deref() == Some(name))
}

fn extract_field<'a>(
    field: &str,
    task: &'a JsonValue,
    container: &'a JsonValue,
) -> Option<&'a JsonValue> {
    match field {
        CLUSTER_KEY => scalar(task, "Cluster"),
        SERVICE_NAME_KEY => scalar(task, "ServiceName"),
        VPC_ID_KEY => scalar(task, "VPCID"),
        TASK_ARN_KEY => scalar(task, "TaskARN"),
        FAMILY_KEY => scalar(task, "Family"),
        REVISION_KEY => scalar(task, "Revision"),
        DESIRED_STATUS_KEY => scalar(task, "DesiredStatus"),
        KNOWN_STATUS_KEY => scalar(task, "KnownStatus"),
        PULL_STARTED_AT_KEY => scalar(task, "PullStartedAt"),
        PULL_STOPPED_AT_KEY => scalar(task, "PullStoppedAt"),
        AVAILABILITY_ZONE_KEY => scalar(task, "AvailabilityZone"),
        LAUNCH_TYPE_KEY => scalar(task, "LaunchType"),
        EXECUTION_STOPPED_AT_KEY => scalar(task, "ExecutionStoppedAt"),
        FAULT_INJECTION_ENABLED_KEY => scalar(task, "FaultInjectionEnabled"),
        CONTAINER_ID_KEY => scalar(container, "DockerId"),
        CONTAINER_NAME_KEY => scalar(container, "Name"),
        DOCKER_NAME_KEY => scalar(container, "DockerName"),
        CONTAINER_ARN_KEY => scalar(container, "ContainerARN"),
        IMAGE_KEY => scalar(container, "Image"),
        IMAGE_ID_KEY => scalar(container, "ImageID"),
        CONTAINER_DESIRED_STATUS_KEY => scalar(container, "DesiredStatus"),
        CONTAINER_KNOWN_STATUS_KEY => scalar(container, "KnownStatus"),
        CONTAINER_EXIT_CODE_KEY => scalar(container, "ExitCode"),
        CONTAINER_CREATED_AT_KEY => scalar(container, "CreatedAt"),
        CONTAINER_STARTED_AT_KEY => scalar(container, "StartedAt"),
        CONTAINER_FINISHED_AT_KEY => scalar(container, "FinishedAt"),
        CONTAINER_TYPE_KEY => scalar(container, "Type"),
        LOG_DRIVER_KEY => scalar(container, "LogDriver"),
        SNAPSHOTTER_KEY => scalar(container, "Snapshotter"),
        RESTART_COUNT_KEY => scalar(container, "RestartCount"),
        _ => None,
    }
}

fn scalar<'a>(value: &'a JsonValue, key: &str) -> Option<&'a JsonValue> {
    value
        .get(key)
        .filter(|value| value.is_string() || value.is_number() || value.is_boolean())
}

fn json_string(value: &JsonValue, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToOwned::to_owned)
}

fn json_to_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::String(value) => Value::from(value.clone()),
        JsonValue::Number(value) => {
            if let Some(value) = value.as_i64() {
                Value::from(value)
            } else if let Some(value) = value.as_u64() {
                match i64::try_from(value) {
                    Ok(value) => Value::from(value),
                    Err(_) => Value::from(value.to_string()),
                }
            } else if let Some(value) = value.as_f64() {
                Value::from(value)
            } else {
                Value::from(value.to_string())
            }
        }
        JsonValue::Bool(value) => Value::from(*value),
        _ => Value::Null,
    }
}

fn value_to_metric_tag(value: &Value) -> String {
    // Metric tags are strings even when log enrichment preserves scalar types.
    match value {
        Value::Bytes(value) => String::from_utf8_lossy(value).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Boolean(value) => value.to_string(),
        Value::Null => "null".to_string(),
        value => value.to_string_lossy().into_owned(),
    }
}

// This creates a simplified string from the namespace. Since the namespace is technically
// a target path, it can contain syntax that is undesirable for a metric tag.
fn create_metric_namespace(namespace: &OwnedTargetPath) -> String {
    let mut output = String::new();
    for segment in &namespace.path.segments {
        if !output.is_empty() {
            output += ".";
        }
        match segment {
            OwnedSegment::Field(field) => {
                output += field;
            }
            OwnedSegment::Index(i) => {
                output += &i.to_string();
            }
        }
    }
    output
}

fn create_key(namespace: &Option<OwnedTargetPath>, key: &'static str) -> MetadataKey {
    if let Some(namespace) = namespace {
        MetadataKey {
            log_path: namespace.with_field_appended(key),
            metric_tag: format!("{}.{}", create_metric_namespace(namespace), key),
        }
    } else {
        MetadataKey {
            log_path: OwnedTargetPath::event(owned_value_path!(key)),
            metric_tag: key.to_owned(),
        }
    }
}

impl Keys {
    pub fn new(namespace: Option<OptionalTargetPath>, fields: &[String]) -> Self {
        let namespace = namespace.and_then(|namespace| namespace.path);

        Keys {
            keys: fields
                .iter()
                .filter_map(|field| {
                    FIELD_CATALOG
                        .get(field.as_str())
                        .map(|field| (*field, create_key(&namespace, field)))
                })
                .collect(),
        }
    }
}

#[derive(Debug)]
struct MissingEndpointError;

impl fmt::Display for MissingEndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ECS metadata endpoint was not configured and {METADATA_URI_V4_ENV} is not set"
        )
    }
}

impl error::Error for MissingEndpointError {}

#[derive(Debug)]
struct MissingCurrentContainerNameError;

impl fmt::Display for MissingCurrentContainerNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "current ECS container metadata response did not include Name"
        )
    }
}

impl error::Error for MissingCurrentContainerNameError {}

#[derive(Debug)]
struct MissingContainerError {
    name: String,
}

impl fmt::Display for MissingContainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ECS task metadata did not include container {:?}",
            self.name
        )
    }
}

impl error::Error for MissingContainerError {}

#[derive(Debug)]
struct UnknownFieldError {
    field: String,
}

impl fmt::Display for UnknownFieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown ECS metadata field {:?}", self.field)
    }
}

impl error::Error for UnknownFieldError {}

#[derive(Debug)]
struct UnexpectedHttpStatusError {
    status: http::StatusCode,
}

impl fmt::Display for UnexpectedHttpStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "got unexpected status code: {}", self.status)
    }
}

impl error::Error for UnexpectedHttpStatusError {}

#[derive(Debug, snafu::Snafu)]
enum EcsMetadataError {
    #[snafu(display("Unable to parse ECS metadata response: {}.", source))]
    ParseMetadata { source: serde_json::Error },
}

#[cfg(test)]
mod test {
    use std::{
        convert::Infallible,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use hyper::{
        Body, Response, Server, StatusCode,
        body::Bytes,
        service::{make_service_fn, service_fn},
    };
    use serial_test::serial;
    use tokio::{sync::mpsc, time::Duration};
    use tokio_stream::wrappers::ReceiverStream;
    use vector_lib::{assert_event_data_eq, lookup::event_path};
    use vrl::value::Value;

    use super::*;
    use crate::{
        config::{LogNamespace, OutputId, TransformConfig, schema::Definition},
        event::{LogEvent, Metric, metric},
        test_util::{addr::next_addr, components::assert_transform_compliance},
        transforms::test::create_topology,
    };

    const CURRENT_CONTAINER: &str = r#"{
        "DockerId": "task-id-1111111111",
        "Name": "vector",
        "DockerName": "vector"
    }"#;

    const TASK_METADATA_EC2: &str = r#"{
        "Cluster": "default",
        "TaskARN": "arn:aws:ecs:us-east-1:123456789012:task/default/ec2",
        "Family": "ec2-task",
        "Revision": "3",
        "ServiceName": "vector-service",
        "DesiredStatus": "RUNNING",
        "KnownStatus": "RUNNING",
        "PullStartedAt": "2026-06-05T01:00:00Z",
        "PullStoppedAt": "2026-06-05T01:00:01Z",
        "AvailabilityZone": "us-east-1a",
        "VPCID": "vpc-1234567890abcdef0",
        "LaunchType": "EC2",
        "Containers": [
            {
                "DockerId": "task-id-1111111111",
                "Name": "vector",
                "DockerName": "vector",
                "Image": "public.ecr.aws/vector/vector:latest",
                "ImageID": "sha256:vector-ec2",
                "DesiredStatus": "RUNNING",
                "KnownStatus": "RUNNING",
                "Type": "NORMAL",
                "ContainerARN": "arn:aws:ecs:us-east-1:123456789012:container/default/ec2/vector",
                "LogDriver": "awslogs",
                "RestartCount": 0
            },
            {
                "DockerId": "pause-container-id",
                "Name": "~internal~ecs~pause",
                "DockerName": "ecs-internal-pause",
                "Image": "amazon/amazon-ecs-pause:0.1.0",
                "ImageID": "sha256:pause",
                "Type": "CNI_PAUSE"
            }
        ]
    }"#;

    const TASK_METADATA_FARGATE: &str = r#"{
        "Cluster": "arn:aws:ecs:us-east-1:123456789012:cluster/example",
        "TaskARN": "arn:aws:ecs:us-east-1:123456789012:task/example/abc",
        "Family": "vector-task",
        "Revision": "7",
        "ServiceName": "vector-service",
        "DesiredStatus": "RUNNING",
        "KnownStatus": "RUNNING",
        "PullStartedAt": "2026-06-05T01:00:00Z",
        "PullStoppedAt": "2026-06-05T01:00:01Z",
        "AvailabilityZone": "us-east-1a",
        "LaunchType": "FARGATE",
        "Containers": [
            {
                "DockerId": "task-id-1111111111",
                "Name": "vector",
                "DockerName": "vector",
                "Image": "public.ecr.aws/vector/vector:latest",
                "ImageID": "sha256:vector",
                "DesiredStatus": "RUNNING",
                "KnownStatus": "RUNNING",
                "CreatedAt": "2026-06-05T01:00:02Z",
                "StartedAt": "2026-06-05T01:00:03Z",
                "Type": "NORMAL",
                "ContainerARN": "arn:aws:ecs:us-east-1:123456789012:container/example/abc/vector",
                "LogDriver": "awslogs",
                "Snapshotter": "overlayfs",
                "RestartCount": 2
            },
            {
                "DockerId": "task-id-2222222222",
                "Name": "app",
                "DockerName": "app",
                "Image": "public.ecr.aws/example/app:latest",
                "ImageID": "sha256:app",
                "ExitCode": 0,
                "Type": "NORMAL",
                "ContainerARN": "arn:aws:ecs:us-east-1:123456789012:container/example/abc/app",
                "Snapshotter": "soci"
            }
        ]
    }"#;

    const TASK_METADATA_MANAGED_INSTANCES: &str = r#"{
        "Cluster": "arn:aws:ecs:us-east-1:123456789012:cluster/managed",
        "TaskARN": "arn:aws:ecs:us-east-1:123456789012:task/managed/def",
        "Family": "managed-task",
        "Revision": "11",
        "ServiceName": "managed-service",
        "DesiredStatus": "RUNNING",
        "KnownStatus": "RUNNING",
        "Limits": { "CPU": 1, "Memory": 3072 },
        "PullStartedAt": "2026-06-05T01:00:00Z",
        "PullStoppedAt": "2026-06-05T01:00:01Z",
        "AvailabilityZone": "us-east-1b",
        "LaunchType": "MANAGED_INSTANCES",
        "Containers": [
            {
                "DockerId": "task-id-1111111111",
                "Name": "vector",
                "DockerName": "vector",
                "Image": "public.ecr.aws/vector/vector:latest",
                "ImageID": "sha256:vector-managed",
                "DesiredStatus": "RUNNING",
                "KnownStatus": "RUNNING",
                "Type": "NORMAL",
                "ContainerARN": "arn:aws:ecs:us-east-1:123456789012:container/managed/def/vector",
                "LogDriver": "awslogs",
                "Snapshotter": "overlayfs",
                "RestartCount": 1
            }
        ],
        "ClockDrift": {
            "ClockErrorBound": 0.14120749999999999,
            "ReferenceTimestamp": "2026-06-05T01:10:00Z",
            "ClockSynchronizationStatus": "SYNCHRONIZED"
        },
        "FaultInjectionEnabled": false
    }"#;

    const TASK_METADATA: &str = TASK_METADATA_FARGATE;

    async fn start_metadata_server() -> String {
        start_metadata_server_with_failures(0).await
    }

    async fn start_metadata_server_with_failures(failures: usize) -> String {
        let (_guard, addr) = next_addr();
        let remaining_failures = Arc::new(AtomicUsize::new(failures));

        let make_svc = make_service_fn(move |_| {
            let remaining_failures = Arc::clone(&remaining_failures);
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let remaining_failures = Arc::clone(&remaining_failures);
                    async move {
                        if remaining_failures.load(Ordering::SeqCst) > 0 {
                            remaining_failures.fetch_sub(1, Ordering::SeqCst);
                            return Ok::<_, Infallible>(
                                Response::builder()
                                    .status(StatusCode::SERVICE_UNAVAILABLE)
                                    .body(Body::from("not ready"))
                                    .unwrap(),
                            );
                        }

                        let body = match req.uri().path() {
                            "/" => CURRENT_CONTAINER,
                            "/task" => TASK_METADATA,
                            _ => "",
                        };

                        let status = if body.is_empty() {
                            StatusCode::NOT_FOUND
                        } else {
                            StatusCode::OK
                        };

                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(status)
                                .body(Body::from(body))
                                .unwrap(),
                        )
                    }
                }))
            }
        });

        tokio::spawn(Server::bind(&addr).serve(make_svc));
        format!("http://{addr}")
    }

    async fn start_stalled_body_server() -> String {
        let (_guard, addr) = next_addr();

        let make_svc = make_service_fn(move |_| async move {
            Ok::<_, Infallible>(service_fn(move |_| async move {
                let (mut sender, body) = Body::channel();
                tokio::spawn(async move {
                    sender.send_data(Bytes::from_static(b"{")).await.unwrap();
                    std::future::pending::<()>().await;
                });

                Ok::<_, Infallible>(
                    Response::builder()
                        .status(StatusCode::OK)
                        .body(body)
                        .unwrap(),
                )
            }))
        });

        tokio::spawn(Server::bind(&addr).serve(make_svc));
        format!("http://{addr}")
    }

    async fn start_task_only_metadata_server() -> String {
        let (_guard, addr) = next_addr();

        let make_svc = make_service_fn(move |_| async move {
            Ok::<_, Infallible>(service_fn(move |req| async move {
                match req.uri().path() {
                    "/task" => Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::OK)
                            .body(Body::from(TASK_METADATA))
                            .unwrap(),
                    ),
                    _ => Ok::<_, Infallible>(
                        Response::builder()
                            .status(StatusCode::SERVICE_UNAVAILABLE)
                            .body(Body::from("current container unavailable"))
                            .unwrap(),
                    ),
                }
            }))
        });

        tokio::spawn(Server::bind(&addr).serve(make_svc));
        format!("http://{addr}")
    }

    fn make_metric() -> Metric {
        Metric::new(
            "event",
            metric::MetricKind::Incremental,
            metric::MetricValue::Counter { value: 1.0 },
        )
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<EcsMetadata>();
    }

    #[tokio::test]
    async fn schema_def_with_string_input() {
        let transform_config = EcsMetadata::default();

        let input_definition =
            Definition::new(Kind::bytes(), Kind::any_object(), [LogNamespace::Vector]);

        let mut outputs = transform_config.outputs(
            &Default::default(),
            &[(OutputId::dummy(), input_definition)],
        );
        assert_eq!(outputs.len(), 1);
        let output = outputs.pop().unwrap();
        let actual_schema_def = output.schema_definitions(true)[&OutputId::dummy()].clone();
        assert!(actual_schema_def.event_kind().is_object());
    }

    #[tokio::test]
    async fn enrich_log_with_default_fields() {
        assert_transform_compliance(async {
            let endpoint = start_metadata_server().await;
            let transform_config = EcsMetadata {
                endpoint: Some(endpoint),
                ..Default::default()
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut log = LogEvent::default();
            log.insert(event_path!("aws", "ecs", "task-arn"), "existing");

            let mut expected_log = log.clone();
            expected_log.insert(
                event_path!("aws", "ecs", "cluster"),
                "arn:aws:ecs:us-east-1:123456789012:cluster/example",
            );
            expected_log.insert(
                event_path!("aws", "ecs", "task-arn"),
                "arn:aws:ecs:us-east-1:123456789012:task/example/abc",
            );
            expected_log.insert(event_path!("aws", "ecs", "family"), "vector-task");
            expected_log.insert(event_path!("aws", "ecs", "revision"), "7");
            expected_log.insert(event_path!("aws", "ecs", "service-name"), "vector-service");
            expected_log.insert(event_path!("aws", "ecs", "launch-type"), "FARGATE");
            expected_log.insert(event_path!("aws", "ecs", "availability-zone"), "us-east-1a");
            expected_log.insert(event_path!("aws", "ecs", "container-name"), "vector");
            expected_log.insert(
                event_path!("aws", "ecs", "container-id"),
                "task-id-1111111111",
            );
            expected_log.insert(
                event_path!("aws", "ecs", "container-arn"),
                "arn:aws:ecs:us-east-1:123456789012:container/example/abc/vector",
            );
            expected_log.insert(
                event_path!("aws", "ecs", "image"),
                "public.ecr.aws/vector/vector:latest",
            );
            expected_log.insert(event_path!("aws", "ecs", "image-id"), "sha256:vector");

            tx.send(log.into()).await.unwrap();

            let event = out.recv().await.unwrap();
            assert_event_data_eq!(event.into_log(), expected_log);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn enrich_metric_with_default_fields() {
        assert_transform_compliance(async {
            let endpoint = start_metadata_server().await;
            let transform_config = EcsMetadata {
                endpoint: Some(endpoint),
                ..Default::default()
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let mut metric = make_metric();
            metric.replace_tag("aws.ecs.task-arn".to_string(), "existing".to_string());

            let mut expected_metric = metric.clone();
            expected_metric.replace_tag(
                "aws.ecs.cluster".to_string(),
                "arn:aws:ecs:us-east-1:123456789012:cluster/example".to_string(),
            );
            expected_metric.replace_tag(
                "aws.ecs.task-arn".to_string(),
                "arn:aws:ecs:us-east-1:123456789012:task/example/abc".to_string(),
            );
            expected_metric.replace_tag("aws.ecs.family".to_string(), "vector-task".to_string());
            expected_metric.replace_tag("aws.ecs.revision".to_string(), "7".to_string());
            expected_metric.replace_tag(
                "aws.ecs.service-name".to_string(),
                "vector-service".to_string(),
            );
            expected_metric.replace_tag("aws.ecs.launch-type".to_string(), "FARGATE".to_string());
            expected_metric.replace_tag(
                "aws.ecs.availability-zone".to_string(),
                "us-east-1a".to_string(),
            );
            expected_metric.replace_tag("aws.ecs.container-name".to_string(), "vector".to_string());
            expected_metric.replace_tag(
                "aws.ecs.container-id".to_string(),
                "task-id-1111111111".to_string(),
            );
            expected_metric.replace_tag(
                "aws.ecs.container-arn".to_string(),
                "arn:aws:ecs:us-east-1:123456789012:container/example/abc/vector".to_string(),
            );
            expected_metric.replace_tag(
                "aws.ecs.image".to_string(),
                "public.ecr.aws/vector/vector:latest".to_string(),
            );
            expected_metric
                .replace_tag("aws.ecs.image-id".to_string(), "sha256:vector".to_string());

            tx.send(metric.into()).await.unwrap();

            let event = out.recv().await.unwrap();
            assert_event_data_eq!(event.into_metric(), expected_metric);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn container_name_override_and_scalar_types() {
        assert_transform_compliance(async {
            let endpoint = start_metadata_server().await;
            let transform_config = EcsMetadata {
                endpoint: Some(endpoint),
                container_name: Some("app".into()),
                namespace: Some(OptionalTargetPath::none()),
                fields: vec![
                    CONTAINER_NAME_KEY.into(),
                    CONTAINER_ID_KEY.into(),
                    CONTAINER_EXIT_CODE_KEY.into(),
                    SNAPSHOTTER_KEY.into(),
                ],
                ..Default::default()
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let log = LogEvent::default();
            tx.send(log.into()).await.unwrap();

            let event = out.recv().await.unwrap();
            let log = event.into_log();
            assert_eq!(
                log.get(event_path!(CONTAINER_NAME_KEY)),
                Some(&Value::from("app"))
            );
            assert_eq!(
                log.get(event_path!(CONTAINER_ID_KEY)),
                Some(&Value::from("task-id-2222222222"))
            );
            assert_eq!(
                log.get(event_path!(CONTAINER_EXIT_CODE_KEY)),
                Some(&Value::from(0))
            );
            assert_eq!(
                log.get(event_path!(SNAPSHOTTER_KEY)),
                Some(&Value::from("soci"))
            );

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn container_name_override_skips_current_container_request() {
        let endpoint = start_task_only_metadata_server().await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            container_name: Some("app".into()),
            initial_retry_attempts: 1,
            ..Default::default()
        };

        assert!(
            transform_config
                .build(&TransformContext::default())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn initial_retry_succeeds_after_transient_failures() {
        let endpoint = start_metadata_server_with_failures(2).await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            initial_retry_attempts: 3,
            initial_retry_backoff_secs: Duration::from_millis(1),
            ..Default::default()
        };

        assert!(
            transform_config
                .build(&TransformContext::default())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn initial_retry_fails_when_required() {
        let endpoint = start_metadata_server_with_failures(3).await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            initial_retry_attempts: 3,
            initial_retry_backoff_secs: Duration::from_millis(1),
            ..Default::default()
        };

        let error = match transform_config.build(&TransformContext::default()).await {
            Ok(_) => panic!("expected initial refresh to fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("got unexpected status code"));
    }

    #[tokio::test]
    async fn refresh_timeout_covers_stalled_body() {
        let endpoint = start_stalled_body_server().await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            initial_retry_attempts: 1,
            refresh_timeout_secs: Duration::from_millis(10),
            ..Default::default()
        };

        let result = tokio::time::timeout(
            Duration::from_millis(100),
            transform_config.build(&TransformContext::default()),
        )
        .await
        .expect("refresh timeout should cover stalled response body");

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn initial_retry_failure_can_be_optional() {
        let endpoint = start_metadata_server_with_failures(3).await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            initial_retry_attempts: 3,
            initial_retry_backoff_secs: Duration::from_millis(1),
            required: false,
            ..Default::default()
        };

        assert!(
            transform_config
                .build(&TransformContext::default())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn unknown_field_is_error() {
        let transform_config = EcsMetadata {
            fields: vec!["not-a-field".into()],
            ..Default::default()
        };

        let error = match transform_config.build(&TransformContext::default()).await {
            Ok(_) => panic!("expected unknown field to fail"),
            Err(error) => error,
        };

        assert_eq!(
            error.to_string(),
            "unknown ECS metadata field \"not-a-field\""
        );
    }

    #[tokio::test]
    async fn missing_container_is_error() {
        let endpoint = start_metadata_server().await;
        let transform_config = EcsMetadata {
            endpoint: Some(endpoint),
            container_name: Some("missing".into()),
            ..Default::default()
        };

        let error = match transform_config.build(&TransformContext::default()).await {
            Ok(_) => panic!("expected missing container to fail"),
            Err(error) => error,
        };

        assert_eq!(
            error.to_string(),
            "ECS task metadata did not include container \"missing\""
        );
    }

    #[tokio::test]
    async fn absent_supported_field_is_omitted() {
        assert_transform_compliance(async {
            let endpoint = start_metadata_server().await;
            let transform_config = EcsMetadata {
                endpoint: Some(endpoint),
                fields: vec![VPC_ID_KEY.into(), CLUSTER_KEY.into()],
                ..Default::default()
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            tx.send(LogEvent::default().into()).await.unwrap();

            let event = out.recv().await.unwrap();
            let log = event.into_log();
            assert_eq!(
                log.get(event_path!("aws", "ecs", "cluster")),
                Some(&Value::from(
                    "arn:aws:ecs:us-east-1:123456789012:cluster/example"
                ))
            );
            assert_eq!(log.get(event_path!("aws", "ecs", "vpc-id")), None);

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[test]
    fn extracts_fields_from_all_documented_launch_type_examples() {
        for (task_metadata, launch_type, snapshotter, fault_injection_enabled) in [
            (TASK_METADATA_EC2, "EC2", None, None),
            (TASK_METADATA_FARGATE, "FARGATE", Some("overlayfs"), None),
            (
                TASK_METADATA_MANAGED_INSTANCES,
                "MANAGED_INSTANCES",
                Some("overlayfs"),
                Some(false),
            ),
        ] {
            let task: JsonValue = serde_json::from_str(task_metadata).unwrap();
            let container = find_container(&task, "vector").unwrap();

            assert_eq!(
                extract_field(LAUNCH_TYPE_KEY, &task, container).map(json_to_value),
                Some(Value::from(launch_type))
            );
            assert_eq!(
                extract_field(CONTAINER_NAME_KEY, &task, container).map(json_to_value),
                Some(Value::from("vector"))
            );
            assert!(
                extract_field(TASK_ARN_KEY, &task, container).is_some(),
                "{launch_type} example should include a task ARN"
            );
            assert_eq!(
                extract_field(SNAPSHOTTER_KEY, &task, container).map(json_to_value),
                snapshotter.map(Value::from)
            );
            assert_eq!(
                extract_field(FAULT_INJECTION_ENABLED_KEY, &task, container).map(json_to_value),
                fault_injection_enabled.map(Value::from)
            );
        }
    }

    #[tokio::test]
    #[serial]
    async fn default_endpoint_uses_environment() {
        let endpoint = start_metadata_server().await;
        unsafe {
            env::set_var(METADATA_URI_V4_ENV, &endpoint);
        }

        let transform_config = EcsMetadata {
            endpoint: None,
            initial_retry_backoff_secs: Duration::from_millis(1),
            ..Default::default()
        };

        assert!(
            transform_config
                .build(&TransformContext::default())
                .await
                .is_ok()
        );

        unsafe {
            env::remove_var(METADATA_URI_V4_ENV);
        }
    }

    #[tokio::test]
    #[serial]
    async fn missing_endpoint_honors_required() {
        unsafe {
            env::remove_var(METADATA_URI_V4_ENV);
        }

        let required_config = EcsMetadata {
            endpoint: None,
            initial_retry_attempts: 1,
            ..Default::default()
        };
        let error = match required_config.build(&TransformContext::default()).await {
            Ok(_) => panic!("expected missing endpoint to fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(METADATA_URI_V4_ENV));

        let optional_config = EcsMetadata {
            endpoint: None,
            initial_retry_attempts: 1,
            required: false,
            ..Default::default()
        };
        assert!(
            optional_config
                .build(&TransformContext::default())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn metric_values_are_stringified() {
        assert_transform_compliance(async {
            let endpoint = start_metadata_server().await;
            let transform_config = EcsMetadata {
                endpoint: Some(endpoint),
                fields: vec![RESTART_COUNT_KEY.into()],
                ..Default::default()
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            tx.send(make_metric().into()).await.unwrap();

            let event = out.recv().await.unwrap();
            let metric = event.into_metric();
            assert_eq!(
                metric.tag_value("aws.ecs.restart-count"),
                Some("2".to_string())
            );
            assert_eq!(value_to_metric_tag(&Value::from(false)), "false");

            drop(tx);
            topology.stop().await;
            assert_eq!(out.recv().await, None);
        })
        .await;
    }
}
