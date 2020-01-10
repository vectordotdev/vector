use crate::{
    event::{self, Event, ValueKind},
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
};
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, FixedOffset, Utc};
use futures::{
    sync::mpsc::{self, Sender, UnboundedReceiver, UnboundedSender},
    Async, Future, Sink, Stream,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use shiplift::{
    builder::{ContainerFilter, EventFilter, LogsOptions},
    rep::ContainerDetails,
    tty::{Chunk, StreamType},
    Docker,
};
use std::borrow::Borrow;
use std::sync::Arc;
use std::{collections::HashMap, env};
use string_cache::DefaultAtom as Atom;
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

type DockerEvent = shiplift::rep::Event;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    include_containers: Option<Vec<String>>,
    include_labels: Option<Vec<String>>,
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
        out: Sender<Event>,
    ) -> crate::Result<super::Source> {
        Ok(Box::new(DockerSource::new(self.clone(), out)) as Box<_>)
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
    /// Only logs created at, or after this moment are logged. UNIX timestamp
    now_timestamp: i64,
}

impl DockerSourceCore {
    /// Only logs created at, or after this moment are logged.
    fn new(config: DockerConfig) -> Self {
        // ?NOTE: Constructs a new Docker instance for a docker host listening at url specified by an env var DOCKER_HOST.
        // ?      Otherwise connects to unix socket which requires sudo privileges, or docker group membership.
        let docker = Docker::new();

        // Only logs created at, or after this moment are logged.
        let now = chrono::Local::now();
        let now_timestamp = now.timestamp();
        info!(
            message = "Capturing logs from now on",
            now = %now.to_rfc3339()
        );
        DockerSourceCore {
            config,
            docker,
            now_timestamp,
        }
    }

    /// Returns event stream coming from docker.
    fn docker_event_stream(
        &self,
    ) -> impl Stream<Item = DockerEvent, Error = shiplift::Error> + Send {
        let mut options = shiplift::builder::EventsOptions::builder();

        // Limit events to those after core creation
        options.since(&(self.now_timestamp as u64));

        // event  | emmited on commands
        // -------+-------------------
        // start  | docker start, docker run, restart policy, docker restart
        // upause | docker unpause
        // die    | docker restart, docker stop, docker kill, process exited, oom
        // pause  | docker pause
        options.filter(
            vec!["start", "upause", "die", "pause"]
                .into_iter()
                .map(|s| EventFilter::Event(s.into()))
                .collect(),
        );

        // Apply include filters
        let mut filters = Vec::new();

        if let Some(include_containers) = &self.config.include_containers {
            filters.extend(
                include_containers
                    .iter()
                    .map(|s| EventFilter::Container(s.clone())),
            );
        }

        if let Some(include_labels) = &self.config.include_labels {
            filters.extend(include_labels.iter().map(|l| EventFilter::Label(l.clone())));
        }

        options.filter(filters);

        self.docker.events(&options.build())
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
    events: Box<dyn Stream<Item = DockerEvent, Error = shiplift::Error> + Send>,
    ///  mappings of seen container_id to their data
    containers: HashMap<ContainerId, ContainerState>,
    /// collection of container_id not to be listened
    ignore_container_id: Vec<ContainerId>,
    ///receives ContainerLogInfo comming from event stream futures
    main_recv: UnboundedReceiver<ContainerLogInfo>,
}

impl DockerSource {
    fn new(config: DockerConfig, out: Sender<Event>) -> impl Future<Item = (), Error = ()> {
        // Only logs created at, or after this moment are logged.
        let core = DockerSourceCore::new(config);

        // main event stream, with whom only newly started/restarted containers will be loged.
        let events = core.docker_event_stream();
        info!(message = "Listening docker events");

        // Channel of communication between main future and event_stream futures
        let (main_send, main_recv) = mpsc::unbounded::<ContainerLogInfo>();

        // Starting with logs from now.
        // TODO: Is this exception acceptable?
        // Only somewhat exception to this is case where:
        // t0 -- outside: container running
        // t1 -- now_timestamp
        // t2 -- outside: container stoped
        // t3 -- list_containers
        // In that case, logs between [t1,t2] will be pulled to vector only on next start/unpause of that container.
        let esb = EventStreamBuilder::new(core, out, main_send);

        // Construct, capture currently running containers, and do main future(self)
        DockerSource {
            esb,
            events: Box::new(events) as Box<_>,
            containers: HashMap::new(),
            ignore_container_id: Vec::new(),
            main_recv,
        }
        .running_containers()
        .and_then(|source| source)
    }

