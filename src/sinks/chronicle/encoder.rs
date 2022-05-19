use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    event::Event,
    internal_events,
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::HttpEventEncoder,
        PartitionInnerBuffer,
    },
    template::{Template, TemplateRenderingError},
};

// TODO Can this come from somewhere else?
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

pub(super) type PartitionKey = String;

pub(super) struct ChronicleSinkEventEncoder {
    pub(super) field: Template,
    pub(super) encoding: EncodingConfigWithDefault<Encoding>,
}

impl ChronicleSinkEventEncoder {
    fn render_key(
        &self,
        event: &Event,
    ) -> Result<PartitionKey, (Option<&str>, TemplateRenderingError)> {
        let field = self
            .field
            .render_string(event)
            .map_err(|e| (Some("field"), e))?;
        Ok(field)
    }
}

impl HttpEventEncoder<PartitionInnerBuffer<Value, PartitionKey>> for ChronicleSinkEventEncoder {
    fn encode_event(
        &mut self,
        mut event: Event,
    ) -> Option<PartitionInnerBuffer<Value, PartitionKey>> {
        let key = self
            .render_key(&event)
            .map_err(|(field, error)| {
                emit!(internal_events::TemplateRenderingError {
                    error,
                    field,
                    drop_event: true,
                });
            })
            .ok()?;

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();
        let json: Option<serde_json::Value> = log.try_into().ok();
        json.map(|log| PartitionInnerBuffer::new(log, key))
    }
}
