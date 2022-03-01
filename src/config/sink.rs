use async_trait::async_trait;
use component::ComponentDescription;
use serde::{Deserialize, Serialize};
use vector_buffers::{Acker, BufferConfig, BufferType};
use vector_core::config::{AcknowledgementsConfig, GlobalOptions, Input};

use super::{component, ComponentKey, ProxyConfig, Resource};
use crate::{
    serde::bool_or_struct,
    sinks::{self, util::UriSerde},
};

#[derive(Deserialize, Serialize, Debug)]
pub struct SinkOuter<T> {
    #[serde(default = "Default::default")] // https://github.com/serde-rs/serde/issues/1541
    pub inputs: Vec<T>,
    // We are accepting this option for backward compatibility.
    healthcheck_uri: Option<UriSerde>,

    // We are accepting bool for backward compatibility.
    #[serde(deserialize_with = "crate::serde::bool_or_struct")]
    #[serde(default)]
    healthcheck: SinkHealthcheckOptions,

    #[serde(default)]
    pub buffer: BufferConfig,

    #[serde(
        default,
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    proxy: ProxyConfig,

    #[serde(flatten)]
    pub inner: Box<dyn SinkConfig>,

    #[serde(
        default,
        deserialize_with = "bool_or_struct",
        skip_serializing_if = "vector_core::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
}

impl<T> SinkOuter<T> {
    pub fn new(inputs: Vec<T>, inner: Box<dyn SinkConfig>) -> SinkOuter<T> {
        SinkOuter {
            inputs,
            buffer: Default::default(),
            healthcheck: SinkHealthcheckOptions::default(),
            healthcheck_uri: None,
            inner,
            proxy: Default::default(),
            acknowledgements: Default::default(),
        }
    }

    pub fn resources(&self, id: &ComponentKey) -> Vec<Resource> {
        let mut resources = self.inner.resources();
        for stage in self.buffer.stages() {
            match stage {
                BufferType::Memory { .. } => {}
                BufferType::DiskV1 { .. } | BufferType::DiskV2 { .. } => {
                    resources.push(Resource::DiskBuffer(id.to_string()))
                }
            }
        }
        resources
    }

    pub fn healthcheck(&self) -> SinkHealthcheckOptions {
        if self.healthcheck_uri.is_some() && self.healthcheck.uri.is_some() {
            warn!("Both `healthcheck.uri` and `healthcheck_uri` options are specified. Using value of `healthcheck.uri`.")
        } else if self.healthcheck_uri.is_some() {
            warn!(
                "The `healthcheck_uri` option has been deprecated, use `healthcheck.uri` instead."
            )
        }
        SinkHealthcheckOptions {
            uri: self
                .healthcheck
                .uri
                .clone()
                .or_else(|| self.healthcheck_uri.clone()),
            ..self.healthcheck.clone()
        }
    }

    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }

    pub(super) fn map_inputs<U>(self, f: impl Fn(&T) -> U) -> SinkOuter<U> {
        let inputs = self.inputs.iter().map(f).collect();
        self.with_inputs(inputs)
    }

    pub(super) fn with_inputs<U>(self, inputs: Vec<U>) -> SinkOuter<U> {
        SinkOuter {
            inputs,
            inner: self.inner,
            buffer: self.buffer,
            healthcheck: self.healthcheck,
            healthcheck_uri: self.healthcheck_uri,
            proxy: self.proxy,
            acknowledgements: self.acknowledgements,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(default)]
pub struct SinkHealthcheckOptions {
    pub enabled: bool,
    pub uri: Option<UriSerde>,
}

impl Default for SinkHealthcheckOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            uri: None,
        }
    }
}

impl From<bool> for SinkHealthcheckOptions {
    fn from(enabled: bool) -> Self {
        Self { enabled, uri: None }
    }
}

impl From<UriSerde> for SinkHealthcheckOptions {
    fn from(uri: UriSerde) -> Self {
        Self {
            enabled: true,
            uri: Some(uri),
        }
    }
}

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug + Send + Sync {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(sinks::VectorSink, sinks::Healthcheck)>;

    fn input(&self) -> Input;

    fn sink_type(&self) -> &'static str;

    /// Resources that the sink is using.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    fn can_acknowledge(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct SinkContext {
    pub acker: Acker,
    pub healthcheck: SinkHealthcheckOptions,
    pub globals: GlobalOptions,
    pub proxy: ProxyConfig,
}

impl SinkContext {
    #[cfg(test)]
    pub fn new_test() -> Self {
        Self {
            acker: Acker::passthrough(),
            healthcheck: SinkHealthcheckOptions::default(),
            globals: GlobalOptions::default(),
            proxy: ProxyConfig::default(),
        }
    }

    pub fn acker(&self) -> Acker {
        self.acker.clone()
    }

    pub const fn globals(&self) -> &GlobalOptions {
        &self.globals
    }

    pub const fn proxy(&self) -> &ProxyConfig {
        &self.proxy
    }
}

pub type SinkDescription = ComponentDescription<Box<dyn SinkConfig>>;

inventory::collect!(SinkDescription);