    /// Future that captures currently running containers, and starts event streams for them.
    fn running_containers(mut self) -> impl Future<Item = Self, Error = ()> {
        let mut options = shiplift::ContainerListOptions::builder();

        // by docker API, using both type of include results in AND between them
        // TODO: missing feature in shiplift to include ContainerFilter::Name
        // Include-label
        options.filter(
            self.esb
                .core
                .config
                .include_labels
                .clone()
                .unwrap_or_default()
                .iter()
                .map(|s| ContainerFilter::LabelName(s.clone()))
                .collect(),
        );

        // Find out it's own container id, if it's inside a docker container.
        // Since docker doesn't readily provide such information,
        // various approches need to be made. As such the solution is not
        // exact, but probable.
        // This is to be used only if source is in state of catching everything.
        // Or in other words, if includes are used then this is not necessary.
        let exclude_self = self
            .esb
            .core
            .config
            .include_containers
            .clone()
            .unwrap_or_default()
            .is_empty()
            && self
                .esb
                .core
                .config
                .include_labels
                .clone()
                .unwrap_or_default()
                .is_empty();

        // HOSTNAME hint
        // It may contain shortened container id.
        let hostname = env::var("HOSTNAME").ok();

        // IMAGE hint
        // If image name starts with this.
        let image = VECTOR_IMAGE_NAME;

        // Future
        self.esb
            .core
            .docker
            .containers()
            .list(&options.build())
            .map(move |list| {
                for container in list {
                    trace!(
                        message = "Found already running container",
                        id = field::display(&container.id),
                        names = field::debug(&container.names)
                    );

                    if exclude_self {
                        let hostname_hint = hostname
                            .as_ref()
                            .map(|maybe_short_id| container.id.starts_with(maybe_short_id))
                            .unwrap_or(false);
                        let image_hint = container.image.starts_with(image);
                        if hostname_hint || image_hint {
                            // This container is probably itself.
                            // So ignore it.
                            info!(
                                message = "Detected self container",
                                id = field::display(&container.id)
                            );
                            self.ignore_container_id
                                .push(ContainerId::new(container.id));
                            continue;
                        }
                    }

                    // This check is necessary since shiplift doesn't have way to include
                    // names into request to docker.
                    if !self.esb.core.config.container_name_included(
                        container.id.as_str(),
                        container.names.iter().map(|s| {
                            // In this case shiplift gives names with starting '/' so it needs to be removed.
                            let s = s.as_str();
                            if s.starts_with('/') {
                                s.split_at('/'.len_utf8()).1
                            } else {
                                s
                            }
                        }),
                    ) {
                        trace!(
                            message = "Container excluded",
                            id = field::display(&container.id)
                        );
                        continue;
                    }

                    let id = ContainerId::new(container.id);

                    self.containers.insert(id.clone(), self.esb.start(id));
                }

                // Sort for efficient lookup
                self.ignore_container_id.sort();
                self
            })
            .map_err(|error| error!(message="Listing currently running containers, failed",%error))
    }
}

impl Future for DockerSource {
    type Item = ();
    type Error = ();

