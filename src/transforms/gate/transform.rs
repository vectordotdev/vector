use std::collections::VecDeque;

use vector_lib::internal_event::{Count, InternalEventHandle as _, Registered};

use crate::{
    conditions::Condition,
    event::Event,
    internal_events::GateEventsDropped,
    transforms::{FunctionTransform, OutputBuffer},
    transforms::gate::state::GateState,
};

#[derive(Clone)]
pub struct Gate {
    // Configuration parameters
    pass_when: Option<Condition>,
    open_when: Option<Condition>,
    close_when: Option<Condition>,
    max_events: usize,
    auto_close: bool,
    tail_events: usize,

    // Internal variables
    current_state: GateState,
    buffer: VecDeque<Event>,
    events_counter: usize,
    events_dropped: Registered<GateEventsDropped>,
    is_closing: bool,
}

impl Gate {
    // This function is dead code when the feature flag `transforms-impl-gate` is specified but not
    // `transforms-gate`.
    #![allow(dead_code)]
    pub fn new(
        pass_when: Option<Condition>,
        open_when: Option<Condition>,
        close_when: Option<Condition>,
        max_events: usize,
        auto_close: bool,
        tail_events: usize,
    ) -> crate::Result<Self> {
        let buffer = VecDeque::with_capacity(max_events);

        Ok(Gate {
            pass_when,
            open_when,
            close_when,
            max_events,
            auto_close,
            tail_events,
            events_dropped: register!(GateEventsDropped),
            current_state: GateState::Closed,
            buffer,
            events_counter: 0,
            is_closing: false,
        })
    }
}

impl FunctionTransform for Gate {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let (pass_gate, event) = match self.pass_when.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (result, event)
            }
            _ => (false, event)
        };

        let (open_gate, event) = match self.open_when.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (result, event)
            }
            _ => (false, event)
        };

        let (close_gate, event) = match self.close_when.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (result, event)
            }
            _ => (false, event)
        };

        if self.buffer.capacity() < self.max_events {
            self.buffer.reserve(self.max_events);
        }

        if self.buffer.len() >= self.max_events {
            self.buffer.pop_front();
        }

        self.buffer.push_back(event);

        if pass_gate {
            self.buffer.pop_back().map(|evt| output.push(evt));
        } else if open_gate {
            self.current_state = GateState::Open;
            self.buffer.drain(..).for_each(|evt| output.push(evt));
            self.events_counter = 0;

            if self.auto_close {
                self.is_closing = true;
            }
        } else if close_gate {
            self.buffer.pop_back().map(|evt| output.push(evt));
            self.is_closing = true;
        } else if self.current_state == GateState::Open {
            self.buffer.pop_back().map(|evt| output.push(evt));
        } else {
            self.events_dropped.emit(Count(1));
        }

        if self.is_closing {
            self.events_counter += 1;

            if self.events_counter > self.tail_events {
                self.current_state = GateState::Closed;
                self.events_counter = 0;
                self.is_closing = false;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::core::Value;

    use crate::{
        event::{Event, LogEvent},
        test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };
    use crate::conditions::{ConditionConfig, VrlConfig};
    use crate::transforms::gate::config::GateConfig;

    use super::*;

    #[tokio::test]
    async fn gate_manual_close() {
        assert_transform_compliance(async {
            let open_config = VrlConfig {
                source: String::from(r#".message == "open""#),
                runtime: Default::default(),
            };

            let close_config = VrlConfig {
                source: String::from(r#".message == "close""#),
                runtime: Default::default(),
            };

            let pass_config = VrlConfig {
                source: String::from(r#".message == "hello""#),
                runtime: Default::default(),
            };

            let gate_config = GateConfig {
                auto_close: Some(false),
                open_when: Some(AnyCondition::from(ConditionConfig::Vrl(open_config))),
                close_when: Some(AnyCondition::from(ConditionConfig::Vrl(close_config))),
                pass_when: Some(AnyCondition::from(ConditionConfig::Vrl(pass_config))),
                max_events: Some(3),
                tail_events: Some(3),
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), gate_config).await;

            let messages = [
                "drop1", "drop2", "pass1", "hello", "pass2", "open", "pass3", "pass4",
                "pass5", "close", "pass6", "pass7", "pass8", "drop3", "drop4", "drop5"
            ];

            let events = messages.map(|msg| Event::from(LogEvent::from(msg)));

            for evt in events {
                tx.send(evt.clone()).await.unwrap();
            }

            assert_message("hello", out.recv().await).await;
            assert_message("pass1", out.recv().await).await;
            assert_message("pass2", out.recv().await).await;
            assert_message("open", out.recv().await).await;
            assert_message("pass3", out.recv().await).await;
            assert_message("pass4", out.recv().await).await;
            assert_message("pass5", out.recv().await).await;
            assert_message("close", out.recv().await).await;
            assert_message("pass6", out.recv().await).await;
            assert_message("pass7", out.recv().await).await;
            assert_message("pass8", out.recv().await).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        }).await;
    }

    #[tokio::test]
    async fn gate_auto_close() {
        assert_transform_compliance(async {
            let open_config = VrlConfig {
                source: String::from(r#".message == "open""#),
                runtime: Default::default(),
            };

            let close_config = VrlConfig {
                source: String::from(r#".message == "close""#),
                runtime: Default::default(),
            };

            let pass_config = VrlConfig {
                source: String::from(r#".message == "hello""#),
                runtime: Default::default(),
            };

            let gate_config = GateConfig {
                auto_close: Some(true),
                open_when: Some(AnyCondition::from(ConditionConfig::Vrl(open_config))),
                close_when: Some(AnyCondition::from(ConditionConfig::Vrl(close_config))),
                pass_when: Some(AnyCondition::from(ConditionConfig::Vrl(pass_config))),
                max_events: Some(3),
                tail_events: Some(3),
            };

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), gate_config).await;

            let messages = [
                "drop1", "drop2", "pass1", "hello", "pass2", "open", "pass3", "pass4",
                "pass5", "drop3", "drop4", "drop5"
            ];

            let events = messages.map(|msg| Event::from(LogEvent::from(msg)));

            for evt in events {
                tx.send(evt.clone()).await.unwrap();
            }

            assert_message("hello", out.recv().await).await;
            assert_message("pass1", out.recv().await).await;
            assert_message("pass2", out.recv().await).await;
            assert_message("open", out.recv().await).await;
            assert_message("pass3", out.recv().await).await;
            assert_message("pass4", out.recv().await).await;
            assert_message("pass5", out.recv().await).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        }).await;
    }

    async fn assert_message(message: &str, event: Option<Event>) {
        assert_eq!(
            &Value::from(message),
            event.unwrap().as_log().get("message").unwrap()
        );
    }
}
