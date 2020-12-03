//! Kubernetes metadata transform.
//!
//! Implements a highly configurable transform for annotating events from
//! the Kubernetes API state.

#![deny(missing_docs)]

use crate::kubernetes as k8s;
use crate::{
    config::{DataType, TransformConfig, TransformDescription},
    event::Event,
    transforms::{self, FunctionTransform},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{convert::Infallible, future::Future};

mod watch_request_builder;

/// Configuration for the `kubernetes_metadata` transform.
#[derive(Default, Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    watch_request_builder: watch_request_builder::Config,
    label_selector: Option<String>,
    field_selector: Option<String>,
}

const COMPONENT_NAME: &str = "kubernetes_metadata";

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_metadata")]
impl TransformConfig for Config {
    async fn build(&self) -> crate::Result<transforms::Transform> {
        let runtime = Runtime::new(&self)?;
        let (runtime_loop, transform) = runtime.run().await?;
        tokio::spawn(runtime_loop);
        Ok(transforms::Transform::function(transform))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        COMPONENT_NAME
    }
}

inventory::submit! {
    TransformDescription::new::<Config>(COMPONENT_NAME)
}

impl_generate_config_from_default!(Config);

struct Runtime {
    client: k8s::client::Client,
    watch_request_builder: watch_request_builder::Builder,
    label_selector: Option<String>,
    field_selector: Option<String>,
}

impl Runtime {
    fn new(config: &Config) -> crate::Result<Self> {
        let k8s_config = k8s::client::config::Config::in_cluster()?;
        let client = k8s::client::Client::new(k8s_config)?;
        let watch_request_builder = (&config.watch_request_builder).into();
        Ok(Self {
            client,
            watch_request_builder,
            field_selector: config.label_selector.clone(),
            label_selector: config.field_selector.clone(),
        })
    }

    async fn run(
        self,
    ) -> crate::Result<(
        impl Future<Output = Result<Infallible, crate::Error>>,
        Transform,
    )> {
        let Self {
            client,
            watch_request_builder,
            field_selector,
            label_selector,
        } = self;

        let watcher = k8s::api_watcher::ApiWatcher::new(client, watch_request_builder);
        let watcher = k8s::instrumenting_watcher::InstrumentingWatcher::new(watcher);
        let (_state_reader, state_writer) = evmap::new();
        let state_writer =
            k8s::state::evmap::Writer::new(state_writer, Some(Duration::from_millis(10)));
        let state_writer = k8s::state::instrumenting::Writer::new(state_writer);
        let state_writer =
            k8s::state::delayed_delete::Writer::new(state_writer, Duration::from_secs(60));

        let mut reflector = k8s::reflector::Reflector::new(
            watcher,
            state_writer,
            field_selector,
            label_selector,
            Duration::from_secs(1),
        );

        let exposed_future = async move { reflector.run().await.map_err(|err| err.into()) };
        let transform = Transform {};
        Ok((exposed_future, transform))
    }
}

#[derive(Clone)]
struct Transform {}

impl FunctionTransform for Transform {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let log = event.into_log();
        info!("{:?}", log);
        output.push(Event::Log(log));
    }
}