    /// Main future which listens for events from docker and messages from event streams futures.
    /// Depending on recieved events and messages, may start/restart an event stream future.
    fn poll(&mut self) -> Result<Async<()>, ()> {
        loop {
            match self.main_recv.poll() {
                // Process message from event_stream
                Ok(Async::Ready(Some(info))) => {
                    let state = self
                        .containers
                        .get_mut(&info.id)
                        .expect("Every ContainerLogInfo has it's ContainerState");
                    if state.return_info(info) {
                        self.esb.restart(state);
                    }
                }
                // Check events from docker
                Ok(Async::NotReady) => {
                    match self.events.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        // Process event from docker
                        Ok(Async::Ready(Some(mut event))) => {
                            if let (Some(id), Some(status)) = (event.id.take(), event.status.take())
                            {
                                trace!(
                                    message = "docker event",
                                    id = field::display(&id),
                                    status = field::display(&status),
                                    timestamp = field::display(event.time),
                                    attributes = field::debug(&event.actor.attributes),
                                );
                                let id = ContainerId::new(id);

                                // Update container status
                                match status.as_str() {
                                    "die" | "pause" => {
                                        if let Some(state) = self.containers.get_mut(&id) {
                                            state.stoped();
                                        }
                                    }
                                    "start" | "upause" => {
                                        if let Some(state) = self.containers.get_mut(&id) {
                                            state.running();
                                            self.esb.restart(state);
                                        } else {
                                            let ignore_flag = self
                                                .ignore_container_id
                                                .binary_search_by(|a| a.as_str().cmp(id.as_str()))
                                                .is_ok();

                                            // This check is necessary since shiplift doesn't have way to include
                                            // names into request to docker.
                                            let include_name =
                                                self.esb.core.config.container_name_included(
                                                    id.as_str(),
                                                    event
                                                        .actor
                                                        .attributes
                                                        .get("name")
                                                        .map(|s| s.as_str()),
                                                );

                                            if !ignore_flag && include_name {
                                                // Included
                                                self.containers
                                                    .insert(id.clone(), self.esb.start(id));
                                            } else {
                                                // Ignore
                                            }
                                        }
                                    }
                                    // Ignore
                                    _ => (),
                                }
                            }
                        }
                        Err(error) => error!(source="docker events",%error),
                        // Stream has ended
                        Ok(Async::Ready(None)) => {
                            // TODO: this could be fixed, but should be tryed with some timeoff and exponential backoff
                            error!(message = "docker event stream has ended unexpectedly");
                            info!(message = "Shuting down docker source");
                            return Err(());
                        }
                    }
                }
                Err(()) => error!(message = "Error in docker source main stream"),
                // For some strange reason stream has ended.
                // It should never reach this point. But if it does,
                // something has gone terrible wrong, and this system is probably
                // in invalid state.
                Ok(Async::Ready(None)) => {
                    error!(message = "docker source main stream has ended unexpectedly");
                    info!(message = "Shuting down docker source");
                    return Err(());
                }
            }
        }
    }
}

/// Used to construct and start event stream futures
#[derive(Clone)]
struct EventStreamBuilder {
    core: Arc<DockerSourceCore>,
    /// Event stream futures send events through this
    out: Sender<Event>,
    /// End through which event stream futures send ContainerLogInfo to main future
    main_send: UnboundedSender<ContainerLogInfo>,
}

impl EventStreamBuilder {
    fn new(
        core: DockerSourceCore,
        out: Sender<Event>,
        main_send: UnboundedSender<ContainerLogInfo>,
    ) -> Self {
        EventStreamBuilder {
            core: Arc::new(core),
            out,
            main_send,
        }
    }

    /// Constructs and runs event stream
    fn start(&self, id: ContainerId) -> ContainerState {
        let metadata_fetch = self
            .core
            .docker
            .containers()
            .get(id.as_str())
            .inspect()
            .map_err(|error| error!(message="Fetching container details failed",%error))
            .and_then(|details| {
                ContainerMetadata::from_details(&details)
                    .map_err(|error| error!(message="Metadata extraction failed",%error))
            });

        let this = self.clone();
        let task = metadata_fetch.and_then(move |metadata| {
            this.start_event_stream(ContainerLogInfo::new(id, metadata, this.core.now_timestamp))
        });

        tokio::spawn(task);

        ContainerState::new()
    }

    /// If info is present, restarts event stream
    fn restart(&self, container: &mut ContainerState) {
        if let Some(info) = container.take_info() {
            tokio::spawn(self.start_event_stream(info));
        }
    }

