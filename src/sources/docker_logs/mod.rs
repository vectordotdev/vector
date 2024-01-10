use std::{
    collections::HashMap, convert::TryFrom, future::ready, pin::Pin, sync::Arc, time::Duration,
};

use bollard::{
    container::{InspectContainerOptions, ListContainersOptions, LogOutput, LogsOptions},
    errors::Error as DockerError,
    service::{ContainerInspectResponse, EventMessage},
    system::EventsOptions,
    Docker,
};
use bytes::{Buf, Bytes};
use chrono::{DateTime, FixedOffset, Local, ParseError, Utc};
use futures::{Stream, StreamExt};
use once_cell::sync::Lazy;
use serde_with::serde_as;
use tokio::sync::mpsc;
use tracing_futures::Instrument;
use vector_lib::codecs::{BytesDeserializer, BytesDeserializerConfig};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::internal_event::{
    ByteSize, BytesReceived, InternalEventHandle as _, Protocol, Registered,
};
use vector_lib::lookup::{
    lookup_v2::OptionalValuePath, metadata_path, owned_value_path, path, OwnedValuePath, PathPrefix,
};
use vrl::event_path;
use vrl::value::{kind::Collection, Kind};

use super::util::MultilineConfig;
use crate::{
    config::{log_schema, DataType, SourceConfig, SourceContext, SourceOutput},
    docker::{docker, DockerTlsConfig},
    event::{self, merge_state::LogEventMergeState, EstimatedJsonEncodedSizeOf, LogEvent, Value},
    internal_events::{
        DockerLogsCommunicationError, DockerLogsContainerEventReceived,
        DockerLogsContainerMetadataFetchError, DockerLogsContainerUnwatch,
        DockerLogsContainerWatch, DockerLogsEventsReceived,
        DockerLogsLoggingDriverUnsupportedError, DockerLogsTimestampParseError, StreamClosedError,
    },
    line_agg::{self, LineAgg},
    shutdown::ShutdownSignal,
    SourceSender,
};

#[cfg(test)]
mod tests;

const IMAGE: &str = "image";
const CREATED_AT: &str = "container_created_at";
const NAME: &str = "container_name";
const STREAM: &str = "stream";
const CONTAINER: &str = "container_id";
// Prevent short hostname from being wrongly recognized as a container's short ID.
const MIN_HOSTNAME_LENGTH: usize = 6;

static STDERR: Lazy<Bytes> = Lazy::new(|| "stderr".into());
static STDOUT: Lazy<Bytes> = Lazy::new(|| "stdout".into());
static CONSOLE: Lazy<Bytes> = Lazy::new(|| "console".into());

