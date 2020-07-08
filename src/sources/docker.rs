use crate::{
    event::merge_state::LogEventMergeState,
    event::{self, Event, LogEvent, Value},
    shutdown::ShutdownSignal,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bollard::{
    container::{InspectContainerOptions, ListContainersOptions, LogOutput, LogsOptions},
    errors::{Error as DockerError, ErrorKind as DockerErrorKind},
    service::{ContainerInspectResponse, SystemEventsResponse},
    system::EventsOptions,
    Docker,
};
use bytes05::{Buf, Bytes};
use chrono::{DateTime, FixedOffset, Local, NaiveTime, Utc, MAX_DATE};
use futures::{
    compat::{Future01CompatExt, Sink01CompatExt},
    future,
    sink::SinkExt,
    FutureExt, Stream, StreamExt, TryFutureExt,
};
use futures01::sync::mpsc as mpsc01;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::{collections::HashMap, env};
use string_cache::DefaultAtom as Atom;
use tokio::sync::mpsc;
use tracing::field;

/// The begining of image names of vector docker images packaged by vector.
const VECTOR_IMAGE_NAME: &str = "timberio/vector";

lazy_static! {
    static ref STDERR: Bytes = "stderr".into();
    static ref STDOUT: Bytes = "stdout".into();
    static ref IMAGE: Atom = Atom::from("image");
    static ref CREATED_AT: Atom = Atom::from("container_created_at");
    static ref NAME: Atom = Atom::from("container_name");
    static ref STREAM: Atom = Atom::from("stream");
    static ref CONTAINER: Atom = Atom::from("container_id");
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct DockerConfig {
    include_containers: Option<Vec<String>>, // Starts with actually, not include
    include_labels: Option<Vec<String>>,
    include_images: Option<Vec<String>>,
    partial_event_marker_field: Option<Atom>,
    auto_partial_merge: bool,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            include_containers: None,
            include_labels: None,
            include_images: None,
            partial_event_marker_field: Some(event::PARTIAL.clone()),
            auto_partial_merge: true,
        }
    }
}

impl DockerConfig {
    fn container_name_included<'a>(
        &self,
        id: &str,
        names: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        if let Some(include_containers) = &self.include_containers {
            let id_flag = include_containers
                .iter()
                .any(|include| id.starts_with(include));

            let name_flag = names.into_iter().any(|name| {
                include_containers
                    .iter()
                    .any(|include| name.starts_with(include))
            });

            id_flag || name_flag
        } else {
            true
        }
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

inventory::submit! {
    SourceDescription::new::<DockerConfig>("docker")
}

#[typetag::serde(name = "docker")]
impl SourceConfig for DockerConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc01::Sender<Event>,
    ) -> crate::Result<super::Source> {
        let source = DockerSource::new(
            self.clone().with_empty_partial_event_marker_field_as_none(),
            out,
            shutdown.clone(),
        )?;

        // Capture currently running containers, and do main future(run)
        let fut = async move {
            match source.handle_running_containers().await {
                Ok(source) => source.run().await,
                Err(error) => {
                    error!(message = "Listing currently running containers, failed", %error);
                }
            }
        };

        // Once this ShutdownSignal resolves it will drop DockerSource and by extension it's ShutdownSignal.
        Ok(Box::new(
            async move {
                Ok(tokio::select! {
                    _ = fut => {}
                    _ = shutdown.compat() => {}
                })
            }
            .boxed()
            .compat(),
        ))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "docker"
    }
}

struct DockerSourceCore {
    config: DockerConfig,
    docker: Docker,
    /// Only logs created at, or after this moment are logged.
    now_timestamp: DateTime<Utc>,
}

impl DockerSourceCore {
    fn new(config: DockerConfig) -> crate::Result<Self> {
        // ?NOTE: Constructs a new Docker instance for a docker host listening at url specified by an env var DOCKER_HOST.
        // ?      Otherwise connects to unix socket which requires sudo privileges, or docker group membership.
        let docker = docker()?;

        // Only log events created at-or-after this moment are logged.
        let now = Local::now();
        info!(
            message = "Capturing logs from now on",
            now = %now.to_rfc3339()
        );

        Ok(DockerSourceCore {
            config,
            docker,
            now_timestamp: now.into(),
        })
    }