    fn start_event_stream(&self, info: ContainerLogInfo) -> impl Future<Item = (), Error = ()> {
        // Establish connection
        let mut options = LogsOptions::builder();
        options
            .follow(true)
            .stdout(true)
            .stderr(true)
            // TODO: missing feature in shiplift to use .since(info.log_since())
            .timestamps(true);

        let mut stream = self
            .core
            .docker
            .containers()
            .get(info.id.as_str())
            .logs(&options.build());
        info!(
            message = "Started listening logs on docker container",
            id = field::display(info.id.as_str())
        );

        // Create event streamer
        let mut state = Some((self.main_send.clone(), info));
        tokio::prelude::stream::poll_fn(move || {
            // !Hot code: from here
            if let Some(&mut (_, ref mut info)) = state.as_mut() {
                // Main event loop
                loop {
                    return match stream.poll() {
                        Ok(Async::Ready(Some(message))) => {
                            if let Some(event) = info.new_event(message) {
                                Ok(Async::Ready(Some(event)))
                            } else {
                                continue;
                            }
                            // !Hot code: to here
                        }
                        Ok(Async::Ready(None)) => break,
                        Ok(Async::NotReady) => Ok(Async::NotReady),
                        Err(error) => {
                            error!(message = "docker API container logging error",%error);
                            // On any error, restart connection
                            break;
                        }
                    };
                }

                let (main, info) = state.take().expect("They are present here");
                // End of stream
                info!(
                    message = "Stoped listening logs on docker container",
                    id = field::display(info.id.as_str())
                );
                // TODO: I am not sure that it's necessary to drive this future to completition
                tokio::spawn(
                    main.send(info)
                        .map_err(|e| error!(message="Unable to return ContainerLogInfo to main",%e))
                        .map(|_| ()),
                );
            }

            Ok(Async::Ready(None))
        })
        .forward(self.out.clone().sink_map_err(|_| ()))
        .map(|_| ())
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
        std::str::from_utf8(self.0.borrow()).expect("Bytes should be a still valid String")
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
    /// Unix timestamp of event which created this struct
    created: i64,
    /// Timestamp of last log message with it's generation
    last_log: Option<(DateTime<FixedOffset>, u64)>,
    /// generation of ContainerState at event_stream creation
    generation: u64,
    metadata: ContainerMetadata,
}

impl ContainerLogInfo {
    /// Container docker ID
    /// Unix timestamp of event which created this struct
    fn new(id: ContainerId, metadata: ContainerMetadata, created: i64) -> Self {
        ContainerLogInfo {
            id,
            created,
            last_log: None,
            generation: 0,
            metadata,
        }
    }

    /// Only logs after or equal to this point need to be fetched
    #[allow(dead_code)]
    fn log_since(&self) -> i64 {
        self.last_log
            .as_ref()
            .map(|&(ref d, _)| d.timestamp())
            .unwrap_or(self.created)
            - 1
    }

    /// Expects timestamp at the begining of message
    /// Expects messages to be ordered by timestamps
    fn new_event(&mut self, message: Chunk) -> Option<Event> {
        let mut log_event = Event::new_empty_log().into_log();

        let stream = match message.stream_type {
            StreamType::StdErr => STDERR.clone(),
            StreamType::StdOut => STDOUT.clone(),
            _ => return None,
        };
        log_event.insert_implicit(STREAM.clone(), stream);

        let mut bytes_message = BytesMut::from(message.data);

        let message = String::from_utf8_lossy(bytes_message.borrow());

        let mut splitter = message.splitn(2, char::is_whitespace);
        let timestamp_str = splitter.next()?;
        match DateTime::parse_from_rfc3339(timestamp_str) {
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
                    None if self.created <= timestamp.timestamp() => (),
                    _ => {
                        trace!(
                            message = "Recieved older log",
                            timestamp = %timestamp_str
                        );
                        return None;
                    }
                }
                // Supply timestamp
                log_event.insert_explicit(event::TIMESTAMP.clone(), timestamp.with_timezone(&Utc));

                self.last_log = Some((timestamp, self.generation));

                let log = splitter.next()?;
                let remove_len = message.len() - log.len();
                bytes_message.advance(remove_len);
            }
            Err(error) => {
                // Recieved bad timestamp, if any at all.
                error!(message="Didn't recieve rfc3339 timestamp from docker",%error);
                // So log whole message
            }
        }

        // Message is actually one line from stderr or stdout, and they are delimited with newline
        // so that newline needs to be removed.
        if bytes_message
            .last()
            .map(|&b| b as char == '\n')
            .unwrap_or(false)
        {
            bytes_message.truncate(bytes_message.len() - 1);
        }

        // Supply message
        log_event.insert_explicit(event::MESSAGE.clone(), bytes_message.freeze());

        // Supply container
        log_event.insert_implicit(CONTAINER.clone(), self.id.0.clone());

        // Add Metadata
        for (key, value) in self.metadata.labels.iter() {
            log_event.insert_implicit(key.clone(), value.clone());
        }
        log_event.insert_implicit(NAME.clone(), self.metadata.name.clone());
        log_event.insert_implicit(IMAGE.clone(), self.metadata.image.clone());
        log_event.insert_implicit(CREATED_AT.clone(), self.metadata.created_at.clone());

        let event = Event::Log(log_event);
        trace!(message = "Received one event.", ?event);
        Some(event)
    }
}

struct ContainerMetadata {
    /// label.key -> String
    labels: Vec<(Atom, ValueKind)>,
    /// name -> String
    name: ValueKind,
    /// image -> String
    image: ValueKind,
    /// created_at -> DateTime<Utc>
    created_at: ValueKind,
}

