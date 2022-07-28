use async_trait::async_trait;
use futures::future::BoxFuture;

#[async_trait]
#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug + Send + Sync {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> vector_core::Result<(vector_core::sink::VectorSink, Healthcheck)>;

    fn input(&self) -> crate::core::config::Input;

    fn sink_type(&self) -> &'static str;

    /// Resources that the sink is using.
    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }

    fn acknowledgements(&self) -> Option<&crate::core::config::AcknowledgementsConfig>;
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Resource {
    SystemFdOffset(usize),
    Stdin,
    DiskBuffer(String),
}

pub type Healthcheck = BoxFuture<'static, vector_core::Result<()>>;

#[derive(Debug, Clone)]
pub struct SinkContext {
    // pub healthcheck: SinkHealthcheckOptions,
    pub globals: crate::core::config::GlobalOptions,
    pub proxy: crate::core::config::proxy::ProxyConfig,
    // pub schema: crate::core::schema::Options,
}

impl SinkContext {
    pub const fn globals(&self) -> &crate::core::config::GlobalOptions {
        &self.globals
    }

    pub const fn proxy(&self) -> &crate::core::config::proxy::ProxyConfig {
        &self.proxy
    }
}