    /// Returns event stream coming from docker.
    fn docker_event_stream(
        &self,
    ) -> impl Stream<Item = Result<SystemEventsResponse, DockerError>> + Send {
        let mut filters = HashMap::new();

        // event  | emmited on commands
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

        // Apply include filters
        if let Some(include_labels) = &self.config.include_labels {
            filters.insert("label".to_owned(), include_labels.clone());
        }

        if let Some(include_images) = &self.config.include_images {
            filters.insert("image".to_owned(), include_images.clone());
        }

        self.docker.events(Some(EventsOptions {
            since: self.now_timestamp,
            // Handler in Docker API:
            // https://github.com/moby/moby/blob/c833222d54c00d64a0fc44c561a5973ecd414053/api/server/router/system/system_routes.go#L155
            until: MAX_DATE.and_time(NaiveTime::from_hms(0, 0, 0)).unwrap(),
            filters,
        }))
    }
}

/// Main future which listens for events coming from docker, and maintains
/// a fan of event_stream futures.
/// Where each event_stream corresponds to a runing container marked with ContainerLogInfo.
/// While running, event_stream streams Events to out channel.
/// Once a log stream has ended, it sends ContainerLogInfo back to main.
///
/// Future  channel     Future      channel
///           |<---- event_stream ---->out
/// main <----|<---- event_stream ---->out
///           | ...                 ...out
///
struct DockerSource {
    esb: EventStreamBuilder,
    /// event stream from docker
    events: Pin<Box<dyn Stream<Item = Result<SystemEventsResponse, DockerError>> + Send>>,
    ///  mappings of seen container_id to their data
    containers: HashMap<ContainerId, ContainerState>,
    ///receives ContainerLogInfo comming from event stream futures
    main_recv: mpsc::UnboundedReceiver<ContainerLogInfo>,
    /// It may contain shortened container id.
    hostname: Option<String>,
    /// True if self needs to be excluded
    exclude_self: bool,
}

