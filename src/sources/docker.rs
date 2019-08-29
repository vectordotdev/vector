use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bollard::{
    container::{ListContainersOptions, LogOutput, LogsOptions},
    Docker,
};
use chrono::{DateTime, FixedOffset};
use futures::{
    sync::mpsc::{self, Sender, UnboundedReceiver, UnboundedSender},
    Async, Future, Sink, Stream,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};
use tracing::field;

/// The begining of image names of vector docker images packaged by vector.
const VECTOR_IMAGE_NAME: &'static str = "timberio/vector";

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    include_containers: Vec<String>,
    include_labels: Vec<String>,
}

#[typetag::serde(name = "docker")]
impl SourceConfig for DockerConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: Sender<Event>,
    ) -> Result<super::Source, String> {
        DockerSource::new(self.clone(), out).map(|f| Box::new(f) as Box<_>)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
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
    config: DockerConfig,
    docker: Docker,
    esb: EventStreamBuilder,
    /// event stream from docker
    events: Box<
        dyn Stream<
                Item = Result<async_docker::Event, async_docker::Error>,
                Error = async_docker::Error,
            > + Send,
    >,
    ///  mappings of seen container_id to their data
    containers: HashMap<String, ContainerState>,
    /// collection of container_id not to be listened
    ignore_container_id: Vec<String>,
    ///receives ContainerLogInfo comming from event stream futures
    main_recv: UnboundedReceiver<ContainerLogInfo>,
}

impl DockerSource {
    fn new(
        config: DockerConfig,
        out: Sender<Event>,
    ) -> Result<impl Future<Item = (), Error = ()>, String> {
        // ?NOTE: Requiers sudo privileges, or docker group membership.
        // Without extra configuration of docker on user side, there is no way around above.
        let docker = Docker::connect_with_local_defaults().map_err(|error| {
            error!(message="Error connecting to docker server",%error);
            "Failed to connect to docker server".to_owned()
        })?;

        // Channel of communication between main future and event_stream futures
        let (main_send, main_recv) = mpsc::unbounded::<ContainerLogInfo>();

        // main event stream, with whom only newly started/restarted containers will be loged.
        let events = DockerSource::docker_event_stream(&config)?;
        info!(message = "Listening docker events");

        // Starting with logs from now.
        // TODO: Is this exception acceptable?
        // Only somewhat exception to this is case where:
        // t0 -- outside: container running
        // t1 -- now_timestamp
        // t2 -- outside: container stoped
        // t3 -- list_containers
        // In that case, logs between [t1,t2] will be pulled to vector only on next start/unpause of that container.
        let esb = EventStreamBuilder::new(docker.clone(), out, main_send);

        // Construct, capture currently running containers, and do main future(self)
        Ok(DockerSource {
            config,
            docker,
            esb,
            events,
            containers: HashMap::new(),
            ignore_container_id: Vec::new(),
            main_recv,
        }
        .running_containers()
        .and_then(|source| source))
    }

