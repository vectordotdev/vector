use crate::config::{DataType, TransformConfig, TransformContext, TransformDescription};
use crate::event::Event;
use crate::transforms::{TaskTransform, Transform};

use async_stream::stream;
use futures::{stream, Stream, StreamExt};
use governor::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::pin::Pin;
use std::time::Duration;

#[derive(Deserialize, Default, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct ThrottleConfig {
    events_per_second: u32,
    bytes_per_second: u32,
    key_field: Option<String>,
    //behavior: Behavior,
}

//#[derive(Clone, Debug)]
//enum Behavior {
//    ShedLoad,
//    BackPressure,
//}

inventory::submit! {
    TransformDescription::new::<ThrottleConfig>("throttle")
}

impl_generate_config_from_default!(ThrottleConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "throttle")]
impl TransformConfig for ThrottleConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Throttle::new(self).map(Transform::task)
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "throttle"
    }
}

#[derive(Clone, Debug)]
pub struct Throttle {
    events_per_second: NonZeroU32,
    bytes_per_second: NonZeroU32,
    key_field: Option<String>,
    //behavior: Behavior,
}

impl Throttle {
    pub fn new(config: &ThrottleConfig) -> crate::Result<Self> {
        Ok(Self {
            events_per_second: NonZeroU32::new(config.events_per_second).unwrap(),
            bytes_per_second: NonZeroU32::new(config.bytes_per_second).unwrap(),
            key_field: None,
            //behavior: Behavior::ShedLoad,
        })
    }
}

impl TaskTransform for Throttle {
    fn transform(
        self: Box<Self>,
        mut input_rx: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let lim = RateLimiter::direct(Quota::per_second(self.events_per_second));

        let mut flush_stream = tokio::time::interval(Duration::from_millis(1000));

        Box::pin(
            stream! {
              loop {
                let mut output = Vec::new();
                let done = tokio::select! {
                    _ = flush_stream.tick() => {
                        false
                    }
                    maybe_event = input_rx.next() => {
                        match maybe_event {
                            None => true,
                            Some(event) => {
                                match lim.check() {
                                    Ok(()) => {
                                        output.push(event);
                                        false
                                    }
                                    _ => {
                                        // Dropping event
                                        false
                                    }
                                }
                            }
                        }
                    }
                };
                yield stream::iter(output.into_iter());
                if done { break }
              }
            }
            .flatten(),
        )
    }
}
