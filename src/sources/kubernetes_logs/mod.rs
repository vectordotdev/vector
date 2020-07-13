//! This mod implements `kubernetes_logs` source.
//! The scope of this source is to consume the log files that `kubelet` keeps
//! at `/var/log/pods` at the host of the k8s node when `vector` itself is
//! running inside the cluster as a `DaemonSet`.

#![deny(missing_docs)]

use crate::event::{self, Event};
use crate::internal_events::{KubernetesLogsEventAnnotationFailed, KubernetesLogsEventReceived};
use crate::kubernetes as k8s;
use crate::{
    dns::Resolver,
    shutdown::ShutdownSignal,
    sources,
    topology::config::{DataType, GlobalOptions, SourceConfig, SourceDescription},
    transforms::Transform,
};
use bytes05::Bytes;
use evmap10::{self as evmap};
use file_source::{FileServer, FileServerShutdown, Fingerprinter};
use futures::{future::FutureExt, sink::Sink, stream::StreamExt};
use futures01::sync::mpsc;
use k8s_openapi::api::core::v1::Pod;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

mod k8s_paths_provider;
mod lifecycle;
mod parser;
mod partial_events_merger;
mod path_helpers;
mod pod_metadata_annotator;
mod transform_utils;
mod util;

use k8s_paths_provider::K8sPathsProvider;
use lifecycle::Lifecycle;
use pod_metadata_annotator::PodMetadataAnnotator;

/// The key we use for `file` field.
const FILE_KEY: &str = "file";

/// The `self_node_name` value env var key.
const SELF_NODE_NAME_ENV_KEY: &str = "VECTOR_SELF_NODE_NAME";

/// Configuration for the `kubernetes_logs` source.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// The `name` of the Kubernetes `Node` that Vector runs at.
    /// Required to filter the `Pod`s to only include the ones with the log
    /// files accessible locally.
    #[serde(default = "default_self_node_name_env_template")]
    self_node_name: String,

    /// Automatically merge partial events.
    #[serde(default = "crate::serde::default_true")]
    auto_partial_merge: bool,

    /// Specifies the field names for metadata annotation.
    annotation_fields: pod_metadata_annotator::FieldsSpec,
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
        let source = Source::new(self, Resolver, globals, name)?;

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
    auto_partial_merge: bool,
    fields_spec: pod_metadata_annotator::FieldsSpec,
}

