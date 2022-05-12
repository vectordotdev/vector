use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    event::Event,
    sinks::util::{
        encoding::{EncodingConfigWithDefault, EncodingConfiguration},
        http::HttpEventEncoder,
    },
};

// TODO Can this come from somewhere else?
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

pub(super) struct ChronicleSinkEventEncoder {
    pub(super) encoding: EncodingConfigWithDefault<Encoding>,
}

impl HttpEventEncoder<Value> for ChronicleSinkEventEncoder {
    fn encode_event(&mut self, mut event: Event) -> Option<Value> {
        self.encoding.apply_rules(&mut event);
        let log = event.into_log();

        log.try_into().ok()
    }
}