    /// Returns event stream coming from docker.
    fn docker_event_stream(
        config: &DockerConfig,
    ) -> Result<
        Box<
            dyn Stream<
                    Item = Result<async_docker::Event, async_docker::Error>,
                    Error = async_docker::Error,
                > + Send,
        >,
        String,
    > {
        // TODO: async_docker should be replaced with bollard once it supports events
        // ?NOTE: Requiers sudo privileges, or docker group membership.
        // Without extra configuration of docker on user side, there is no way around above.
        let docker_for_events = async_docker::new_docker(None).map_err(|error| {
            error!(message="Error connecting to docker server",%error);
            "Failed to connect to docker server".to_owned()
        })?;

        let mut options = async_docker::EventsOptionsBuilder::new();

        // event  | emmited on commands
        // -------+-------------------
        // start  | docker start, docker run, restart policy, docker restart
        // upause | docker unpause
        // die    | docker restart, docker stop, docker kill, process exited, oom
        // pause  | docker pause
        options.filter(
            vec!["start", "upause", "die", "pause"]
                .into_iter()
                .map(|s| async_docker::EventFilter::Event(s.into()))
                .collect(),
        );

        // by docker API, using both type of include results in AND between them

        // Include-name
        if !config.include_containers.is_empty() {
            options.filter(
                config
                    .include_containers
                    .iter()
                    .map(|s| async_docker::EventFilter::Container(s.to_owned()))
                    .collect(),
            );
        }

        // Include-label
        if !config.include_labels.is_empty() {
            options.filter(
                config
                    .include_labels
                    .iter()
                    .map(|s| async_docker::EventFilter::Label(s.to_owned()))
                    .collect(),
            );
        }

        Ok(docker_for_events.events(&options.build()))
    }
    /// Future that captures currently running containers, and starts event streams for them.
    fn running_containers(mut self) -> impl Future<Item = Self, Error = ()> {
        let mut options = ListContainersOptions::default();

        // by docker API, using both type of include results in AND between them

        // Include-name
        if !self.config.include_containers.is_empty() {
            options
                .filters
                .insert("name".to_owned(), self.config.include_containers.clone());
        }

        // Include-label
        if !self.config.include_labels.is_empty() {
            options
                .filters
                .insert("label".to_owned(), self.config.include_labels.clone());
        }

        // Find out it's own container id, if it's inside a docker container.
        // Since docker doesn't readily provide such information,
        // various approches need to be made. As such the solution is not
        // exact, but probable.
        // This is to be used only if source is in state of catching everything.
        // Or in other words, if includes are used then this is not necessary.
        let exclude_self =
            self.config.include_containers.is_empty() && self.config.include_labels.is_empty();

        // HOSTNAME hint
        // It may contain shortened container id.
        let hostname = env::var("HOSTNAME").ok();

        // IMAGE hint
        // If image name starts with this.
        let image = VECTOR_IMAGE_NAME;

        // Future
        self.docker
            .list_containers(Some(options))
            .map(move |list| {
                for container in list {
                    trace!(
                        message = "Found already running container",
                        id = field::display(&container.id)
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
                            self.ignore_container_id.push(container.id);
                            continue;
                        }
                    }

                    self.containers
                        .insert(container.id.clone(), self.esb.start(container.id));
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
                        Ok(Async::Ready(Some(Ok(mut event)))) => {
                            match (event.id.take(), event.status.take()) {
                                (Some(id), Some(status)) => {
                                    trace!(
                                        message = "docker event",
                                        id = field::display(&id),
                                        status = field::display(&status),
                                        timestamp = field::display(event.time),
                                    );
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
                                            } else if self
                                                .ignore_container_id
                                                .binary_search(&id)
                                                .is_err()
                                            {
                                                self.containers
                                                    .insert(id.clone(), self.esb.start(id));
                                            } else {
                                                // Ignore
                                            }
                                        }
                                        // Ignore
                                        _ => (),
                                    }
                                }
                                // Ignore
                                _ => (),
                            }
                        }
                        Ok(Async::Ready(Some(Err(error)))) => {
                            error!(message = "Error in docker event stream",%error)
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
    docker: Docker,
    /// Only logs created at, or after this moment are logged. UNIX timestamp
    now_timestamp: i64,
    /// Event stream futures send events through this
    out: Sender<Event>,
    /// End through which event stream futures send ContainerLogInfo to main future
    main_send: UnboundedSender<ContainerLogInfo>,
}

impl EventStreamBuilder {
    /// Only logs created at, or after this moment are logged.
    fn new(
        docker: Docker,
        out: Sender<Event>,
        main_send: UnboundedSender<ContainerLogInfo>,
    ) -> Self {
        let now = chrono::Local::now();
        let now_timestamp = now.timestamp();
        info!(
            message = "Capturing logs from now on",
            now = field::display(now.to_rfc3339())
        );
        EventStreamBuilder {
            docker,
            now_timestamp,
            out,
            main_send,
        }
    }

    /// Constructs and runs event stream
    fn start(&self, id: String) -> ContainerState {
        let mut state = ContainerState::new(id, self.now_timestamp);
        self.restart(&mut state);
        state
    }

    /// If info is present, restarts event stream
    fn restart(&self, container: &mut ContainerState) {
        container
            .take_info()
            .map(|info| self.start_event_stream(info));
    }

    fn start_event_stream(&self, info: ContainerLogInfo) {
        // Establish connection
        let options = LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            since: info.log_since(),
            timestamps: true,
            ..Default::default()
        };
        let mut stream = self.docker.logs(&info.id, Some(options));
        info!(
            message = "Started listening logs on docker container",
            id = field::display(&info.id)
        );