/// Configuration for the `docker_logs` source.
#[serde_as]
#[configurable_component(source("docker_logs", "Collect container logs from a Docker Daemon."))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct DockerLogsConfig {
    /// Overrides the name of the log field used to add the current hostname to each event.
    ///
    /// By default, the [global `log_schema.host_key` option][global_host_key] is used.
    ///
    /// [global_host_key]: https://vector.dev/docs/reference/configuration/global-options/#log_schema.host_key
    #[serde(default = "default_host_key")]
    host_key: OptionalValuePath,

    /// Docker host to connect to.
    ///
    /// Use an HTTPS URL to enable TLS encryption.
    ///
    /// If absent, the `DOCKER_HOST` environment variable is used. If `DOCKER_HOST` is also absent,
    /// the default Docker local socket (`/var/run/docker.sock` on Unix platforms,
    /// `//./pipe/docker_engine` on Windows) is used.
    #[configurable(metadata(docs::examples = "http://localhost:2375"))]
    #[configurable(metadata(docs::examples = "https://localhost:2376"))]
    #[configurable(metadata(docs::examples = "unix:///var/run/docker.sock"))]
    #[configurable(metadata(docs::examples = "npipe:////./pipe/docker_engine"))]
    #[configurable(metadata(docs::examples = "/var/run/docker.sock"))]
    #[configurable(metadata(docs::examples = "//./pipe/docker_engine"))]
    docker_host: Option<String>,

    /// A list of container IDs or names of containers to exclude from log collection.
    ///
    /// Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
    /// container whose name started with `foo`. This applies equally whether matching container IDs or names.
    ///
    /// By default, the source collects logs for all containers. If `exclude_containers` is configured, any
    /// container that matches a configured exclusion is excluded even if it is also included with
    /// `include_containers`, so care should be taken when using prefix matches as they cannot be overridden by a
    /// corresponding entry in `include_containers`, for example, excluding `foo` by attempting to include `foo-specific-id`.
    ///
    /// This can be used in conjunction with `include_containers`.
    #[configurable(metadata(
        docs::examples = "exclude_",
        docs::examples = "exclude_me_0",
        docs::examples = "ad08cc418cf9"
    ))]
    exclude_containers: Option<Vec<String>>, // Starts with actually, not exclude

    /// A list of container IDs or names of containers to include in log collection.
    ///
    /// Matching is prefix first, so specifying a value of `foo` would match any container named `foo` as well as any
    /// container whose name started with `foo`. This applies equally whether matching container IDs or names.
    ///
    /// By default, the source collects logs for all containers. If `include_containers` is configured, only
    /// containers that match a configured inclusion and are also not excluded get matched.
    ///
    /// This can be used in conjunction with `exclude_containers`.
    #[configurable(metadata(
        docs::examples = "include_",
        docs::examples = "include_me_0",
        docs::examples = "ad08cc418cf9"
    ))]
    include_containers: Option<Vec<String>>, // Starts with actually, not include

    /// A list of container object labels to match against when filtering running containers.
    ///
    /// Labels should follow the syntax described in the [Docker object labels](https://docs.docker.com/config/labels-custom-metadata/) documentation.
    #[configurable(metadata(
        docs::examples = "org.opencontainers.image.vendor=Vector",
        docs::examples = "com.mycorp.internal.animal=fish",
    ))]
    include_labels: Option<Vec<String>>,

    /// A list of image names to match against.
    ///
    /// If not provided, all images are included.
    #[configurable(metadata(docs::examples = "httpd", docs::examples = "redis",))]
    include_images: Option<Vec<String>>,

    /// Overrides the name of the log field used to mark an event as partial.
    ///
    /// If `auto_partial_merge` is disabled, partial events are emitted with a log field, set by this
    /// configuration value, indicating that the event is not complete.
    #[serde(default = "default_partial_event_marker_field")]
    partial_event_marker_field: Option<String>,

    /// Enables automatic merging of partial events.
    auto_partial_merge: bool,

    /// The amount of time to wait before retrying after an error.
    #[serde_as(as = "serde_with::DurationSeconds<u64>")]
    #[serde(default = "default_retry_backoff_secs")]
    #[configurable(metadata(docs::human_name = "Retry Backoff"))]
    retry_backoff_secs: Duration,

    /// Multiline aggregation configuration.
    ///
    /// If not specified, multiline aggregation is disabled.
    #[configurable(derived)]
    multiline: Option<MultilineConfig>,

    #[configurable(derived)]
    tls: Option<DockerTlsConfig>,

    /// The namespace to use for logs. This overrides the global setting.
    #[serde(default)]
    #[configurable(metadata(docs::hidden))]
    pub log_namespace: Option<bool>,
}

impl Default for DockerLogsConfig {
    fn default() -> Self {
        Self {
            host_key: default_host_key(),
            docker_host: None,
            tls: None,
            exclude_containers: None,
            include_containers: None,
            include_labels: None,
            include_images: None,
            partial_event_marker_field: default_partial_event_marker_field(),
            auto_partial_merge: true,
            multiline: None,
            retry_backoff_secs: default_retry_backoff_secs(),
            log_namespace: None,
        }
    }
}

fn default_host_key() -> OptionalValuePath {
    log_schema().host_key().cloned().into()
}

fn default_partial_event_marker_field() -> Option<String> {
    Some(event::PARTIAL.to_string())
}

const fn default_retry_backoff_secs() -> Duration {
    Duration::from_secs(2)
}

impl DockerLogsConfig {
    fn container_name_or_id_included<'a>(
        &self,
        id: &str,
        names: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        let containers: Vec<String> = names.into_iter().map(Into::into).collect();

        self.include_containers
            .as_ref()
            .map(|include_list| Self::name_or_id_matches(id, &containers, include_list))
            .unwrap_or(true)
            && !(self
                .exclude_containers
                .as_ref()
                .map(|exclude_list| Self::name_or_id_matches(id, &containers, exclude_list))
                .unwrap_or(false))
    }

    fn name_or_id_matches(id: &str, names: &[String], items: &[String]) -> bool {
        items.iter().any(|flag| id.starts_with(flag))
            || names
                .iter()
                .any(|name| items.iter().any(|item| name.starts_with(item)))
    }

    fn with_empty_partial_event_marker_field_as_none(mut self) -> Self {
        if let Some(val) = &self.partial_event_marker_field {
            if val.is_empty() {
                self.partial_event_marker_field = None;
            }
        }
        self
    }
}

