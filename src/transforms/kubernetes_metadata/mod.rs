use crate::kubernetes as k8s;
use crate::{
    config::{DataType, TransformConfig, TransformDescription},
    event::Event,
    transforms,
    transforms::FunctionTransform,
};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize, Serialize, Debug, Clone)]
pub struct Config {}

const COMPONENT_NAME: &str = "kubernetes_metadata";

#[async_trait::async_trait]
#[typetag::serde(name = "kubernetes_metadata")]
impl TransformConfig for Config {
    async fn build(&self) -> crate::Result<transforms::Transform> {
        let transform = Transform::new(&self)?;
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

#[derive(Clone)]
struct Transform {
    client: k8s::client::Client,
}

impl Transform {
    fn new(_config: &Config) -> crate::Result<Self> {
        let k8s_config = k8s::client::config::Config::in_cluster()?;
        let client = k8s::client::Client::new(k8s_config)?;
        Ok(Self { client })
    }
}

impl FunctionTransform for Transform {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let log = event.into_log();
        info!("{:?}", log);
        output.push(Event::Log(log));
    }
}
