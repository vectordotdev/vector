//! Configuration for AMQP Properties
use vector_config::configurable_component;
use lapin::{
    types::ShortString,
    BasicProperties,
};

/// AMQP Properties options.
#[configurable_component]
#[configurable(title = "Configure AMQP properties for the messages.")]
#[derive(Clone, Debug, Default)]
pub struct AmqpPropertiesConfig {
    /// Content-Type for the AMQP messages.
    #[configurable(derived)]
    pub(crate) content_type: Option<String>,

    /// Content-Encoding for the AMQP messages.
    #[configurable(derived)]
    pub(crate) content_encoding: Option<String>,
}

impl AmqpPropertiesConfig {
    pub fn build(&self) -> BasicProperties {
        let mut prop = BasicProperties::default();
        if let Some(content_type) = &self.content_type {
            prop = prop.with_content_type(ShortString::from(content_type.clone()));
        }
        if let Some(content_encoding) = &self.content_encoding {
            prop = prop.with_content_encoding(ShortString::from(content_encoding.clone()));
        }
        prop
    }
}