impl_generate_config_from_default!(DockerLogsConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "docker_logs")]
impl SourceConfig for DockerLogsConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let log_namespace = cx.log_namespace(self.log_namespace);
        let source = DockerLogsSource::new(
            self.clone().with_empty_partial_event_marker_field_as_none(),
            cx.out,
            cx.shutdown.clone(),
            log_namespace,
        )?;

        // Capture currently running containers, and do main future(run)
        let fut = async move {
            match source.handle_running_containers().await {
                Ok(source) => source.run().await,
                Err(error) => {
                    error!(
                        message = "Listing currently running containers failed.",
                        %error
                    );
                }
            }
        };

        let shutdown = cx.shutdown;
        // Once this ShutdownSignal resolves it will drop DockerLogsSource and by extension it's ShutdownSignal.
        Ok(Box::pin(async move {
            Ok(tokio::select! {
                _ = fut => {}
                _ = shutdown => {}
            })
        }))
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<SourceOutput> {
        let host_key = self.host_key.clone().path.map(LegacyKey::Overwrite);

        let schema_definition = BytesDeserializerConfig
            .schema_definition(global_log_namespace.merge(self.log_namespace))
            .with_source_metadata(
                Self::NAME,
                host_key,
                &owned_value_path!("host"),
                Kind::bytes().or_undefined(),
                Some("host"),
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(CONTAINER))),
                &owned_value_path!(CONTAINER),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(IMAGE))),
                &owned_value_path!(IMAGE),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(NAME))),
                &owned_value_path!(NAME),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(CREATED_AT))),
                &owned_value_path!(CREATED_AT),
                Kind::timestamp(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!("label"))),
                &owned_value_path!("labels"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                Some(LegacyKey::Overwrite(owned_value_path!(STREAM))),
                &owned_value_path!(STREAM),
                Kind::bytes(),
                None,
            )
            .with_source_metadata(
                Self::NAME,
                log_schema()
                    .timestamp_key()
                    .cloned()
                    .map(LegacyKey::Overwrite),
                &owned_value_path!("timestamp"),
                Kind::timestamp(),
                Some("timestamp"),
            )
            .with_vector_metadata(
                log_schema().source_type_key(),
                &owned_value_path!("source_type"),
                Kind::bytes(),
                None,
            )
            .with_vector_metadata(
                None,
                &owned_value_path!("ingest_timestamp"),
                Kind::timestamp(),
                None,
            );

        vec![SourceOutput::new_logs(DataType::Log, schema_definition)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

struct DockerLogsSourceCore {
    config: DockerLogsConfig,
    line_agg_config: Option<line_agg::Config>,
    docker: Docker,
    /// Only logs created at, or after this moment are logged.
    now_timestamp: DateTime<Utc>,
}

impl DockerLogsSourceCore {
    fn new(config: DockerLogsConfig) -> crate::Result<Self> {
        // ?NOTE: Constructs a new Docker instance for a docker host listening at url specified by an env var DOCKER_HOST.
        // ?      Otherwise connects to unix socket which requires sudo privileges, or docker group membership.
        let docker = docker(config.docker_host.clone(), config.tls.clone())?;

        // Only log events created at-or-after this moment are logged.
        let now = Local::now();
        info!(
            message = "Capturing logs from now on.",
            now = %now.to_rfc3339()
        );

        let line_agg_config = if let Some(ref multiline_config) = config.multiline {
            Some(line_agg::Config::try_from(multiline_config)?)
        } else {
            None
        };

        Ok(DockerLogsSourceCore {
            config,
            line_agg_config,
            docker,
            now_timestamp: now.into(),
        })
    }

    /// Returns event stream coming from docker.
    fn docker_logs_event_stream(
        &self,
    ) -> impl Stream<Item = Result<EventMessage, DockerError>> + Send {
        let mut filters = HashMap::new();

        // event  | emitted on commands
        // -------+-------------------
        // start  | docker start, docker run, restart policy, docker restart
        // unpause | docker unpause
        // die    | docker restart, docker stop, docker kill, process exited, oom
        // pause  | docker pause
        filters.insert(
            "event".to_owned(),
            vec![
                "start".to_owned(),
                "unpause".to_owned(),
                "die".to_owned(),
                "pause".to_owned(),
            ],
        );
        filters.insert("type".to_owned(), vec!["container".to_owned()]);

        // Apply include filters.
        if let Some(include_labels) = &self.config.include_labels {
            filters.insert("label".to_owned(), include_labels.clone());
        }

        if let Some(include_images) = &self.config.include_images {
            filters.insert("image".to_owned(), include_images.clone());
        }

        self.docker.events(Some(EventsOptions {
            since: Some(self.now_timestamp),
            until: None,
            filters,
        }))
    }
}

/// Main future which listens for events coming from docker, and maintains
/// a fan of event_stream futures.
/// Where each event_stream corresponds to a running container marked with ContainerLogInfo.
/// While running, event_stream streams Events to out channel.
/// Once a log stream has ended, it sends ContainerLogInfo back to main.
///
/// Future  channel     Future      channel
///           |<---- event_stream ---->out
/// main <----|<---- event_stream ---->out
///           | ...                 ...out
///
struct DockerLogsSource {
    esb: EventStreamBuilder,
    /// event stream from docker
    events: Pin<Box<dyn Stream<Item = Result<EventMessage, DockerError>> + Send>>,
    ///  mappings of seen container_id to their data
    containers: HashMap<ContainerId, ContainerState>,
    ///receives ContainerLogInfo coming from event stream futures
    main_recv: mpsc::UnboundedReceiver<Result<ContainerLogInfo, (ContainerId, ErrorPersistence)>>,
    /// It may contain shortened container id.
    hostname: Option<String>,
    backoff_duration: Duration,
}

impl DockerLogsSource {
    fn new(
        config: DockerLogsConfig,
        out: SourceSender,
        shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
    ) -> crate::Result<DockerLogsSource> {
        let backoff_secs = config.retry_backoff_secs;

        let host_key = config.host_key.clone();
        let hostname = crate::get_hostname().ok();

        // Only logs created at, or after this moment are logged.
        let core = DockerLogsSourceCore::new(config)?;

        // main event stream, with whom only newly started/restarted containers will be logged.
        let events = core.docker_logs_event_stream();
        info!(message = "Listening to docker log events.");

        // Channel of communication between main future and event_stream futures
        let (main_send, main_recv) =
            mpsc::unbounded_channel::<Result<ContainerLogInfo, (ContainerId, ErrorPersistence)>>();

        // Starting with logs from now.
        // TODO: Is this exception acceptable?
        // Only somewhat exception to this is case where:
        // t0 -- outside: container running
        // t1 -- now_timestamp
        // t2 -- outside: container stopped
        // t3 -- list_containers
        // In that case, logs between [t1,t2] will be pulled to vector only on next start/unpause of that container.
        let esb = EventStreamBuilder {
            host_key,
            hostname: hostname.clone(),
            core: Arc::new(core),
            out,
            main_send,
            shutdown,
            log_namespace,
        };

        Ok(DockerLogsSource {
            esb,
            events: Box::pin(events),
            containers: HashMap::new(),
            main_recv,
            hostname,
            backoff_duration: backoff_secs,
        })
    }

    /// Future that captures currently running containers, and starts event streams for them.
    async fn handle_running_containers(mut self) -> crate::Result<Self> {
        let mut filters = HashMap::new();

        // Apply include filters
        if let Some(include_labels) = &self.esb.core.config.include_labels {
            filters.insert("label".to_owned(), include_labels.clone());
        }

        if let Some(include_images) = &self.esb.core.config.include_images {
            filters.insert("ancestor".to_owned(), include_images.clone());
        }

        self.esb
            .core
            .docker
            .list_containers(Some(ListContainersOptions {
                all: false, // only running containers
                filters,
                ..Default::default()
            }))
            .await?
            .into_iter()
            .for_each(|container| {
                let id = container.id.unwrap();
                let names = container.names.unwrap();

                trace!(message = "Found already running container.", id = %id, names = ?names);

                if self.exclude_self(id.as_str()) {
                    info!(message = "Excluded self container.", id = %id);
                    return;
                }

                if !self.esb.core.config.container_name_or_id_included(
                    id.as_str(),
                    names.iter().map(|s| {
                        // In this case bollard / shiplift gives names with starting '/' so it needs to be removed.
                        let s = s.as_str();
                        if s.starts_with('/') {
                            s.split_at('/'.len_utf8()).1
                        } else {
                            s
                        }
                    }),
                ) {
                    info!(message = "Excluded container.", id = %id);
                    return;
                }

                let id = ContainerId::new(id);
                self.containers.insert(id.clone(), self.esb.start(id, None));
            });

        Ok(self)
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                value = self.main_recv.recv() => {
                    match value {
                        Some(Ok(info)) => {
                            let state = self
                                .containers
                                .get_mut(&info.id)
                                .expect("Every ContainerLogInfo has it's ContainerState");
                            if state.return_info(info) {
                                self.esb.restart(state);
                            }
                        },
                        Some(Err((id,persistence))) => {
                            let state = self
                                .containers
                                .remove(&id)
                                .expect("Every started ContainerId has it's ContainerState");
                            match persistence{
                                ErrorPersistence::Transient => if state.is_running() {
                                    let backoff= Some(self.backoff_duration);
                                    self.containers.insert(id.clone(), self.esb.start(id, backoff));
                                }
                                // Forget the container since the error is permanent.
                                ErrorPersistence::Permanent => (),
                            }
                        }
                        None => {
                            error!(message = "The docker_logs source main stream has ended unexpectedly.");
                            info!(message = "Shutting down docker_logs source.");
                            return;
                        }
                    };
                }
                value = self.events.next() => {
                    match value {
                        Some(Ok(mut event)) => {
                            let action = event.action.unwrap();
                            let actor = event.actor.take().unwrap();
                            let id = actor.id.unwrap();
                            let attributes = actor.attributes.unwrap();

                            emit!(DockerLogsContainerEventReceived { container_id: &id, action: &action });

                            let id = ContainerId::new(id.to_owned());

                            // Update container status
                            match action.as_str() {
                                "die" | "pause" => {
                                    if let Some(state) = self.containers.get_mut(&id) {
                                        state.stopped();
                                    }
                                }
                                "start" | "unpause" => {
                                    if let Some(state) = self.containers.get_mut(&id) {
                                        state.running();
                                        self.esb.restart(state);
                                    } else {
                                        let include_name =
                                            self.esb.core.config.container_name_or_id_included(
                                                id.as_str(),
                                                attributes.get("name").map(|s| s.as_str()),
                                            );

                                        let exclude_self = self.exclude_self(id.as_str());

                                        if include_name && !exclude_self {
                                            self.containers.insert(id.clone(), self.esb.start(id, None));
                                        }
                                    }
                                }
                                _ => {},
                            };
                        }
                        Some(Err(error)) => {
                            emit!(DockerLogsCommunicationError {
                                error,
                                container_id: None,
                            });
                            return;
                        },
                        None => {
                            // TODO: this could be fixed, but should be tried with some timeoff and exponential backoff
                            error!(message = "Docker log event stream has ended unexpectedly.");
                            info!(message = "Shutting down docker_logs source.");
                            return;
                        }
                    };
                }
            };
        }
    }

    fn exclude_self(&self, id: &str) -> bool {
        self.hostname
            .as_ref()
            .map(|hostname| id.starts_with(hostname) && hostname.len() >= MIN_HOSTNAME_LENGTH)
            .unwrap_or(false)
    }
}

