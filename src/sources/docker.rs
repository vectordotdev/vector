use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bollard::{
    container::{LogOutput, LogsOptions},
    Docker,
};
use chrono::{DateTime, FixedOffset};
use futures::{future::poll_fn, sync::mpsc, Async, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::field;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DockerConfig {
    include_containers: Vec<String>,
    include_labels: Vec<String>,
    exclude_containers: Vec<String>,
    exclude_labels: Vec<String>,
}

#[typetag::serde(name = "docker")]
impl SourceConfig for DockerConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> Result<super::Source, String> {
        docker_source(self.clone(), out).map(|f| Box::new(f) as Box<_>)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

/// Exchanged between main future and event_stream futures
struct ContainerLogInfo {
    /// Container Docker ID
    id: String,
    /// Unix timestamp of event which created this struct
    created: i64,
    /// Timestamp of last log message
    last_log: Option<DateTime<FixedOffset>>,
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
    _: DockerConfig,
    out: mpsc::Sender<Event>,
) -> Result<impl Future<Item = (), Error = ()>, String> {
    // TODO: What about currently running containers
    // TODO: use DockerConfig

    // TODO: async_docker should be replaced with bollard once it supports events
    let docker_for_events = async_docker::new_docker(None).map_err(|error| {
        error!(message="Error connecting to docker server",%error);
        "Failed to connect to docker server".to_owned()
    })?;

    let docker = Docker::connect_with_local_defaults().map_err(|error| {
        error!(message="Error connecting to docker server",%error);
        "Failed to connect to docker server".to_owned()
    })?;

    // Will log only newly started/restarted containers.
    // event - emmited on commands
    // ---------------------------
    // start - docker start, docker run, restart policy, docker restart
    // upause - docker unpause
    // die - docker restart, docker stop, docker kill, process exited, oom
    // pause - docker pause
    let events = vec!["start", "upause", "die", "pause"];
    let options = async_docker::EventsOptionsBuilder::new()
        .filter(
            events
                .into_iter()
                .map(|s| async_docker::EventFilter::Event(s.into()))
                .collect(),
        )
        .build();
    let mut events = docker_for_events.events(&options);
    info!(message = "Listening Docker events");

    let mut containers = HashMap::<String, ContainerState>::new();
    // Channel could be unbounded, since there won't be millions of containers,
    // but bounded should be more performant in most cases.
    let (main_send, mut main_recv) = mpsc::channel::<ContainerLogInfo>(100);

    // Main
    Ok(poll_fn(move || loop {
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
            // Check events from Docker
            Ok(Async::NotReady) => {
                match events.poll() {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    // Process event from Docker
                    Ok(Async::Ready(Some(Ok(event)))) => {
                        match (event.id.as_ref(), event.status.as_ref()) {
                            (Some(id), Some(status)) => {
                                // Update container status
                                match status.as_str() {
                                    "die" | "pause" => {
                                        if let Some(v) = containers.get_mut(id) {
                                            v.running = false;
                                        }
                                    }
                                    "start" | "upause" => {
                                        if let Some(v) = containers.get_mut(id) {
                                            v.running = true;
                                            v.generation += 1;

                                            if let Some(info) = v.info.take() {
                                                run_event_stream(
                                                    &v,
                                                    info,
                                                    out.clone(),
                                                    main_send.clone(),
                                                    &docker,
                                                );
                                            }
                                        } else {
                                            let v = ContainerState {
                                                info: None,
                                                running: true,
                                                generation: 0,
                                            };
                                            let info = ContainerLogInfo {
                                                id: id.clone(),
                                                created: event.time as i64,
                                                last_log: None,
                                                generation: 0,
                                            };
                                            run_event_stream(
                                                &v,
                                                info,
                                                out.clone(),
                                                main_send.clone(),
                                                &docker,
                                            );

                                            containers.insert(id.clone(), v);
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
                    // Stream has ended
                    Ok(Async::Ready(None)) => {
                        // TODO: this could be fixed, but should be tryed with some timeoff, with exponential backoff
                        error!(message = "Docker event stream has ended unexpectedly");
                        info!(message = "Shuting down Docker source");
                        return Err(());
                    }
                    Ok(Async::Ready(Some(Err(error)))) => {
                        error!(message = "Error in Docker event stream",%error)
                    }
                    Err(error) => error!(source="Docker events",%error),
                }
            }
            Err(()) => error!(message = "Error in Docker source main stream"),
            // For some strange reason stream has ended.
            // It should never reach this point. But if it does,
            // something has gone terrible wrong, and this system is probably
            // in invalid state.
            Ok(Async::Ready(None)) => {
                error!(message = "Docker source main stream has ended unexpectedly");
                info!(message = "Shuting down Docker source");
                return Err(());
            }
        }
    }))
}

fn run_event_stream(
    container: &ContainerState,
    mut info: ContainerLogInfo,
    out: mpsc::Sender<Event>,
    main: mpsc::Sender<ContainerLogInfo>,
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
            .map(|d| d.timestamp())
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

    let mut state = Some((main, info));
    // Create event streamer
    let event_stream = tokio::prelude::stream::poll_fn(move || {
        if let Some(&mut (_, ref mut info)) = state.as_mut() {
            // Main event loop
            loop {
                return match stream.poll() {
                    Ok(Async::Ready(Some(message))) => {
                        if let Some(event) = process_logoutput(message, info) {
                            Ok(Async::Ready(Some(event)))
                        } else {
                            continue;
                        }
                    }
                    Ok(Async::Ready(None)) => break,
                    Ok(Async::NotReady) => Ok(Async::NotReady),
                    Err(error) => {
                        error!(message = "Docker API container logging error",%error);
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
fn process_logoutput(message: LogOutput, info: &mut ContainerLogInfo) -> Option<Event> {
    let mut log_event = Event::new_empty_log().into_log();

    // TODO: Source could be supplied to log_event
    let (message, _) = match message {
        LogOutput::StdErr { message } => (message, "stderr"),
        LogOutput::StdOut { message } => (message, "stdout"),
        _ => return None,
    };

    let mut splitter = message.splitn(2, char::is_whitespace);
    let timestamp = splitter.next()?;
    let log = match DateTime::parse_from_rfc3339(timestamp) {
        Ok(timestamp) => {
            match info.last_log.as_ref() {
                // Recieved log has already been processed
                Some(last) if last > &timestamp => return None,
                _ => (),
            }
            // Supply timestamp
            log_event.insert_explicit(
                event::TIMESTAMP.clone(),
                timestamp.with_timezone(&chrono::Utc).into(),
            );

            info.last_log = Some(timestamp);
            splitter.next()?
        }
        Err(error) => {
            // Recieved bad timestamp, if any at all.
            error!(message="Didn't recieve rfc3339 timestamp from Docker",%error);
            // So log whole message
            message.as_str()
        }
    };

    // Supply message
    log_event.insert_explicit(event::MESSAGE.clone(), log.into());

    // Supply host
    log_event.insert_implicit(event::HOST.clone(), info.id.as_str().into());

    let event = Event::Log(log_event);
    trace!(message = "Received one event", event = field::debug(&event));
    Some(event)
}