        // Create event streamer
        let mut state = Some((self.main_send.clone(), info));
        let event_stream = tokio::prelude::stream::poll_fn(move || {
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
                        Ok(Async::Ready(None)) => {
                            let (main, info) = state.take().expect("They are present here");
                            // End of stream
                            info!(
                                message = "Stoped listening logs on docker container",
                                id = field::display(&info.id)
                            );
                            // TODO: I am not sure that it's necessary to drive this future to completition
                            tokio::spawn(
                                main.send(info)
                                    .map_err(|e| error!(message="Unable to return ContainerLogInfo to main",%e))
                                    .map(|_| ()),
                            );
                            Ok(Async::Ready(None))
                        }
                        Ok(Async::NotReady) => Ok(Async::NotReady),
                        Err(error) => {
                            error!(message = "docker API container logging error",%error);
                            Err(())
                        }
                    };
                }
            }

            Ok(Async::Ready(None))
        })
        .forward(self.out.clone().sink_map_err(|_| ()))
        .map(|_| ());

        // Run event_stream
        tokio::spawn(event_stream);
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
    /// Container docker ID
    /// Unix timestamp of event which created this struct
    fn new(id: String, created: i64) -> Self {
        let info = ContainerLogInfo {
            id: id,
            created,
            last_log: None,
            generation: 0,
        };
        ContainerState {
            info: Some(info),
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
    id: String,
    /// Unix timestamp of event which created this struct
    created: i64,
    /// Timestamp of last log message with it's generation
    last_log: Option<(DateTime<FixedOffset>, u64)>,
    /// generation of ContainerState at event_stream creation
    generation: u64,
}

impl ContainerLogInfo {
    /// Only logs after or equal to this point need to be fetched
    fn log_since(&self) -> i64 {
        self.last_log
            .as_ref()
            .map(|&(ref d, _)| d.timestamp())
            .unwrap_or(self.created)
            - 1
    }

    /// Expects timestamp at the begining of message
    /// Expects messages to be ordered by timestamps
    fn new_event(&mut self, message: LogOutput) -> Option<Event> {
        let mut log_event = Event::new_empty_log().into_log();

        let (message, stream) = match message {
            LogOutput::StdErr { message } => (message, "stderr"),
            LogOutput::StdOut { message } => (message, "stdout"),
            _ => return None,
        };
        log_event.insert_implicit(event::STREAM.clone(), stream.into());

        let mut splitter = message.splitn(2, char::is_whitespace);
        let timestamp_str = splitter.next()?;
        let log = match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(timestamp) => {
                // Timestamp check
                match self.last_log.as_ref() {
                    // Recieved log has not already been processed
                    Some(&(ref last, gen))
                        if *last < timestamp || (*last == timestamp && gen == self.generation) =>
                    {
                        ()
                    }
                    // Recieved log is not from before of creation
                    None if self.created <= timestamp.timestamp() => (),
                    _ => {
                        trace!(
                            message = "Recieved older log",
                            timestamp = field::display(timestamp_str)
                        );
                        return None;
                    }
                }
                // Supply timestamp
                log_event.insert_explicit(
                    event::TIMESTAMP.clone(),
                    timestamp.with_timezone(&chrono::Utc).into(),
                );

                self.last_log = Some((timestamp, self.generation));
                splitter.next()?
            }
            Err(error) => {
                // Recieved bad timestamp, if any at all.
                error!(message="Didn't recieve rfc3339 timestamp from docker",%error);
                // So log whole message
                message.as_str()
            }
        };

        // Supply message
        log_event.insert_explicit(event::MESSAGE.clone(), log.into());

        // Supply container
        log_event.insert_implicit(event::CONTAINER.clone(), self.id.as_str().into());

        let event = Event::Log(log_event);
        trace!(message = "Received one event", event = field::debug(&event));
        Some(event)
    }
}

#[cfg(all(test, feature = "docker-integration-tests"))]
mod tests {
    use super::*;
    use crate::test_util::{collect_n, trace_init};
    use bollard::container;