/// Used to construct and start event stream futures
#[derive(Clone)]
struct EventStreamBuilder {
    host_key: OptionalValuePath,
    hostname: Option<String>,
    core: Arc<DockerLogsSourceCore>,
    /// Event stream futures send events through this
    out: SourceSender,
    /// End through which event stream futures send ContainerLogInfo to main future
    main_send: mpsc::UnboundedSender<Result<ContainerLogInfo, (ContainerId, ErrorPersistence)>>,
    /// Self and event streams will end on this.
    shutdown: ShutdownSignal,
    log_namespace: LogNamespace,
}

impl EventStreamBuilder {
    /// Spawn a task to runs event stream until shutdown.
    fn start(&self, id: ContainerId, backoff: Option<Duration>) -> ContainerState {
        let this = self.clone();
        tokio::spawn(
            async move {
                if let Some(duration) = backoff {
                    tokio::time::sleep(duration).await;
                }

                match this
                    .core
                    .docker
                    .inspect_container(id.as_str(), None::<InspectContainerOptions>)
                    .await
                {
                    Ok(details) => match ContainerMetadata::from_details(details) {
                        Ok(metadata) => {
                            let info = ContainerLogInfo::new(id, metadata, this.core.now_timestamp);
                            this.run_event_stream(info).await;
                            return;
                        }
                        Err(error) => emit!(DockerLogsTimestampParseError {
                            error,
                            container_id: id.as_str()
                        }),
                    },
                    Err(error) => emit!(DockerLogsContainerMetadataFetchError {
                        error,
                        container_id: id.as_str()
                    }),
                }

                this.finish(Err((id, ErrorPersistence::Transient)));
            }
            .in_current_span(),
        );

        ContainerState::new_running()
    }