impl Source {
    fn new(
        config: &Config,
        resolver: Resolver,
        globals: &GlobalOptions,
        name: &str,
    ) -> crate::Result<Self> {
        let self_node_name = if config.self_node_name.is_empty()
            || config.self_node_name == default_self_node_name_env_template()
        {
            std::env::var(SELF_NODE_NAME_ENV_KEY).map_err(|_| {
                format!(
                    "self_node_name config value or {} env var is not set",
                    SELF_NODE_NAME_ENV_KEY
                )
            })?
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
            auto_partial_merge: config.auto_partial_merge,
            fields_spec: config.annotation_fields.clone(),
        })
    }

    async fn run<O>(self, out: O, global_shutdown: ShutdownSignal) -> crate::Result<()>
    where
        O: Sink<Event> + Send + 'static,
        <O as Sink<Event>>::Error: std::error::Error,
    {
        let Self {
            client,
            self_node_name,
            data_dir,
            auto_partial_merge,
            fields_spec,
        } = self;

        let field_selector = format!("spec.nodeName={}", self_node_name);
        let label_selector = "vector.dev/exclude!=true".to_owned();

        let watcher = k8s_runtime::client::Watcher::new(client, Pod::watch_pod_for_all_namespaces);
        let watcher = k8s::instrumenting_watcher::InstrumentingWatcher::new(watcher);
        let (state_reader, state_writer) = evmap::new();
        let state_writer =
            k8s_runtime::state::evmap::Writer::new(state_writer, Some(Duration::from_millis(10)));
        let state_writer = k8s::instrumenting_state::Writer::new(state_writer);
        let state_writer =
            k8s_runtime::state::delayed_delete::Writer::new(state_writer, Duration::from_secs(60));

        let mut reflector = k8s_runtime::reflector::Reflector::new(
            watcher,
            state_writer,
            Some(field_selector),
            Some(label_selector),
            Duration::from_secs(1),
        );
        let reflector_process = reflector.run();

        let paths_provider = K8sPathsProvider::new(state_reader.clone());
        let annotator = PodMetadataAnnotator::new(state_reader, fields_spec);

        // TODO: maybe some of the parameters have to be configurable.
        let max_line_bytes = 32 * 1024; // 32 KiB
        let file_server = FileServer {
            paths_provider,
            max_read_bytes: 2048,
            start_at_beginning: true,
            ignore_before: None,
            max_line_bytes,
            data_dir,
            glob_minimum_cooldown: Duration::from_secs(10),
            fingerprinter: Fingerprinter::FirstLineChecksum {
                max_line_length: max_line_bytes,
            },
            oldest_first: false,
            remove_after: None,
        };

        let (file_source_tx, file_source_rx) =
            futures::channel::mpsc::channel::<(Bytes, String)>(100);

        let mut parser = parser::build();
        let mut partial_events_merger = partial_events_merger::build(auto_partial_merge);

        let events = file_source_rx.map(move |(bytes, file)| {
            emit!(KubernetesLogsEventReceived {
                file: &file,
                byte_size: bytes.len(),
            });
            let mut event = create_event(bytes, &file);
            if annotator.annotate(&mut event, &file).is_none() {
                emit!(KubernetesLogsEventAnnotationFailed { event: &event });
            }
            event
        });
        let events = events
            .filter_map(move |event| futures::future::ready(parser.transform(event)))
            .filter_map(move |event| {
                futures::future::ready(partial_events_merger.transform(event))
            });

        let event_processing_loop = events.map(Ok).forward(out);

        let mut lifecycle = Lifecycle::new();
        {
            let (slot, shutdown) = lifecycle.add();
            let fut =
                util::cancel_on_signal(reflector_process, shutdown).map(|result| match result {
                    Ok(()) => info!(message = "reflector process completed gracefully"),
                    Err(error) => {
                        error!(message = "reflector process exited with an error", ?error)
                    }
                });
            slot.bind(Box::pin(fut));
        }
        {
            let (slot, shutdown) = lifecycle.add();
            let fut = util::run_file_server(file_server, file_source_tx, shutdown).map(|result| {
                match result {
                    Ok(FileServerShutdown) => info!(message = "file server completed gracefully"),
                    Err(error) => error!(message = "file server exited with an error", ?error),
                }
            });
            slot.bind(Box::pin(fut));
        }
        {
            let (slot, shutdown) = lifecycle.add();
            let fut = util::complete_with_deadline_on_signal(
                event_processing_loop,
                shutdown,
                Duration::from_secs(30), // more than enough time to propagate
            )
            .map(|result| {
                match result {
                    Ok(Ok(())) => info!(message = "event processing loop completed gracefully"),
                    Ok(Err(error)) => error!(
                        message = "event processing loop exited with an error",
                        ?error
                    ),
                    Err(error) => error!(
                        message = "event processing loop timed out during the shutdown",
                        ?error
                    ),
                };
            });
            slot.bind(Box::pin(fut));
        }

        lifecycle.run(global_shutdown).await;
        info!(message = "done");
        Ok(())
    }
}

fn create_event(line: Bytes, file: &str) -> Event {
    let mut event = Event::from(line);

    // Add source type.
    event
        .as_mut_log()
        .insert(event::log_schema().source_type_key(), COMPONENT_NAME);

    // Add file.
    event.as_mut_log().insert(FILE_KEY, file);

    event
}

/// This function returns the default value for `self_node_name` variable
/// as it should be at the generated config file.
fn default_self_node_name_env_template() -> String {
    format!("${{{}}}", SELF_NODE_NAME_ENV_KEY)
}
