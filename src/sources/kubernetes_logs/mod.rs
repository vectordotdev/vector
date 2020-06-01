//! This mod implements `kubernetes_logs` source.
//! The scope of this source is to consume the log files that `kubelet` keeps
//! at `/var/log/pods` at the host of the k8s node.

#![deny(missing_docs)]

mod k8s_paths_provider;
mod parser;
mod partial_events_merger;
mod path_helpers;
mod pod_metadata_annotator;

use crate::event::{self, Event};
use crate::internal_events::KubernetesLogsEventReceived;
use crate::kubernetes as k8s;
use crate::{
    dns::Resolver,
    shutdown::ShutdownSignal,
    sources,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    transforms::Transform,
};
use evmap10::{self as evmap};
use file_source::{FileServer, FileServerShutdown, Fingerprinter};
use futures::{future::FutureExt, sink::Sink, stream::StreamExt};
use futures01::sync::mpsc;
use k8s_openapi::api::core::v1::Pod;
use k8s_paths_provider::K8sPathsProvider;
use pod_metadata_annotator::PodMetadataAnnotator;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::spawn_blocking;

/// The key we use for `file` field.
const FILE_KEY: &str = "file";

/// Configuration for the `kubernetes_logs` source.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    self_node_name: String,
}

inventory::submit! {
    SourceDescription::new_without_default::<Config>(COMPONENT_NAME)
}

const COMPONENT_NAME: &str = "kubernetes_logs";

#[typetag::serde(name = "kubernetes_logs")]
impl SourceConfig for Config {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        shutdown: ShutdownSignal,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<sources::Source> {
        let source = Source::init(self, Resolver, globals, name)?;

        // TODO: this is a workaround for the legacy futures 0.1.
        // When the core is updated to futures 0.3 this should be simplied
        // significantly.
        let out = futures::compat::Compat01As03Sink::new(out);
        let fut = source.run(out, shutdown);
        let fut = fut.map(|result| {
            result.map_err(|error| {
                error!(message = "source future failed", ?error);
            })
        });
        let fut = Box::pin(fut);
        let fut = futures::compat::Compat::new(fut);
        let fut: sources::Source = Box::new(fut);
        Ok(fut)
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        COMPONENT_NAME
    }
}

#[derive(Clone)]
struct Source {
    client: k8s::client::Client,
    self_node_name: String,
    data_dir: PathBuf,
}

impl Source {
    fn init(
        config: &Config,
        resolver: Resolver,
        globals: &GlobalOptions,
        name: &str,
    ) -> crate::Result<Self> {
        let self_node_name = if config.self_node_name.is_empty() {
            std::env::var("VECTOR_SELF_NODE_NAME")
                .map_err(|_| "VECTOR_SELF_NODE_NAME is not set")?
        } else {
            config.self_node_name.clone()
        };
        info!(
            message = "obtained Kubernetes Node name to collect logs for (self)",
            ?self_node_name
        );

        let k8s_config = k8s::client::config::Config::in_cluster()?;
        let client = k8s::client::Client::new(k8s_config, resolver)?;

        let data_dir = globals.resolve_and_make_data_subdir(None, name)?;

        Ok(Self {
            client,
            self_node_name,
            data_dir,
        })
    }

    async fn run<O>(self, out: O, shutdown: ShutdownSignal) -> crate::Result<()>
    where
        O: Sink<Event> + Send,
        <O as Sink<Event>>::Error: std::error::Error,
    {
        let Self {
            client,
            self_node_name,
            data_dir,
        } = self;

        let field_selector = format!("spec.nodeName={}", self_node_name);
        let label_selector = "vector.dev/exclude!=true".to_owned();

        let watcher = k8s::api_watcher::ApiWatcher::new(client, Pod::watch_pod_for_all_namespaces);
        let (state_reader, state_writer) = evmap::new();

        let mut reflector = k8s::reflector::Reflector::new(
            watcher,
            state_writer,
            Some(field_selector),
            Some(label_selector),
            std::time::Duration::from_secs(1),
        );
        let reflector_process = reflector.run();

        let paths_provider = K8sPathsProvider::new(state_reader.clone());
        let annotator = PodMetadataAnnotator::new(state_reader);

        // TODO: maybe some of the parameters have to be configurable.
        let file_server = FileServer {
            paths_provider,
            max_read_bytes: 2048,
            start_at_beginning: true,
            ignore_before: None,
            max_line_bytes: 32 * 1024, // 32 KiB
            data_dir,
            glob_minimum_cooldown: Duration::from_secs(10),
            // Use device inodes for fingerprinting.
            // - Docker recreates files on rotation: https://github.com/moby/moby/blob/75d655320e2a443185f8fa4992dc89bd2da0ea68/daemon/logger/loggerutils/logfile.go#L182-L222
            // - CRI-O recreates files on rotation: https://github.com/cri-o/cri-o/blob/ad83d2a35a30b8a336b16a0ea5f7afc6aebfb9b7/internal/oci/runtime_oci.go#L988-L1042
            // The rest should do the same.
            fingerprinter: Fingerprinter::DevInode,
            oldest_first: false,
            remove_after: None,
        };

        let (file_source_tx, file_source_rx) = futures::channel::mpsc::channel(100);

        let span = info_span!("file_server");
        let file_server_join_handle = spawn_blocking(move || {
            let _enter = span.enter();
            let result =
                file_server.run(file_source_tx, futures::compat::Compat01As03::new(shutdown));
            result.expect("file server exited with an error")
        });

        let mut parser = parser::build();
        let mut partial_events_merger = partial_events_merger::build(true);

        let events = file_source_rx.map(|(bytes, file)| {
            emit!(KubernetesLogsEventReceived {
                file: &file,
                byte_size: bytes.len(),
            });
            create_event(bytes, file)
        });
        let events = events
            .filter_map(move |event| futures::future::ready(parser.transform(event)))
            .filter_map(move |event| futures::future::ready(partial_events_merger.transform(event)))
            .map(move |mut event| {
                if annotator.annotate(&mut event).is_none() {
                    warn!(
                        message = "failed to annotate event with pod metadata",
                        ?event
                    );
                }
                event
            });

        let event_processing_loop = events.map(Ok).forward(out);

        use std::future::Future;
        use std::pin::Pin;
        let list: Vec<Pin<Box<dyn Future<Output = ()> + Send>>> = vec![
            Box::pin(reflector_process.map(|result| match result {
                Ok(_val) => info!(message = "reflector process completed gracefully"),
                Err(error) => error!(message = "reflector process exited with an error", ?error),
            })),
            Box::pin(file_server_join_handle.map(|result| match result {
                Ok(FileServerShutdown) => info!(message = "file server completed gracefully"),
                Err(error) => error!(message = "file server exited with an error", ?error),
            })),
            Box::pin(event_processing_loop.map(|result| {
                match result {
                    Ok(()) => info!(message = "event processing loop completed gracefully"),
                    Err(error) => error!(
                        message = "event processing loop exited with an error",
                        ?error
                    ),
                };
            })),
        ];
        let mut futs: futures::stream::FuturesUnordered<_> = list.into_iter().collect();

        while let Some(()) = futs.next().await {
            trace!(message = "another future complete");
        }

        info!(message = "done");
        Ok(())
    }
}

fn create_event(line: bytes05::Bytes, file: String) -> Event {
    let mut event = Event::from(line);

    // Add source type.
    event
        .as_mut_log()
        .insert(event::log_schema().source_type_key(), COMPONENT_NAME);

    // Add file.
    event.as_mut_log().insert(FILE_KEY, file);

    event
}