    /// If info is present, restarts event stream which will run until shutdown.
    fn restart(&self, container: &mut ContainerState) {
        if let Some(info) = container.take_info() {
            let this = self.clone();
            tokio::spawn(this.run_event_stream(info).in_current_span());
        }
    }

    async fn run_event_stream(mut self, mut info: ContainerLogInfo) {
        // Establish connection
        let options = Some(LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            since: info.log_since(),
            timestamps: true,
            ..Default::default()
        });

        let stream = self.core.docker.logs(info.id.as_str(), options);
        emit!(DockerLogsContainerWatch {
            container_id: info.id.as_str()
        });

        // Create event streamer
        let mut partial_event_merge_state = None;

        let core = Arc::clone(&self.core);

        let bytes_received = register!(BytesReceived::from(Protocol::HTTP));

        let mut error = None;
        let events_stream = stream
            .map(|value| {
                match value {
                    Ok(message) => Ok(info.new_event(
                        message,
                        core.config.partial_event_marker_field.clone(),
                        core.config.auto_partial_merge,
                        &mut partial_event_merge_state,
                        &bytes_received,
                        self.log_namespace,
                    )),
                    Err(error) => {
                        // On any error, restart connection
                        match &error {
                            DockerError::DockerResponseServerError { status_code, .. }
                                if *status_code == http::StatusCode::NOT_IMPLEMENTED =>
                            {
                                emit!(DockerLogsLoggingDriverUnsupportedError {
                                    error,
                                    container_id: info.id.as_str(),
                                });
                                Err(ErrorPersistence::Permanent)
                            }
                            _ => {
                                emit!(DockerLogsCommunicationError {
                                    error,
                                    container_id: Some(info.id.as_str())
                                });
                                Err(ErrorPersistence::Transient)
                            }
                        }
                    }
                }
            })
            .take_while(|v| {
                error = v.as_ref().err().cloned();
                ready(v.is_ok())
            })
            .filter_map(|v| ready(v.ok().flatten()))
            .take_until(self.shutdown.clone());

        let events_stream: Box<dyn Stream<Item = LogEvent> + Unpin + Send> =
            if let Some(ref line_agg_config) = core.line_agg_config {
                Box::new(line_agg_adapter(
                    events_stream,
                    line_agg::Logic::new(line_agg_config.clone()),
                    self.log_namespace,
                ))
            } else {
                Box::new(events_stream)
            };