impl DockerSource {
    fn new(
        config: DockerConfig,
        out: mpsc01::Sender<Event>,
        shutdown: ShutdownSignal,
    ) -> crate::Result<DockerSource> {
        // Find out it's own container id, if it's inside a docker container.
        // Since docker doesn't readily provide such information,
        // various approches need to be made. As such the solution is not
        // exact, but probable.
        // This is to be used only if source is in state of catching everything.
        // Or in other words, if includes are used then this is not necessary.
        let exclude_self = config
            .include_containers
            .clone()
            .unwrap_or_default()
            .is_empty()
            && config.include_labels.clone().unwrap_or_default().is_empty();

        // Only logs created at, or after this moment are logged.
        let core = DockerSourceCore::new(config)?;

        // main event stream, with whom only newly started/restarted containers will be loged.
        let events = core.docker_event_stream();
        info!(message = "Listening docker events");

        // Channel of communication between main future and event_stream futures
        let (main_send, main_recv) = mpsc::unbounded_channel::<ContainerLogInfo>();

        // Starting with logs from now.
        // TODO: Is this exception acceptable?
        // Only somewhat exception to this is case where:
        // t0 -- outside: container running
        // t1 -- now_timestamp
        // t2 -- outside: container stoped
        // t3 -- list_containers
        // In that case, logs between [t1,t2] will be pulled to vector only on next start/unpause of that container.
        let esb = EventStreamBuilder {
            core: Arc::new(core),
            out,
            main_send,
            shutdown,
        };

        Ok(DockerSource {
            esb,
            events: Box::pin(events),
            containers: HashMap::new(),
            main_recv,
            hostname: env::var("HOSTNAME").ok(),
            exclude_self,
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
                let image = container.image.unwrap();

                trace!(
                    message = "Found already running container",
                    id = field::display(&id),
                    names = field::debug(&names)
                );

                if !self.exclude_vector(id.as_str(), image.as_str()) {
                    return;
                }

                if !self.esb.core.config.container_name_included(
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
                    trace!(message = "Container excluded", id = field::display(&id));
                    return;
                }

                let id = ContainerId::new(id);
                self.containers.insert(id.clone(), self.esb.start(id));
            });

        Ok(self)
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                value = self.main_recv.next() => {
                    match value {
                        Some(info) => {
                            let state = self
                                .containers
                                .get_mut(&info.id)
                                .expect("Every ContainerLogInfo has it's ContainerState");
                            if state.return_info(info) {
                                self.esb.restart(state);
                            }
                        }
                        None => {
                            error!(message = "docker source main stream has ended unexpectedly");
                            info!(message = "Shuting down docker source");
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

                            trace!(
                                message = "docker event",
                                id = field::display(&id),
                                action = field::display(&action),
                                timestamp = field::display(event.time.unwrap()),
                                attributes = field::debug(&attributes),
                            );
                            let id = ContainerId::new(id);

                            // Update container status
                            match action.as_str() {
                                "die" | "pause" => {
                                    if let Some(state) = self.containers.get_mut(&id) {
                                        state.stoped();
                                    }
                                }
                                "start" | "unpause" => {
                                    if let Some(state) = self.containers.get_mut(&id) {
                                        state.running();
                                        self.esb.restart(state);
                                    } else {
                                        let include_name =
                                            self.esb.core.config.container_name_included(
                                                id.as_str(),
                                                attributes.get("name").map(|s| s.as_str()),
                                            );

                                        let self_check = self.exclude_vector(
                                            id.as_str(),
                                            attributes.get("image").map(|s| s.as_str()),
                                        );

                                        if include_name && self_check {
                                            self.containers.insert(id.clone(), self.esb.start(id));
                                        }
                                    }
                                }
                                _ => {},
                            };
                        }
                        Some(Err(error)) => error!(source = "docker events", %error),
                        None => {
                            // TODO: this could be fixed, but should be tryed with some timeoff and exponential backoff
                            error!(message = "docker event stream has ended unexpectedly");
                            info!(message = "Shuting down docker source");
                            return;
                        }
                    };
                }
            };
        }
    }

    /// True if container with the given id and image must be excluded from logging,
    /// because it's a vector instance, probably this one.
    fn exclude_vector<'a>(&self, id: &str, image: impl Into<Option<&'a str>>) -> bool {
        if self.exclude_self {
            let hostname_hint = self
                .hostname
                .as_ref()
                .map(|maybe_short_id| id.starts_with(maybe_short_id))
                .unwrap_or(false);
            let image_hint = image
                .into()
                .map(|image| image.starts_with(VECTOR_IMAGE_NAME))
                .unwrap_or(false);
            if hostname_hint || image_hint {
                // This container is probably itself.
                info!(message = "Detected self container", id);
                return false;
            }
        }
        true
    }
}

/// Used to construct and start event stream futures
#[derive(Clone)]
struct EventStreamBuilder {
    core: Arc<DockerSourceCore>,
    /// Event stream futures send events through this
    out: mpsc01::Sender<Event>,
    /// End through which event stream futures send ContainerLogInfo to main future
    main_send: mpsc::UnboundedSender<ContainerLogInfo>,
    /// Self and event streams will end on this.
    shutdown: ShutdownSignal,
}

impl EventStreamBuilder {
    /// Constructs and runs event stream until shutdown.
    fn start(&self, id: ContainerId) -> ContainerState {
        let this = self.clone();
        tokio::spawn(async move {
            Arc::clone(&this.core)
                .docker
                .inspect_container(id.clone().as_str(), None::<InspectContainerOptions>)
                .map_err(|error| error!(message = "Fetching container details failed", %error))
                .and_then(|details| async move {
                    ContainerMetadata::from_details(details)
                        .map_err(|error| error!(message = "Metadata extraction failed", %error))
                })
                .and_then(|metadata| async move {
                    let info = ContainerLogInfo::new(id, metadata, this.core.now_timestamp);
                    this.start_event_stream(info).await;
                    Ok::<(), ()>(())
                })
                .await
        });

        ContainerState::new()
    }

    /// If info is present, restarts event stream which will run until shutdown.
    fn restart(&self, container: &mut ContainerState) {
        if let Some(info) = container.take_info() {
            let this = self.clone();
            tokio::spawn(async move { this.start_event_stream(info).await });
        }
    }

    async fn start_event_stream(&self, mut info: ContainerLogInfo) {
        // Establish connection
        let options = Some(LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            since: info.log_since(),
            timestamps: true,
            ..Default::default()
        });