    /// None if docker is not present on the system
    fn source<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
    ) -> (mpsc::Receiver<Event>, tokio::runtime::Runtime) {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let source = source_with(name, label, &mut rt);
        (source, rt)
    }

    /// None if docker is not present on the system
    fn source_with<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        rt: &mut tokio::runtime::Runtime,
    ) -> mpsc::Receiver<Event> {
        trace_init();
        let (sender, recv) = mpsc::channel(100);
        rt.spawn(
            DockerConfig {
                include_containers: vec![name.to_owned()],
                include_labels: label.into().map(|l| vec![l.to_owned()]).unwrap_or_default(),
                ..DockerConfig::default()
            }
            .build("default", &GlobalOptions::default(), sender)
            .unwrap(),
        );
        recv
    }

    fn docker() -> Docker {
        Docker::connect_with_local_defaults().expect("docker present on system")
    }

    /// Users should ensure to remove container before exiting.
    fn log_container<'a, L: Into<Option<&'a str>>>(
        name: &str,
        label: L,
        log: &str,
        docker: &Docker,
        rt: &mut tokio::runtime::Runtime,
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
        rt: &mut tokio::runtime::Runtime,
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
        rt: &mut tokio::runtime::Runtime,
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
        rt: &mut tokio::runtime::Runtime,
    ) -> Option<String> {
        let future = docker.create_container(
            Some(container::CreateContainerOptions {
                name: name.to_owned(),
            }),
            container::Config {
                image: Some("busybox".to_owned()),
                cmd: Some(cmd),
                labels: label.into().map(|l| {
                    let mut map = HashMap::new();
                    map.insert(l.to_owned(), String::new());
                    map
                }),
                ..container::Config::default()
            },
        );
        rt.block_on(future)
            .map_err(|e| error!(%e))
            .ok()
            .map(|c| c.id)
    }

    /// Returns once container has started
    #[must_use]
    fn container_start(id: &str, docker: &Docker, rt: &mut tokio::runtime::Runtime) -> Option<()> {
        let future = docker.start_container(id, None::<container::StartContainerOptions<String>>);
        rt.block_on(future).ok()
    }

    /// Returns once container is done running
    #[must_use]
    fn container_wait(id: &str, docker: &Docker, rt: &mut tokio::runtime::Runtime) -> Option<()> {
        let future = docker.wait_container(id, None::<container::WaitContainerOptions<String>>);
        rt.block_on(future.into_future())
            .map_err(|(e, _)| error!(%e))
            .map(|_| ())
            .ok()
    }

    /// Returns once container is done running
    #[must_use]
    fn container_run(id: &str, docker: &Docker, rt: &mut tokio::runtime::Runtime) -> Option<()> {
        container_start(id, docker, rt)?;
        container_wait(id, docker, rt)
    }

    fn container_remove(id: &str, docker: &Docker, rt: &mut tokio::runtime::Runtime) {
        let future = docker.remove_container(id, None::<container::RemoveContainerOptions>);
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
        rt: &mut tokio::runtime::Runtime,
    ) {
        let id = log_container(name, label, log, docker, rt);
        for _ in 0..n {
            if container_run(&id, docker, rt).is_none() {
                container_remove(&id, docker, rt);
                panic!("Container run failed");
            }
        }
        container_remove(&id, docker, rt);
    }

    #[test]
    fn newly_started() {
        let message = "9";
        let name = "vector_test_newly_started";

        let (out, mut rt) = source(name, None);
        let docker = docker();

        container_log_n(1, name, None, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }

    #[test]
    fn restart() {
        let message = "10";
        let name = "vector_test_restart";

        let (out, mut rt) = source(name, None);
        let docker = docker();

        container_log_n(2, name, None, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 2)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into());
        assert_eq!(events[1].as_log()[&event::MESSAGE], message.into());
    }

    #[test]
    fn include_containers() {
        let message = "11";
        let name = "vector_test_include_container_1";

        let (out, mut rt) = source(name, None);
        let docker = docker();

        container_log_n(
            1,
            "vector_test_include_container_2",
            None,
            "13",
            &docker,
            &mut rt,
        );
        container_log_n(1, name, None, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }

    #[test]
    fn include_labels() {
        let message = "12";
        let name = "vector_test_include_labels";
        let label = "vector_test_include_label";

        let (out, mut rt) = source(name, label);
        let docker = docker();

        container_log_n(1, name, None, "13", &docker, &mut rt);
        container_log_n(1, name, label, message, &docker, &mut rt);

        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }

    #[test]
    fn currently_running() {
        let message = "14";
        let name = "vector_test_currently_running";
        let delay = 3; // sec

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let docker = docker();

        let id = delayed_container(name, None, message, delay, &docker, &mut rt);
        if container_start(&id, &docker, &mut rt).is_none() {
            container_remove(&id, &docker, &mut rt);
            panic!("Container start failed");
        }

        std::thread::sleep(std::time::Duration::from_secs(1));
        let out = source_with(name, None, &mut rt);
        let events = rt.block_on(collect_n(out, 1)).ok().unwrap();
        let _ = container_wait(&id, &docker, &mut rt);
        container_remove(&id, &docker, &mut rt);

        assert_eq!(events[0].as_log()[&event::MESSAGE], message.into())
    }
}