        let host_key = self.host_key.clone().path;
        let hostname = self.hostname.clone();
        let result = {
            let mut stream = events_stream
                .map(move |event| add_hostname(event, &host_key, &hostname, self.log_namespace));
            self.out.send_event_stream(&mut stream).await.map_err(|_| {
                let (count, _) = stream.size_hint();
                emit!(StreamClosedError { count });
            })
        };

        // End of stream
        emit!(DockerLogsContainerUnwatch {
            container_id: info.id.as_str()
        });

        let result = match (result, error) {
            (Ok(()), None) => Ok(info),
            (Err(()), _) => Err((info.id, ErrorPersistence::Permanent)),
            (_, Some(occurrence)) => Err((info.id, occurrence)),
        };

        self.finish(result);
    }

    fn finish(self, result: Result<ContainerLogInfo, (ContainerId, ErrorPersistence)>) {
        // This can legally fail when shutting down, and any other
        // reason should have been logged in the main future.
        _ = self.main_send.send(result);
    }
}

fn add_hostname(
    mut log: LogEvent,
    host_key: &Option<OwnedValuePath>,
    hostname: &Option<String>,
    log_namespace: LogNamespace,
) -> LogEvent {
    if let Some(hostname) = hostname {
        let legacy_host_key = host_key.as_ref().map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            legacy_host_key,
            path!("host"),
            hostname.clone(),
        );
    }

    log
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ErrorPersistence {
    Transient,
    Permanent,
}

/// Container ID as assigned by Docker.
/// Is actually a string.
#[derive(Hash, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ContainerId(Bytes);

impl ContainerId {
    fn new(id: String) -> Self {
        ContainerId(id.into())
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("Container Id Bytes aren't String")
    }
}

/// Kept by main to keep track of container state
struct ContainerState {
    /// None if there is a event_stream of this container.
    info: Option<ContainerLogInfo>,
    /// True if Container is currently running
    running: bool,
    /// Of running
    generation: u64,
}

impl ContainerState {
    /// It's ContainerLogInfo pair must be created exactly once.
    const fn new_running() -> Self {
        ContainerState {
            info: None,
            running: true,
            generation: 0,
        }
    }

    fn running(&mut self) {
        self.running = true;
        self.generation += 1;
    }

    fn stopped(&mut self) {
        self.running = false;
    }

    const fn is_running(&self) -> bool {
        self.running
    }

    /// True if it needs to be restarted.
    #[must_use]
    fn return_info(&mut self, info: ContainerLogInfo) -> bool {
        debug_assert!(self.info.is_none());
        // Generation is the only one strictly necessary,
        // but with v.running, restarting event_stream is automatically done.
        let restart = self.running || info.generation < self.generation;
        self.info = Some(info);
        restart
    }

    fn take_info(&mut self) -> Option<ContainerLogInfo> {
        self.info.take().map(|mut info| {
            // Update info
            info.generation = self.generation;
            info
        })
    }
}

/// Exchanged between main future and event_stream futures
struct ContainerLogInfo {
    /// Container docker ID
    id: ContainerId,
    /// Timestamp of event which created this struct
    created: DateTime<Utc>,
    /// Timestamp of last log message with it's generation
    last_log: Option<(DateTime<FixedOffset>, u64)>,
    /// generation of ContainerState at event_stream creation
    generation: u64,
    metadata: ContainerMetadata,
}

impl ContainerLogInfo {
    /// Container docker ID
    /// Unix timestamp of event which created this struct
    const fn new(id: ContainerId, metadata: ContainerMetadata, created: DateTime<Utc>) -> Self {
        ContainerLogInfo {
            id,
            created,
            last_log: None,
            generation: 0,
            metadata,
        }
    }

    /// Only logs after or equal to this point need to be fetched
    fn log_since(&self) -> i64 {
        self.last_log
            .as_ref()
            .map(|(d, _)| d.timestamp())
            .unwrap_or_else(|| self.created.timestamp())
            - 1
    }