        let stream = self.core.docker.logs(info.id.as_str(), options);
        info!(
            message = "Started listening logs on docker container",
            id = field::display(info.id.as_str())
        );

        // Create event streamer
        let main_send = self.main_send.clone();
        let partial_event_marker_field = self.core.config.partial_event_marker_field.clone();
        let auto_partial_merge = self.core.config.auto_partial_merge;
        let mut partial_event_merge_state = None;

        stream
            .map(|value| {
                match value {
                    Ok(message) => Ok(info.new_event(
                        message,
                        partial_event_marker_field.clone(),
                        auto_partial_merge,
                        &mut partial_event_merge_state,
                    )),
                    Err(error) => {
                        // On any error, restart connection
                        match error.kind() {
                            DockerErrorKind::DockerResponseServerError { status_code, .. }
                                if *status_code == http::StatusCode::NOT_IMPLEMENTED =>
                            {
                                error!(
                                    r#"docker engine is not using either `jsonfile` or `journald`
                                        logging driver. Please enable one of these logging drivers
                                        to get logs from the docker daemon."#
                                )
                            }
                            _ => error!(message = "docker API container logging error", %error),
                        };

                        Err(())
                    }
                }
            })
            .take_while(|v| future::ready(v.is_ok()))
            .filter_map(|v| future::ready(v.transpose()))
            .take_until(self.shutdown.clone().compat())
            .forward(self.out.clone().sink_compat().sink_map_err(|_| ()))
            .map(|_| {})
            .await;

        // End of stream
        info!(
            message = "Stoped listening logs on docker container",
            id = field::display(info.id.as_str())
        );