impl ContainerMetadata {
    fn from_details(details: &ContainerDetails) -> Result<Self, chrono::format::ParseError> {
        let labels = details
            .config
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
            name: remove_slash(details.name.as_str()).into(),
            image: details.config.image.as_str().into(),
            created_at: DateTime::parse_from_rfc3339(details.created.as_str())?
                .with_timezone(&Utc)
                .into(),
        })
    }
}

/// Removes / at the start of str
fn remove_slash(s: &str) -> &str {
    s.trim_start_matches("/")
}

#[cfg(all(test, feature = "docker-integration-tests"))]
mod tests {
    use super::*;
    use crate::runtime;
    use crate::test_util::{self, collect_n, trace_init};

    static BUXYBOX_IMAGE_TAG: &'static str = "latest";

    fn pull(image: &str, docker: &Docker, rt: &mut runtime::Runtime) {
        let list_option = shiplift::ImageListOptions::builder()
            .filter_name(image)
            .build();

        if let Ok(images) = rt
            .block_on(docker.images().list(&list_option))
            .map_err(|e| error!(%e))
        {
            if images.is_empty() {
                trace!("Pulling image");
                let options = shiplift::PullOptions::builder()
                    .image(image)
                    .tag(BUXYBOX_IMAGE_TAG)
                    .build();
                let _ = rt
                    .block_on(docker.images().pull(&options).collect())
                    .map_err(|e| error!(%e));
            }
        }
        // Try running the tests in any case.
    }

    /// None if docker is not present on the system
    fn source<'a, L: Into<Option<&'a str>>>(
        names: &[&str],
        label: L,
    ) -> (mpsc::Receiver<Event>, runtime::Runtime) {
        let mut rt = test_util::runtime();
        let source = source_with(names, label, &mut rt);
        (source, rt)
    }

    /// None if docker is not present on the system
    fn source_with<'a, L: Into<Option<&'a str>>>(
        names: &[&str],
        label: L,
        rt: &mut runtime::Runtime,
    ) -> mpsc::Receiver<Event> {
        trace_init();
        let (sender, recv) = mpsc::channel(100);
        rt.spawn(
            DockerConfig {
                include_containers: Some(names.iter().map(|&s| s.to_owned()).collect()),
                include_labels: Some(label.into().map(|l| vec![l.to_owned()]).unwrap_or_default()),
                ..DockerConfig::default()
            }
            .build("default", &GlobalOptions::default(), sender)
            .unwrap(),
        );
        recv
    }

    fn docker() -> Docker {
        Docker::new()
    }

    /// Users should ensure to remove container before exiting.
    fn log_container<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        log: &str,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> String {
        cmd_container(
            name,
            label,
            vec!["echo".to_owned(), log.to_owned()],
            docker,
            rt,
        )
    }

    /// Users should ensure to remove container before exiting.
    /// Delay in seconds
    fn delayed_container<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        log: &str,
        delay: u32,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> String {
        cmd_container(
            name,
            label,
            vec![
                "sh".to_owned(),
                "-c".to_owned(),
                format!("echo before; sleep {}; echo {}", delay, log),
            ],
            docker,
            rt,
        )
    }