    /// Expects timestamp at the beginning of message.
    /// Expects messages to be ordered by timestamps.
    fn new_event(
        &mut self,
        log_output: LogOutput,
        partial_event_marker_field: Option<String>,
        auto_partial_merge: bool,
        partial_event_merge_state: &mut Option<LogEventMergeState>,
        bytes_received: &Registered<BytesReceived>,
        log_namespace: LogNamespace,
    ) -> Option<LogEvent> {
        let (stream, mut bytes_message) = match log_output {
            LogOutput::StdErr { message } => (STDERR.clone(), message),
            LogOutput::StdOut { message } => (STDOUT.clone(), message),
            LogOutput::Console { message } => (CONSOLE.clone(), message),
            LogOutput::StdIn { message: _ } => return None,
        };

        bytes_received.emit(ByteSize(bytes_message.len()));

        let message = String::from_utf8_lossy(&bytes_message);
        let mut splitter = message.splitn(2, char::is_whitespace);
        let timestamp_str = splitter.next()?;
        let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(timestamp) => {
                // Timestamp check. This is included to avoid processing the same log multiple times, which can
                // occur when a container changes generations, and to avoid processing logs with timestamps before
                // the created timestamp.
                match self.last_log.as_ref() {
                    Some(&(last, gen)) => {
                        if last < timestamp || (last == timestamp && gen == self.generation) {
                            // Noop - log received in order.
                        } else {
                            // Docker returns logs in order.
                            // If we reach this state, this log is from a previous generation of the container.
                            // It was already processed, so we can safely skip it.
                            trace!(
                                message = "Received log from previous container generation.",
                                log_timestamp = %timestamp_str,
                                last_log_timestamp = %last,
                            );
                            return None;
                        }
                    }
                    None => {
                        if self.created < timestamp.with_timezone(&Utc) {
                            // Noop - first log to process.
                        } else {
                            // Received a log with a timestamp before that provided to the Docker API.
                            // This should not happen, but if it does, we can just ignore these logs.
                            trace!(
                                message = "Received log from before created timestamp.",
                                log_timestamp = %timestamp_str,
                                created_timestamp = %self.created
                            );
                            return None;
                        }
                    }
                }

                self.last_log = Some((timestamp, self.generation));

                let log_len = splitter.next().map(|log| log.len()).unwrap_or(0);
                let remove_len = message.len() - log_len;
                bytes_message.advance(remove_len);

                // Provide the timestamp.
                Some(timestamp.with_timezone(&Utc))
            }
            Err(error) => {
                // Received bad timestamp, if any at all.
                emit!(DockerLogsTimestampParseError {
                    error,
                    container_id: self.id.as_str()
                });
                // So continue normally but without a timestamp.
                None
            }
        };

        // Message is actually one line from stderr or stdout, and they are
        // delimited with newline, so that newline needs to be removed.
        // If there's no newline, the event is considered partial, and will
        // either be merged within the docker source, or marked accordingly
        // before sending out, depending on the configuration.
        let is_partial = if bytes_message
            .last()
            .map(|&b| b as char == '\n')
            .unwrap_or(false)
        {
            bytes_message.truncate(bytes_message.len() - 1);
            if bytes_message
                .last()
                .map(|&b| b as char == '\r')
                .unwrap_or(false)
            {
                bytes_message.truncate(bytes_message.len() - 1);
            }
            false
        } else {
            true
        };

        // Build the log.
        let deserializer = BytesDeserializer;
        let mut log = deserializer.parse_single(bytes_message, log_namespace);

