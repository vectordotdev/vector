use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};

use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    transforms::{FunctionTransform, Transform},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, warn};
use vector_core::event::LogEvent;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
struct JqConfig {
    query: String,
}

inventory::submit! {
    TransformDescription::new::<JqConfig>("jq")
}

impl GenerateConfig for JqConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            query: ".".to_owned(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "jq")]
impl TransformConfig for JqConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Jq::new(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "jq"
    }
}

#[derive(Debug, Clone)]
struct Jq {
    query: String,
}

impl Jq {
    pub fn new(config: JqConfig) -> Self {
        Self {
            query: config.query.clone(),
        }
    }
}

impl FunctionTransform for Jq {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let l = event.as_log().clone();
        let v: Result<serde_json::Value, crate::Error> = l.try_into();
        if let Ok(l) = v {
            let lstr = l.to_string();
            let o = jq_rs::run(self.query.as_str(), lstr.as_str());
            if let Ok(out_str) = o {
                let json = serde_json::Value::from_str(out_str.as_str());
                if let Ok(json_value) = json {
                    match json_value {
                        serde_json::Value::Array(ref v) => {
                            v.iter().for_each(|ref e| {
                                let log_evt = LogEvent::try_from((*e).clone());
                                if let Ok(evt) = log_evt {
                                    output.push(Event::Log(evt));
                                } else {
                                    let error = log_evt.unwrap_err();
                                    error!(message = "Unhandled log \t", %json_value, %error);
                                }
                            });
                        }
                        serde_json::Value::Object(_) => {
                            let log_evt = LogEvent::try_from(json_value);
                            if let Ok(evt) = log_evt {
                                output.push(Event::Log(evt));
                            } else {
                                let error = log_evt.unwrap_err();
                                error!(message = "Unhandled log \t", %error);
                            }
                        }
                        serde_json::Value::Null => (),
                        _ => {
                            error!(message = "Unsupport json type \t", %json_value);
                        }
                    }
                } else {
                    debug!(message = "Unhandled json \t", %out_str);
                }
            } else {
                let error = o.unwrap_err();
                warn!(message = "error running query: \t", %error);
                // output.push(event)
            }
        } else {
            let error = v.unwrap_err();
            warn!(message = "error decoding event: \t", %error);
        }
    }
}
