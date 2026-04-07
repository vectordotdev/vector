use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use futures::StreamExt;
use http_1::{HeaderName, HeaderValue};
use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
use kube::{
    Client, Config as ClientConfig,
    api::Api,
    config::{self, KubeConfigOptions},
    runtime::{WatchStreamExt, reflector, watcher},
};
use tokio::{
    sync::{Semaphore, mpsc},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use vector_lib::config::LogNamespace;

use crate::{
    SourceSender,
    built_info::{PKG_NAME, PKG_VERSION},
    config::GlobalOptions,
    event::Event,
    kubernetes::{custom_reflector, meta_cache::MetaCache},
    shutdown::ShutdownSignal,
    sources::kubernetes_logs::{
        namespace_metadata_annotator::{
            FieldsSpec as NamespaceFieldsSpec, NamespaceMetadataAnnotator,
        },
        node_metadata_annotator::{FieldsSpec as NodeFieldsSpec, NodeMetadataAnnotator},
        pod_metadata_annotator::FieldsSpec as PodFieldsSpec,
    },
};

use super::stream::{StreamRuntime, reconcile_active_streams};
use super::util::{
    apply_pod_event, get_page_size, prepare_field_selector, prepare_label_selector,
    prepare_node_selector, remove_pod,
};
use super::{Config, EVENT_CHANNEL_SIZE, RECONCILE_INTERVAL};
use crate::sources::kubernetes_logs::{SELF_NODE_NAME_ENV_KEY, default_self_node_name_env_template};

#[derive(Clone)]
pub(super) struct Source {
    pub(super) client: Client,
    pub(super) field_selector: String,
    pub(super) label_selector: String,
    pub(super) namespace_label_selector: String,
    pub(super) insert_namespace_fields: bool,
    pub(super) node_selector: String,
    pub(super) self_node_name: String,
    pub(super) pod_fields_spec: PodFieldsSpec,
    pub(super) namespace_fields_spec: NamespaceFieldsSpec,
    pub(super) node_fields_spec: NodeFieldsSpec,
    pub(super) container: Option<String>,
    pub(super) tail_lines: i64,
    pub(super) since_seconds: i64,
    pub(super) max_log_requests: usize,
    pub(super) use_apiserver_cache: bool,
    pub(super) delay_deletion: Duration,
    pub(super) ingestion_timestamp_field: Option<vector_lib::lookup::OwnedTargetPath>,
}

impl Source {
    pub(super) async fn new(config: &Config, _globals: &GlobalOptions) -> crate::Result<Self> {
        let self_node_name = if config.self_node_name.is_empty()
            || config.self_node_name == default_self_node_name_env_template()
        {
            std::env::var(SELF_NODE_NAME_ENV_KEY).map_err(|_| {
                format!(
                    "self_node_name config value or {SELF_NODE_NAME_ENV_KEY} env var is not set"
                )
            })?
        } else {
            config.self_node_name.clone()
        };

        let field_selector =
            prepare_field_selector(&config.extra_field_selector, &self_node_name)?;
        let label_selector = prepare_label_selector(&config.extra_label_selector);
        let namespace_label_selector =
            prepare_label_selector(&config.extra_namespace_label_selector);
        let node_selector = prepare_node_selector(&self_node_name)?;

        let mut client_config = match &config.kube_config_file {
            Some(kc) => {
                ClientConfig::from_custom_kubeconfig(
                    config::Kubeconfig::read_from(kc)?,
                    &KubeConfigOptions::default(),
                )
                .await?
            }
            None => ClientConfig::infer().await?,
        };
        if let Ok(user_agent) = HeaderValue::from_str(&format!("{PKG_NAME}/{PKG_VERSION}")) {
            client_config
                .headers
                .push((HeaderName::from_static("user-agent"), user_agent));
        }
        let client = Client::try_from(client_config)?;

        Ok(Self {
            client,
            field_selector,
            label_selector,
            namespace_label_selector,
            insert_namespace_fields: config.insert_namespace_fields,
            node_selector,
            self_node_name,
            pod_fields_spec: config.pod_annotation_fields.clone(),
            namespace_fields_spec: config.namespace_annotation_fields.clone(),
            node_fields_spec: config.node_annotation_fields.clone(),
            container: config.container.clone(),
            tail_lines: config.tail_lines,
            since_seconds: config.since_seconds,
            max_log_requests: config.max_log_requests,
            use_apiserver_cache: config.use_apiserver_cache,
            delay_deletion: config.delay_deletion_ms,
            ingestion_timestamp_field: config
                .ingestion_timestamp_field
                .clone()
                .and_then(|k| k.path),
        })
    }

    pub(super) async fn run(
        self,
        mut out: SourceSender,
        mut shutdown: ShutdownSignal,
        log_namespace: LogNamespace,
    ) -> crate::Result<()> {
        let Self {
            client,
            field_selector,
            label_selector,
            namespace_label_selector,
            insert_namespace_fields,
            node_selector,
            self_node_name,
            pod_fields_spec,
            namespace_fields_spec,
            node_fields_spec,
            container,
            tail_lines,
            since_seconds,
            max_log_requests,
            use_apiserver_cache,
            delay_deletion,
            ingestion_timestamp_field,
        } = self;

        let list_semantic = if use_apiserver_cache {
            watcher::ListSemantic::Any
        } else {
            watcher::ListSemantic::MostRecent
        };

        let mut reflectors = Vec::new();

        let ns_store_w = reflector::store::Writer::default();
        let ns_state = ns_store_w.as_reader();
        if insert_namespace_fields {
            let namespaces = Api::<Namespace>::all(client.clone());
            let ns_watcher = watcher(
                namespaces,
                watcher::Config {
                    label_selector: Some(namespace_label_selector),
                    list_semantic: list_semantic.clone(),
                    page_size: get_page_size(use_apiserver_cache),
                    ..Default::default()
                },
            )
            .backoff(watcher::DefaultBackoff::default());

            reflectors.push(tokio::spawn(custom_reflector(
                ns_store_w,
                MetaCache::new(),
                ns_watcher,
                delay_deletion,
            )));
        }

        let nodes = Api::<Node>::all(client.clone());
        let node_watcher = watcher(
            nodes,
            watcher::Config {
                field_selector: Some(node_selector),
                list_semantic,
                page_size: get_page_size(use_apiserver_cache),
                ..Default::default()
            },
        )
        .backoff(watcher::DefaultBackoff::default());
        let node_store_w = reflector::store::Writer::default();
        let node_state = node_store_w.as_reader();
        reflectors.push(tokio::spawn(custom_reflector(
            node_store_w,
            MetaCache::new(),
            node_watcher,
            delay_deletion,
        )));

        let namespace_annotator = NamespaceMetadataAnnotator::new(
            ns_state,
            namespace_fields_spec,
            log_namespace,
            Config::NAME,
        );
        let node_annotator =
            NodeMetadataAnnotator::new(node_state, node_fields_spec, log_namespace, Config::NAME);

        let pods_api = Api::<Pod>::all(client.clone());
        let mut pod_events = watcher(
            pods_api,
            watcher::Config {
                field_selector: Some(field_selector),
                label_selector: Some(label_selector),
                page_size: get_page_size(use_apiserver_cache),
                ..Default::default()
            },
        )
        .backoff(watcher::DefaultBackoff::default())
        .boxed();

        let semaphore = (max_log_requests > 0).then(|| Semaphore::new(max_log_requests));
        let semaphore = semaphore.map(std::sync::Arc::new);
        let (event_tx, mut event_rx) = mpsc::channel::<Event>(EVENT_CHANNEL_SIZE);

        let mut pods = HashMap::<String, Pod>::new();
        let mut active = HashMap::<String, JoinHandle<()>>::new();
        let mut initialized = HashSet::<String>::new();

        let mut reconcile = interval(RECONCILE_INTERVAL);
        reconcile.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            if out.send_event(event).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                watcher_event = pod_events.next() => {
                    match watcher_event {
                        Some(Ok(event)) => {
                            match event {
                                watcher::Event::Apply(pod) | watcher::Event::InitApply(pod) => {
                                    apply_pod_event(&mut pods, pod);
                                    reconcile_active_streams(
                                        &pods,
                                        &container,
                                        &mut active,
                                        &mut initialized,
                                        StreamRuntime {
                                            client: &client,
                                            semaphore: semaphore.as_ref(),
                                            tx: &event_tx,
                                            pod_fields_spec: &pod_fields_spec,
                                            namespace_annotator: &namespace_annotator,
                                            node_annotator: &node_annotator,
                                            self_node_name: &self_node_name,
                                            tail_lines,
                                            since_seconds,
                                            ingestion_timestamp_field: ingestion_timestamp_field.as_ref(),
                                            log_namespace,
                                        },
                                    );
                                }
                                watcher::Event::Delete(pod) => {
                                    remove_pod(&mut pods, &pod);
                                    reconcile_active_streams(
                                        &pods,
                                        &container,
                                        &mut active,
                                        &mut initialized,
                                        StreamRuntime {
                                            client: &client,
                                            semaphore: semaphore.as_ref(),
                                            tx: &event_tx,
                                            pod_fields_spec: &pod_fields_spec,
                                            namespace_annotator: &namespace_annotator,
                                            node_annotator: &node_annotator,
                                            self_node_name: &self_node_name,
                                            tail_lines,
                                            since_seconds,
                                            ingestion_timestamp_field: ingestion_timestamp_field.as_ref(),
                                            log_namespace,
                                        },
                                    );
                                }
                                watcher::Event::Init => {
                                    pods.clear();
                                }
                                watcher::Event::InitDone => {
                                    reconcile_active_streams(
                                        &pods,
                                        &container,
                                        &mut active,
                                        &mut initialized,
                                        StreamRuntime {
                                            client: &client,
                                            semaphore: semaphore.as_ref(),
                                            tx: &event_tx,
                                            pod_fields_spec: &pod_fields_spec,
                                            namespace_annotator: &namespace_annotator,
                                            node_annotator: &node_annotator,
                                            self_node_name: &self_node_name,
                                            tail_lines,
                                            since_seconds,
                                            ingestion_timestamp_field: ingestion_timestamp_field.as_ref(),
                                            log_namespace,
                                        },
                                    );
                                }
                            }
                        }
                        Some(Err(error)) => {
                            warn!(message = "Pod watcher received an error. Retrying.", ?error);
                        }
                        None => break,
                    }
                }
                _ = reconcile.tick() => {
                    reconcile_active_streams(
                        &pods,
                        &container,
                        &mut active,
                        &mut initialized,
                        StreamRuntime {
                            client: &client,
                            semaphore: semaphore.as_ref(),
                            tx: &event_tx,
                            pod_fields_spec: &pod_fields_spec,
                            namespace_annotator: &namespace_annotator,
                            node_annotator: &node_annotator,
                            self_node_name: &self_node_name,
                            tail_lines,
                            since_seconds,
                            ingestion_timestamp_field: ingestion_timestamp_field.as_ref(),
                            log_namespace,
                        },
                    );
                }
            }
        }

        for handle in active.into_values() {
            handle.abort();
        }
        for reflector in reflectors {
            reflector.abort();
        }
        Ok(())
    }
}