        // Container ID
        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(CONTAINER))),
            path!(CONTAINER),
            self.id.0.clone(),
        );
        // Container image
        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(IMAGE))),
            path!(IMAGE),
            self.metadata.image.clone(),
        );
        // Container name
        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(NAME))),
            path!(NAME),
            self.metadata.name.clone(),
        );
        // Created at timestamp
        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(CREATED_AT))),
            path!(CREATED_AT),
            self.metadata.created_at,
        );
        // Labels
        if !self.metadata.labels.is_empty() {
            for (key, value) in self.metadata.labels.iter() {
                log_namespace.insert_source_metadata(
                    DockerLogsConfig::NAME,
                    &mut log,
                    Some(LegacyKey::Overwrite(path!("label", key))),
                    path!("labels", key),
                    value.clone(),
                )
            }
        }
        log_namespace.insert_source_metadata(
            DockerLogsConfig::NAME,
            &mut log,
            Some(LegacyKey::Overwrite(path!(STREAM))),
            path!(STREAM),
            stream,
        );

        log_namespace.insert_vector_metadata(
            &mut log,
            log_schema().source_type_key(),
            path!("source_type"),
            Bytes::from_static(DockerLogsConfig::NAME.as_bytes()),
        );

        // This handles the transition from the original timestamp logic. Originally the
        // `timestamp_key` was only populated when a timestamp was parsed from the event.
        match log_namespace {
            LogNamespace::Vector => {
                if let Some(timestamp) = timestamp {
                    log.insert(
                        metadata_path!(DockerLogsConfig::NAME, "timestamp"),
                        timestamp,
                    );
                }

                log.insert(metadata_path!("vector", "ingest_timestamp"), Utc::now());
            }
            LogNamespace::Legacy => {
                if let Some(timestamp) = timestamp {
                    if let Some(timestamp_key) = log_schema().timestamp_key() {
                        log.try_insert((PathPrefix::Event, timestamp_key), timestamp);
                    }
                }
            }
        };

        // If automatic partial event merging is requested - perform the
        // merging.
        // Otherwise mark partial events and return all the events with no
        // merging.
        let log = if auto_partial_merge {
            // Partial event events merging logic.

            // If event is partial, stash it and return `None`.
            if is_partial {
                // If we already have a partial event merge state, the current
                // message has to be merged into that existing state.
                // Otherwise, create a new partial event merge state with the
                // current message being the initial one.
                if let Some(partial_event_merge_state) = partial_event_merge_state {
                    // Depending on the log namespace the actual contents of the log "message" will be
                    // found in either the root of the event ("."), or at the globally configured "message_key".
                    match log_namespace {
                        LogNamespace::Vector => {
                            partial_event_merge_state.merge_in_next_event(log, &["."]);
                        }
                        LogNamespace::Legacy => {
                            partial_event_merge_state.merge_in_next_event(
                                log,
                                &[log_schema()
                                    .message_key()
                                    .expect("global log_schema.message_key to be valid path")
                                    .to_string()],
                            );
                        }
                    }
                } else {
                    *partial_event_merge_state = Some(LogEventMergeState::new(log));
                };
                return None;
            };

            // This is not a partial event. If we have a partial event merge
            // state from before, the current event must be a final event, that
            // would give us a merged event we can return.
            // Otherwise it's just a regular event that we return as-is.
            match partial_event_merge_state.take() {
                // Depending on the log namespace the actual contents of the log "message" will be
                // found in either the root of the event ("."), or at the globally configured "message_key".
                Some(partial_event_merge_state) => match log_namespace {
                    LogNamespace::Vector => {
                        partial_event_merge_state.merge_in_final_event(log, &["."])
                    }
                    LogNamespace::Legacy => partial_event_merge_state.merge_in_final_event(
                        log,
                        &[log_schema()
                            .message_key()
                            .expect("global log_schema.message_key to be valid path")
                            .to_string()],
                    ),
                },
                None => log,
            }
        } else {
            // If the event is partial, just set the partial event marker field.
            if is_partial {
                // Only add partial event marker field if it's requested.
                if let Some(partial_event_marker_field) = partial_event_marker_field {
                    log_namespace.insert_source_metadata(
                        DockerLogsConfig::NAME,
                        &mut log,
                        Some(LegacyKey::Overwrite(path!(
                            partial_event_marker_field.as_str()
                        ))),
                        path!(event::PARTIAL),
                        true,
                    );
                }
            }
            // Return the log event as is, partial or not. No merging here.
            log
        };

        // Partial or not partial - we return the event we got here, because all
        // other cases were handled earlier.
        emit!(DockerLogsEventsReceived {
            byte_size: log.estimated_json_encoded_size_of(),
            container_id: self.id.as_str(),
            container_name: &self.metadata.name_str
        });

        Some(log)
    }
}

struct ContainerMetadata {
    /// label.key -> String
    labels: HashMap<String, String>,
    /// name -> String
    name: Value,
    /// name
    name_str: String,
    /// image -> String
    image: Value,
    /// created_at
    created_at: DateTime<Utc>,
}

impl ContainerMetadata {
    fn from_details(details: ContainerInspectResponse) -> Result<Self, ParseError> {
        let config = details.config.unwrap();
        let name = details.name.unwrap();
        let created = details.created.unwrap();

        let labels = config.labels.unwrap_or_default();

        Ok(ContainerMetadata {
            labels,
            name: name.as_str().trim_start_matches('/').to_owned().into(),
            name_str: name,
            image: config.image.unwrap().into(),
            created_at: DateTime::parse_from_rfc3339(created.as_str())?.with_timezone(&Utc),
        })
    }
}

fn line_agg_adapter(
    inner: impl Stream<Item = LogEvent> + Unpin,
    logic: line_agg::Logic<Bytes, LogEvent>,
    log_namespace: LogNamespace,
) -> impl Stream<Item = LogEvent> {
    let line_agg_in = inner.map(move |mut log| {
        let message_value = match log_namespace {
            LogNamespace::Vector => log
                .remove(event_path!())
                .expect("`.` must exist in the event"),
            LogNamespace::Legacy => log
                .remove(
                    log_schema()
                        .message_key_target_path()
                        .expect("global log_schema.message_key to be valid path"),
                )
                .expect("`message` must exist in the event"),
        };
        let stream_value = match log_namespace {
            LogNamespace::Vector => log
                .get(metadata_path!(DockerLogsConfig::NAME, STREAM))
                .expect("`docker_logs.stream` must exist in the metadata"),
            LogNamespace::Legacy => log
                .get(event_path!(STREAM))
                .expect("stream must exist in the event"),
        };

        let stream = stream_value.coerce_to_bytes();
        let message = message_value.coerce_to_bytes();
        (stream, message, log)
    });
    let line_agg_out = LineAgg::<_, Bytes, LogEvent>::new(line_agg_in, logic);
    line_agg_out.map(move |(_, message, mut log, _)| {
        match log_namespace {
            LogNamespace::Vector => log.insert(event_path!(), message),
            LogNamespace::Legacy => log.insert(
                log_schema()
                    .message_key_target_path()
                    .expect("global log_schema.message_key to be valid path"),
                message,
            ),
        };
        log
    })
}