    /// Users should ensure to remove container before exiting.
    fn cmd_container<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        cmd: Vec<String>,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> String {
        if let Some(id) = cmd_container_for_real(name, label, cmd, docker, rt) {
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
    fn cmd_container_for_real<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        cmd: Vec<String>,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> Option<String> {
        pull("busybox", docker, rt);
        trace!("Creating container");
        let mut options = shiplift::builder::ContainerOptions::builder("busybox");
        options
            .name(name)
            .cmd(cmd.iter().map(|s| s.as_str()).collect());
        label.into().map(|l| {
            let mut map = HashMap::new();
            map.insert(l, "");
            options.labels(&map);
        });

        let future = docker.containers().create(&options.build());
        rt.block_on(future)
            .map_err(|e| error!(%e))
            .ok()
            .map(|c| c.id)
    }

    /// Returns once container has started
    #[must_use]
    fn container_start(
        id: &str,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> Result<(), shiplift::errors::Error> {
        trace!("Starting container");
        let future = docker.containers().get(id).start();
        rt.block_on(future)
    }

    /// Returns once container is done running
    #[must_use]
    fn container_wait(
        id: &str,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> Result<(), shiplift::errors::Error> {
        trace!("Waiting container");
        let future = docker.containers().get(id).wait();
        rt.block_on(future)
            .map(|exit| info!("Container exited with status code: {}", exit.status_code))
    }

    /// Returns once container is done running
    #[must_use]
    fn container_run(
        id: &str,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> Result<(), shiplift::errors::Error> {
        container_start(id, docker, rt)?;
        container_wait(id, docker, rt)
    }

    fn container_remove(id: &str, docker: &Docker, rt: &mut runtime::Runtime) {
        trace!("Removing container");
        let future = docker
            .containers()
            .get(id)
            .remove(shiplift::builder::RmContainerOptions::builder().build());
        // Don't panick, as this is unreleated to test, and there possibly other containers that need to be removed
        let _ = rt.block_on(future).map_err(|e| error!(%e));
    }

    /// Returns once it's certain that log has been made
    /// Expects that this is the only one with a container
    fn container_log_n<'a, L: Into<Option<&'a str>>>(
        n: usize,
        name: &str,
        label: L,
        log: &str,
        docker: &Docker,
        rt: &mut runtime::Runtime,
    ) -> String {
        let id = log_container(name, label, log, docker, rt);
        for _ in 0..n {
            if let Err(error) = container_run(&id, docker, rt) {
                container_remove(&id, docker, rt);
                panic!("Container failed to start with error: {:?}", error);
            }
        }
        id
    }

    #[test]
    fn newly_started() {
        let message = "9";
        let name = "vector_test_newly_started";
        let label = "vector_test_label_newly_started";

        let (out, mut rt) = source(&[name], None);
        let docker = docker();

        let id = container_log_n(1, name, label, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        container_remove(&id, &docker, &mut rt);

        let log = events[0].as_log();
        assert_eq!(log[&event::MESSAGE], message.into());
        assert_eq!(log[&super::CONTAINER], id.into());
        assert!(log.get(&super::CREATED_AT).is_some());
        assert_eq!(log[&super::IMAGE], "busybox".into());
        assert!(log.get(&format!("label.{}", label).into()).is_some());
        assert_eq!(events[0].as_log()[&super::NAME], name.into());
    }

    #[test]
    fn restart() {
        let message = "10";
        let name = "vector_test_restart";

        let (out, mut rt) = source(&[name], None);
        let docker = docker();

        let id = container_log_n(2, name, None, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 2)).ok().unwrap();

        container_remove(&id, &docker, &mut rt);

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], message.into());
    }

    #[test]
    fn include_containers() {
        let message = "11";
        let name0 = "vector_test_include_container_0";
        let name1 = "vector_test_include_container_1";

        let (out, mut rt) = source(&[name1], None);
        let docker = docker();

        let id0 = container_log_n(1, name0, None, "13", &docker, &mut rt);
        let id1 = container_log_n(1, name1, None, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        container_remove(&id0, &docker, &mut rt);
        container_remove(&id1, &docker, &mut rt);

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }

    #[test]
    fn include_labels() {
        let message = "12";
        let name0 = "vector_test_include_labels_0";
        let name1 = "vector_test_include_labels_1";
        let label = "vector_test_include_label";

        let (out, mut rt) = source(&[name0, name1], label);
        let docker = docker();

        let id0 = container_log_n(1, name0, None, "13", &docker, &mut rt);
        let id1 = container_log_n(1, name1, label, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        container_remove(&id0, &docker, &mut rt);
        container_remove(&id1, &docker, &mut rt);

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }

    #[test]
    fn currently_running() {
        let message = "14";
        let name = "vector_test_currently_running";
        let label = "vector_test_label_currently_running";
        let delay = 3; // sec

        let mut rt = test_util::runtime();
        let docker = docker();

        let id = delayed_container(name, label, message, delay, &docker, &mut rt);
        if let Err(error) = container_start(&id, &docker, &mut rt) {
            container_remove(&id, &docker, &mut rt);
            panic!("Container start failed with error: {:?}", error);
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        let out = source_with(&[name], None, &mut rt);
        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();
        let _ = container_wait(&id, &docker, &mut rt);
        container_remove(&id, &docker, &mut rt);

        let log = events[0].as_log();
        assert_eq!(log[&event::MESSAGE], message.into());
        assert_eq!(log[&super::CONTAINER], id.into());
        assert!(log.get(&super::CREATED_AT).is_some());
        assert_eq!(log[&super::IMAGE], "busybox".into());
        assert!(log.get(&format!("label.{}", label).into()).is_some());
        assert_eq!(events[0].as_log()[&super::NAME], name.into());
    }
}