        if let Err(error) = main_send.send(info) {
            error!(message = "Unable to return ContainerLogInfo to main", %error);
        }
    }
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
    fn new() -> Self {
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

    fn stoped(&mut self) {
        self.running = false;
    }

    /// True if it needs to be restarted.
    #[must_use]
    fn return_info(&mut self, info: ContainerLogInfo) -> bool {
        debug_assert!(self.info.is_none());
        // Generation is the only one strictly necessary,
        // but with v.running, restarting event_stream is automtically done.
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
    fn new(id: ContainerId, metadata: ContainerMetadata, created: DateTime<Utc>) -> Self {
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
            .map(|&(ref d, _)| d.timestamp())
            .unwrap_or_else(|| self.created.timestamp())
            - 1
    }

    /// Expects timestamp at the begining of message.
    /// Expects messages to be ordered by timestamps.
    fn new_event(
        &mut self,
        log_output: LogOutput,
        partial_event_marker_field: Option<Atom>,
        auto_partial_merge: bool,
        partial_event_merge_state: &mut Option<LogEventMergeState>,
    ) -> Option<Event> {
        let (stream, mut bytes_message) = match log_output {
            LogOutput::StdErr { message } => (STDERR.clone(), message),
            LogOutput::StdOut { message } => (STDOUT.clone(), message),
            _ => return None,
        };

        let message = String::from_utf8_lossy(&bytes_message);
        let mut splitter = message.splitn(2, char::is_whitespace);
        let timestamp_str = splitter.next()?;
        let timestamp = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(timestamp) => {
                // Timestamp check
                match self.last_log.as_ref() {
                    // Recieved log has not already been processed
                    Some(&(ref last, gen))
                        if *last < timestamp || (*last == timestamp && gen == self.generation) =>
                    {
                        // noop
                    }
                    // Recieved log is not from before of creation
                    None if self.created <= timestamp.with_timezone(&Utc) => (),
                    _ => {
                        trace!(
                            message = "Recieved older log",
                            timestamp = %timestamp_str
                        );
                        return None;
                    }
                }

                self.last_log = Some((timestamp, self.generation));

                let log = splitter.next()?;
                let remove_len = message.len() - log.len();
                bytes_message.advance(remove_len);

                // Provide the timestamp.
                Some(timestamp.with_timezone(&Utc))
            }
            Err(error) => {
                // Recieved bad timestamp, if any at all.
                error!(message="Didn't recieve rfc3339 timestamp from docker",%error);
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
            false
        } else {
            true
        };

        // Prepare the log event.
        let mut log_event = {
            let mut log_event = LogEvent::default();

            // Source type
            log_event.insert(event::log_schema().source_type_key(), "docker");

            // The log message.
            log_event.insert(event::log_schema().message_key().clone(), bytes_message);

            // Stream we got the message from.
            log_event.insert(STREAM.clone(), stream);

            // Timestamp of the event.
            if let Some(timestamp) = timestamp {
                log_event.insert(event::log_schema().timestamp_key().clone(), timestamp);
            }

            // Container ID.
            log_event.insert(CONTAINER.clone(), self.id.0.clone());

            // Labels.
            for (key, value) in self.metadata.labels.iter() {
                log_event.insert(key.clone(), value.clone());
            }

            // Container name.
            log_event.insert(NAME.clone(), self.metadata.name.clone());

            // Container image.
            log_event.insert(IMAGE.clone(), self.metadata.image.clone());

            // Timestamp of the container creation.
            log_event.insert(CREATED_AT.clone(), self.metadata.created_at);

            // Return the resulting log event.
            log_event
        };

        // If automatic partial event merging is requested - perform the
        // merging.
        // Otherwise mark partial events and return all the events with no
        // merging.
        let log_event = if auto_partial_merge {
            // Partial event events merging logic.

            // If event is partial, stash it and return `None`.
            if is_partial {
                // If we already have a partial event merge state, the current
                // message has to be merged into that existing state.
                // Otherwise, create a new partial event merge state with the
                // current message being the initial one.
                if let Some(partial_event_merge_state) = partial_event_merge_state {
                    partial_event_merge_state.merge_in_next_event(
                        log_event,
                        &[event::log_schema().message_key().clone()],
                    );
                } else {
                    *partial_event_merge_state = Some(LogEventMergeState::new(log_event));
                };
                return None;
            };

            // This is not a parial event. If we have a partial event merge
            // state from before, the current event must be a final event, that
            // would give us a merged event we can return.
            // Otherwise it's just a regular event that we return as-is.
            match partial_event_merge_state.take() {
                Some(partial_event_merge_state) => partial_event_merge_state
                    .merge_in_final_event(log_event, &[event::log_schema().message_key().clone()]),
                None => log_event,
            }
        } else {
            // If the event is partial, just set the partial event marker field.
            if is_partial {
                // Only add partial event marker field if it's requested.
                if let Some(partial_event_marker_field) = partial_event_marker_field {
                    log_event.insert(partial_event_marker_field, true);
                }
            }
            // Return the log event as is, partial or not. No merging here.
            log_event
        };

        // Partial or not partial - we return the event we got here, because all
        // other cases were handeled earlier.
        let event = Event::Log(log_event);
        trace!(message = "Received one event.", ?event);
        Some(event)
    }
}

struct ContainerMetadata {
    /// label.key -> String
    labels: Vec<(Atom, Value)>,
    /// name -> String
    name: Value,
    /// image -> String
    image: Value,
    /// created_at
    created_at: DateTime<Utc>,
}

impl ContainerMetadata {
    fn from_details(details: ContainerInspectResponse) -> crate::Result<Self> {
        let config = details.config.unwrap();
        let name = details.name.unwrap();
        let created = details.created.unwrap();

        let labels = config
            .labels
            .as_ref()
            .map(|map| {
                map.iter()
                    .map(|(key, value)| (("label.".to_owned() + key).into(), value.as_str().into()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(ContainerMetadata {
            labels,
            name: name.as_str().trim_start_matches('/').into(),
            image: config.image.unwrap().as_str().into(),
            created_at: DateTime::parse_from_rfc3339(created.as_str())?.with_timezone(&Utc),
        })
    }
}

fn docker() -> Result<Docker, DockerError> {
    let scheme = env::var("DOCKER_HOST").ok().and_then(|host| {
        let uri = host.parse::<hyper::Uri>().expect("invalid url");
        uri.into_parts().scheme
    });

    match scheme.as_ref().map(|s| s.as_str()) {
        Some("http") => Docker::connect_with_http_defaults(),
        Some("https") => Docker::connect_with_tls_defaults(),
        _ => Docker::connect_with_local_defaults(),
    }
}

#[cfg(all(test, feature = "docker-integration-tests"))]
mod tests {
    use super::*;
    use crate::runtime::Runtime;
    use crate::test_util::{collect_n, runtime, trace_init};
    use bollard::{
        container::{
            Config as ContainerConfig, CreateContainerOptions, KillContainerOptions,
            RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
        },
        image::{CreateImageOptions, CreateImageResults, ListImagesOptions},
    };
    use futures::{compat::Future01CompatExt, stream::TryStreamExt};
    use futures01::{Async, Stream as Stream01};

    /// None if docker is not present on the system
    fn source_with<'a, L: Into<Option<&'a str>>>(
        names: &[&str],
        label: L,
        rt: &mut Runtime,
    ) -> mpsc01::Receiver<Event> {
        source_with_config(
            DockerConfig {
                include_containers: Some(names.iter().map(|&s| s.to_owned()).collect()),
                include_labels: Some(label.into().map(|l| vec![l.to_owned()]).unwrap_or_default()),
                ..DockerConfig::default()
            },
            rt,
        )
    }

    /// None if docker is not present on the system
    fn source_with_config(config: DockerConfig, rt: &mut Runtime) -> mpsc01::Receiver<Event> {
        // trace_init();
        let (sender, recv) = mpsc01::channel(100);
        rt.spawn(
            config
                .build(
                    "default",
                    &GlobalOptions::default(),
                    ShutdownSignal::noop(),
                    sender,
                )
                .unwrap(),
        );
        recv
    }

    /// Users should ensure to remove container before exiting.
    async fn log_container(name: &str, label: Option<&str>, log: &str, docker: &Docker) -> String {
        cmd_container(name, label, vec!["echo", log], docker).await
    }

    /// Users should ensure to remove container before exiting.
    /// Will resend message every so often.
    async fn eternal_container(
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
    ) -> String {
        cmd_container(
            name,
            label,
            vec![
                "sh",
                "-c",
                format!("echo before; i=0; while [ $i -le 50 ]; do sleep 0.1; echo {}; i=$((i+1)); done", log).as_str(),
            ],
            docker,
        ).await
    }

    /// Users should ensure to remove container before exiting.
    async fn cmd_container(
        name: &str,
        label: Option<&str>,
        cmd: Vec<&str>,
        docker: &Docker,
    ) -> String {
        if let Some(id) = cmd_container_for_real(name, label, cmd, docker).await {
            id
        } else {
            // Maybe a before created container is present
            info!(
                message = "Assums that named container remained from previous tests",
                name = name
            );
            name.to_owned()
        }
    }

    /// Users should ensure to remove container before exiting.
    async fn cmd_container_for_real(
        name: &str,
        label: Option<&str>,
        cmd: Vec<&str>,
        docker: &Docker,
    ) -> Option<String> {
        pull_busybox(docker).await;

        trace!("Creating container");

        let options = Some(CreateContainerOptions { name });
        let config = ContainerConfig {
            image: Some("busybox"),
            cmd: Some(cmd),
            labels: label.map(|label| vec![(label, "")].into_iter().collect()),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await;
        container.ok().map(|c| c.id)
    }

    /// Polling busybox image
    async fn pull_busybox(docker: &Docker) {
        let mut filters = HashMap::new();
        filters.insert("reference", vec!["busybox:latest"]);

        let options = Some(ListImagesOptions {
            filters,
            ..Default::default()
        });

        let images = docker.list_images(options).await.unwrap();
        if images.is_empty() {
            // If `busybox:latest` not found, pull it
            let options = Some(CreateImageOptions {
                from_image: "busybox",
                tag: "latest",
                ..Default::default()
            });

            docker
                .create_image(options, None, None)
                .for_each(|item| async move {
                    match item.unwrap() {
                        err @ CreateImageResults::CreateImageError { .. } => panic!("{:?}", err),
                        _ => {}
                    }
                })
                .await
        }
    }

    /// Returns once container has started
    async fn container_start(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Starting container");

        let options = None::<StartContainerOptions<&str>>;
        docker.start_container(id, options).await
    }

    /// Returns once container is done running
    async fn container_wait(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Waiting container");

        docker
            .wait_container(id, None::<WaitContainerOptions<&str>>)
            .try_for_each(|exit| async move {
                info!("Container exited with status code: {}", exit.status_code);
                Ok(())
            })
            .await
    }

    /// Returns once container is killed
    async fn container_kill(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Waiting container");

        docker
            .kill_container(id, None::<KillContainerOptions<&str>>)
            .await
    }

    /// Returns once container is done running
    async fn container_run(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        container_start(id, docker).await?;
        container_wait(id, docker).await
    }

    async fn container_remove(id: &str, docker: &Docker) {
        trace!("Removing container");

        // Don't panick, as this is unreleated to test, and there possibly other containers that need to be removed
        let _ = docker
            .remove_container(id, None::<RemoveContainerOptions>)
            .await
            .map_err(|e| error!(%e));
    }

    /// Returns once it's certain that log has been made
    /// Expects that this is the only one with a container
    async fn container_log_n(
        n: usize,
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
    ) -> String {
        let id = log_container(name, label, log, docker).await;
        for _ in 0..n {
            if let Err(error) = container_run(&id, docker).await {
                container_remove(&id, docker).await;
                panic!("Container failed to start with error: {:?}", error);
            }
        }
        id
    }

    /// Once function returns, the container has entered into running state.
    /// Container must be killed before removed.
    fn running_container(
        name: &'static str,
        label: Option<&'static str>,
        log: &'static str,
        docker: &Docker,
        rt: &mut Runtime,
    ) -> String {
        let out = source_with(&[name], None, rt);
        let docker = docker.clone();

        rt.block_on_std(async move {
            let id = eternal_container(name, label, log, &docker).await;
            if let Err(error) = container_start(&id, &docker).await {
                container_remove(&id, &docker).await;
                panic!("Container start failed with error: {:?}", error);
            }

            // Wait for before message
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                "before".into()
            );

            id
        })
    }

    async fn is_empty<T>(mut rx: mpsc01::Receiver<T>) -> Result<bool, ()> {
        futures01::future::poll_fn(move || Ok(Async::Ready(rx.poll()?.is_not_ready())))
            .compat()
            .await
    }

    #[test]
    fn newly_started() {
        trace_init();

        let message = "9";
        let name = "vector_test_newly_started";
        let label = "vector_test_label_newly_started";

        let mut rt = runtime();
        let out = source_with(&[name], None, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id = container_log_n(1, name, Some(label), message, &docker).await;
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            container_remove(&id, &docker).await;

            let log = events[0].as_log();
            assert_eq!(log[&event::log_schema().message_key()], message.into());
            assert_eq!(log[&super::CONTAINER], id.into());
            assert!(log.get(&super::CREATED_AT).is_some());
            assert_eq!(log[&super::IMAGE], "busybox".into());
            assert!(log.get(&format!("label.{}", label).into()).is_some());
            assert_eq!(events[0].as_log()[&super::NAME], name.into());
            assert_eq!(
                events[0].as_log()[event::log_schema().source_type_key()],
                "docker".into()
            );
        });
    }

    #[test]
    fn restart() {
        trace_init();

        let message = "10";
        let name = "vector_test_restart";

        let mut rt = runtime();
        let out = source_with(&[name], None, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id = container_log_n(2, name, None, message, &docker).await;
            let events = collect_n(out, 2).compat().await.ok().unwrap();
            container_remove(&id, &docker).await;

            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                message.into()
            );
            assert_eq!(
                events[1].as_log()[&event::log_schema().message_key()],
                message.into()
            );
        });
    }

    #[test]
    fn include_containers() {
        trace_init();

        let message = "11";
        let name0 = "vector_test_include_container_0";
        let name1 = "vector_test_include_container_1";

        let mut rt = runtime();
        let out = source_with(&[name1], None, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id0 = container_log_n(1, name0, None, "13", &docker).await;
            let id1 = container_log_n(1, name1, None, message, &docker).await;
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            container_remove(&id0, &docker).await;
            container_remove(&id1, &docker).await;

            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                message.into()
            );
        });
    }

    #[test]
    fn include_labels() {
        trace_init();

        let message = "12";
        let name0 = "vector_test_include_labels_0";
        let name1 = "vector_test_include_labels_1";
        let label = "vector_test_include_label";

        let mut rt = runtime();
        let out = source_with(&[name0, name1], label, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id0 = container_log_n(1, name0, None, "13", &docker).await;
            let id1 = container_log_n(1, name1, Some(label), message, &docker).await;
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            container_remove(&id0, &docker).await;
            container_remove(&id1, &docker).await;

            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                message.into()
            );
        });
    }

    #[test]
    fn currently_running() {
        trace_init();

        let message = "14";
        let name = "vector_test_currently_running";
        let label = "vector_test_label_currently_running";

        let mut rt = runtime();
        let docker = docker().unwrap();
        let id = running_container(name, Some(label), message, &docker, &mut rt);
        let out = source_with(&[name], None, &mut rt);

        rt.block_on_std(async move {
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            let _ = container_kill(&id, &docker).await;
            container_remove(&id, &docker).await;

            let log = events[0].as_log();
            assert_eq!(log[&event::log_schema().message_key()], message.into());
            assert_eq!(log[&super::CONTAINER], id.into());
            assert!(log.get(&super::CREATED_AT).is_some());
            assert_eq!(log[&super::IMAGE], "busybox".into());
            assert!(log.get(&format!("label.{}", label).into()).is_some());
            assert_eq!(events[0].as_log()[&super::NAME], name.into());
            assert_eq!(
                events[0].as_log()[event::log_schema().source_type_key()],
                "docker".into()
            );
        });
    }

    #[test]
    fn include_image() {
        trace_init();

        let message = "15";
        let name = "vector_test_include_image";
        let config = DockerConfig {
            include_containers: Some(vec![name.to_owned()]),
            include_images: Some(vec!["busybox".to_owned()]),
            ..DockerConfig::default()
        };

        let mut rt = runtime();
        let out = source_with_config(config, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id = container_log_n(1, name, None, message, &docker).await;
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            container_remove(&id, &docker).await;

            assert_eq!(
                events[0].as_log()[&event::log_schema().message_key()],
                message.into()
            );
        });
    }

    #[test]
    fn not_include_image() {
        trace_init();

        let message = "16";
        let name = "vector_test_not_include_image";
        let config_ex = DockerConfig {
            include_images: Some(vec!["some_image".to_owned()]),
            ..DockerConfig::default()
        };

        let mut rt = runtime();
        let exclude_out = source_with_config(config_ex, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id = container_log_n(1, name, None, message, &docker).await;
            container_remove(&id, &docker).await;

            assert!(is_empty(exclude_out).await.unwrap());
        });
    }

    #[test]
    fn not_include_running_image() {
        trace_init();

        let message = "17";
        let name = "vector_test_not_include_running_image";
        let config_ex = DockerConfig {
            include_images: Some(vec!["some_image".to_owned()]),
            ..DockerConfig::default()
        };
        let config_in = DockerConfig {
            include_containers: Some(vec![name.to_owned()]),
            include_images: Some(vec!["busybox".to_owned()]),
            ..DockerConfig::default()
        };

        let mut rt = runtime();
        let docker = docker().unwrap();

        let id = running_container(name, None, message, &docker, &mut rt);
        let exclude_out = source_with_config(config_ex, &mut rt);
        let include_out = source_with_config(config_in, &mut rt);

        rt.block_on_std(async move {
            let _ = collect_n(include_out, 1).compat().await.ok().unwrap();
            let _ = container_kill(&id, &docker).await;
            container_remove(&id, &docker).await;

            assert!(is_empty(exclude_out).await.unwrap());
        });
    }

    #[test]
    fn log_longer_than_16kb() {
        trace_init();

        let mut message = String::with_capacity(20 * 1024);
        for _ in 0..message.capacity() {
            message.push('0');
        }
        let name = "vector_test_log_longer_than_16kb";

        let mut rt = runtime();
        let out = source_with(&[name], None, &mut rt);

        rt.block_on_std(async move {
            let docker = docker().unwrap();

            let id = container_log_n(1, name, None, message.as_str(), &docker).await;
            let events = collect_n(out, 1).compat().await.ok().unwrap();
            container_remove(&id, &docker).await;

            let log = events[0].as_log();
            assert_eq!(log[&event::log_schema().message_key()], message.into());
        });
    }
}
