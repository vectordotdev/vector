use std::{convert::TryFrom, str::FromStr};

use crate::config::{
    DataType, GenerateConfig, Resource, SourceConfig, SourceContext, SourceDescription,
};
use async_tungstenite::{tokio::connect_async, tungstenite::Message};
use futures::prelude::*;
use futures::{future, pin_mut, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{error, trace};
use vector_core::event::{Event, LogEvent};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct WebSocketConfig {
    pub url: String,
    pub topic: String,
    // #[serde(flatten, default)]
    // pub decoding: DecodingConfig,
}
inventory::submit! {
    SourceDescription::new::<WebSocketConfig>("ws")
}

impl GenerateConfig for WebSocketConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            url: "0.0.0.0:8080".to_owned(),
            topic: json!({
                "subscribe": "",
                "chain_id": ""
            })
            .to_string(),
            // decoding: DecodingConfig::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "ws")]
impl SourceConfig for WebSocketConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<super::Source> {
        let url = self.url.clone();
        let topic = self.topic.clone();
        let out = cx.out;
        let shutdown_shared = cx.shutdown.shared().clone();

        let (mut ws_stream, _) = connect_async(&url).await?;
        trace!(message = "WebSocket handshake has been successfully completed.");

        let text = Message::Text(topic);

        ws_stream.send(text).await?;
        let (_write, read) = ws_stream.split();

        let r = read.filter_map(|s| future::ready(s.ok()));
        let r = r.filter_map(|s| async move {
            match s {
                Message::Text(json_str) => {
                    let json = serde_json::Value::from_str(&json_str.clone());
                    if let Ok(json_value) = json {
                        let log_evt = LogEvent::try_from(json_value);
                        if let Ok(evt) = log_evt {
                            Some(Ok(Event::Log(evt)))
                        } else {
                            error!(message = "Unhandled log");
                            None
                        }
                    } else {
                        debug!(message = "Unhandled json");
                        None
                    }
                }
                _ => {
                    debug!(message = "Unhandled: ", %s);
                    None
                }
            }
        });

        let out = out.sink_map_err(|error| error!(message = "Error sending metric.", %error));
        let r = r.boxed();
        let s = r.forward(out);

        Ok(Box::pin(async move {
            pin_mut!(s);
            future::select(shutdown_shared, s).await;
            Ok(())
        }))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn source_type(&self) -> &'static str {
        "ws"
    }

    fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}
