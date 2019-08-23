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
    future::poll_fn,
    sync::mpsc::{self, Sender},
    Async, Future, Sink, Stream,
};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashMap};
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    include_containers: Vec<String>,
    include_labels: Vec<String>,
    // TODO: add them, or not?
    // ignore_containers: Vec<String>,
    // ignore_labels: Vec<String>,
}

impl Default for DockerConfig {
    fn default() -> Self {
        DockerConfig {
            include_containers: Vec::default(),
            include_labels: Vec::default(),
            // ignore_containers: Vec::default(),
            // ignore_labels: Vec::default(),
        }
    }
}

#[typetag::serde(name = "docker")]
impl SourceConfig for DockerConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: Sender<Event>,
    ) -> Result<super::Source, String> {
        docker_source(self.clone(), out).map(|f| Box::new(f) as Box<_>)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
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

    /// If info is present, runs event stream
    fn run_event_stream(
        &mut self,
        out: &Sender<Event>,
        main: &Sender<ContainerLogInfo>,
        docker: &Docker,
    ) {
        if let Some(info) = self.info.take() {
            run_event_stream(self, info, out.clone(), main.clone(), docker);
        }
    }
}

/// Returns main future which listens for events coming from docker, and maintains
/// a fan of event_stream futures.
/// Where each event_stream corresponds to a runing container marked with ContainerLogInfo.
/// While running, event_stream streams Events to out channel.
/// Once a log stream has ended, it sends ContainerLogInfo back to main.
///
/// Future  channel     Future      channel
///
///           |<---- event_stream ---->out
/// main <----|<---- event_stream ---->out
///           | ...                 ...out
///
#[allow(dead_code)]
fn docker_source(
    config: DockerConfig,
    out: Sender<Event>,
) -> Result<impl Future<Item = (), Error = ()>, String> {
    // TODO: async_docker should be replaced with bollard once it supports events
    // ?NOTE: Requiers sudo privileges, or docker group membership.
    // Without extra configuration of docker on user side, there is no way around above.
    let docker_for_events = async_docker::new_docker(None).map_err(|error| {
        error!(message="Error connecting to docker server",%error);
        "Failed to connect to docker server".to_owned()
    })?;

    // ?NOTE: Requiers sudo privileges, or docker group membership.
    // Without extra configuration of docker on user side, there is no way around above.
    let docker = Docker::connect_with_local_defaults().map_err(|error| {
        error!(message="Error connecting to docker server",%error);
        "Failed to connect to docker server".to_owned()
    })?;

    // Channel could be unbounded, since there won't be millions of containers,
    // but bounded should be more performant in most cases.
    let (main_send, mut main_recv) = mpsc::channel::<ContainerLogInfo>(100);

    // main event stream, with whom only newly started/restarted containers will be loged.
    let mut events = {
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

        // ?NOTE: by docker API, using both type of include results in AND between them

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

        docker_for_events.events(&options.build())
    };
    info!(message = "Listening docker events");

    // Starting with logs from now.
    // TODO: Is this exception acceptable?
    // Only somewhat exception to this is case where:
    // t0 -- outside: container running
    // t1 -- now_timestamp
    // t2 -- outside: container stoped
    // t3 -- list_containers
    // In that case, logs between [t1,t2] will be pulled to vector only on next start/unpause of that container.
    let now = chrono::Local::now();
    let now_timestamp = now.timestamp();
    info!(
        message = "Capturing logs from now on",
        now = field::display(now.to_rfc3339())
    );

    // Future that captures currently running containers, and starts event streams for them.
    let now_running = {
        let mut options = ListContainersOptions::default();

        // ?NOTE: by docker API, using both type of include results in AND between them

        // Include-name
        if !config.include_containers.is_empty() {
            options
                .filters
                .insert("name".to_owned(), config.include_containers.clone());
        }

        // Include-label
        if !config.include_labels.is_empty() {
            options
                .filters
                .insert("label".to_owned(), config.include_labels.clone());
        }

        let docker = docker.clone();
        let main_send = main_send.clone();
        let out = out.clone();

        // Future
        docker.list_containers(Some(options)).map(move |list| {
            let mut containers = HashMap::<String, ContainerState>::new();
            for container in list {
                trace!(
                    message = "Found already running container",
                    id = field::display(&container.id)
                );
                let mut state = ContainerState::new(container.id.clone(), now_timestamp);
                state.run_event_stream(&out, &main_send, &docker);
                containers.insert(container.id, state);
            }
            containers
        })
    };

    // Main docker source future
    let main = now_running.then(move |result| {
        let mut containers = result
            .map_err(|error| error!(message="Listing currently running containers, failed",%error))
            .unwrap_or_default();

        poll_fn(move || loop {
            match main_recv.poll() {
                // Process message from event_stream
                Ok(Async::Ready(Some(info))) => {
                    let v = containers
                        .get_mut(&info.id)
                        .expect("Every ContainerLogInfo has it's ContainerState");
                    if v.running || info.generation < v.generation {
                        // Generation is the only one strictly necessary,
                        // but with v.running, restarting event_stream is automtically done.
                        run_event_stream(&v, info, out.clone(), main_send.clone(), &docker);
                    } else {
                        v.info = Some(info);
                    }
                }
                // Check events from docker
                Ok(Async::NotReady) => {
                    match events.poll() {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        // Process event from docker
                        Ok(Async::Ready(Some(Ok(event)))) => {
                            match (event.id.as_ref(), event.status.as_ref()) {
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
                                            if let Some(v) = containers.get_mut(id) {
                                                v.running = false;
                                            }
                                        }
                                        "start" | "upause" => {
                                            if let Some(state) = containers.get_mut(id) {
                                                state.running = true;
                                                state.generation += 1;

                                                state.run_event_stream(&out, &main_send, &docker);
                                            } else {
                                                let mut state = ContainerState::new(
                                                    id.clone(),
                                                    // logs can actually come with timestamps before timestamp of this event, so no now_timestamp.max(event.time as i64),
                                                    now_timestamp,
                                                );
                                                state.run_event_stream(&out, &main_send, &docker);
                                                containers.insert(id.clone(), state);
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
        })
    });
    // Done
    Ok(main)
}

fn run_event_stream(
    container: &ContainerState,
    mut info: ContainerLogInfo,
    out: Sender<Event>,
    main: Sender<ContainerLogInfo>,
    docker: &Docker,
) {
    // Update info
    info.generation = container.generation;

    // Establish connection
    let options = LogsOptions {
        follow: true,
        stdout: true,
        stderr: true,
        since: info
            .last_log
            .as_ref()
            .map(|&(ref d, _)| d.timestamp())
            .unwrap_or(info.created)
            - 1,
        timestamps: true,
        ..Default::default()
    };
    let mut stream = docker.logs(&info.id, Some(options));
    info!(
        message = "Started listening logs on docker container",
        id = field::display(&info.id)
    );

    // Create event streamer
    let mut state = Some((main, info));
    let event_stream = tokio::prelude::stream::poll_fn(move || {
        // !Hot code: from here
        if let Some(&mut (_, ref mut info)) = state.as_mut() {
            // Main event loop
            loop {
                return match stream.poll() {
                    Ok(Async::Ready(Some(message))) => {
                        if let Some(event) = log_to_event(message, info) {
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
                        Err(())
                    }
                };
            }
        }

        if let Some((main, info)) = state.take() {
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
        }

        Ok(Async::Ready(None))
    })
    .forward(out.sink_map_err(|_| ()))
    .map(|_| ());

    // Run event_stream
    tokio::spawn(event_stream);
}

/// Expects timestamp at the begining of message
/// Expects messages to be ordered by timestamps
fn log_to_event(message: LogOutput, info: &mut ContainerLogInfo) -> Option<Event> {
    let mut log_event = Event::new_empty_log().into_log();

    // TODO: Source could be supplied to log_event, but should it, and how to name it?
    let (message, _) = match message {
        LogOutput::StdErr { message } => (message, "stderr"),
        LogOutput::StdOut { message } => (message, "stdout"),
        _ => return None,
    };

    let mut splitter = message.splitn(2, char::is_whitespace);
    let timestamp_str = splitter.next()?;
    let log = match DateTime::parse_from_rfc3339(timestamp_str) {
        Ok(timestamp) => {
            // Timestamp check
            match info.last_log.as_ref() {
                // Recieved log has already been processed
                Some(&(ref last, gen)) => match last.cmp(&timestamp) {
                    Ordering::Greater => {
                        trace!(
                            message = "Recieved older log",
                            timestamp = field::display(timestamp_str)
                        );
                        return None;
                    }
                    Ordering::Equal if gen < info.generation => {
                        trace!(
                            message = "Recieved log from previous container run",
                            timestamp = field::display(timestamp_str)
                        );
                        return None;
                    }
                    _ => (),
                },
                // Recieved log is from before of creation
                None if info.created > timestamp.timestamp() => {
                    trace!(
                        message = "Recieved backlog",
                        timestamp = field::display(timestamp_str)
                    );
                    return None;
                }
                _ => (),
            }
            // Supply timestamp
            log_event.insert_explicit(
                event::TIMESTAMP.clone(),
                timestamp.with_timezone(&chrono::Utc).into(),
            );

            info.last_log = Some((timestamp, info.generation));
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

    // Supply host
    // TODO: use event::HOST or create new naming, or don't supply at all?
    //log_event.insert_implicit(event::HOST.clone(), info.id.as_str().into());

    let event = Event::Log(log_event);
    trace!(message = "Received one event", event = field::debug(&event));
    Some(event)
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
