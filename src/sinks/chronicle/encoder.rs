use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    event::Event,
    internal_events::TemplateRenderingError,
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::HttpEventEncoder,
        PartitionInnerBuffer,
    },
    template::Template,
};

// TODO Can this come from somewhere else?
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

type PartitionKey = String;

pub(super) struct ChronicleSinkEventEncoder {
    field: Template,
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
    fn encode_event(&mut self, mut event: Event) -> Option<PartitionInnerBuffer<Value, PartitionKey>> {
        let key = self
            .render_key(&event)
            .map_err(|(field, error)| {
                emit!(crate::internal_events::TemplateRenderingError {
                    error,
                    field,
                    drop_event: true,
                });
            })
            .ok()?;

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();

        let mut map = serde_json::map::Map::new();

        map.insert("");

        log.try_into().ok()
    }
}
