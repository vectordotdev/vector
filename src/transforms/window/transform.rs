use std::collections::VecDeque;

use vector_lib::internal_event::{Count, InternalEventHandle as _, Registered};

use crate::{
    conditions::Condition,
    event::Event,
    internal_events::WindowEventsDropped,
    transforms::{FunctionTransform, OutputBuffer},
};

#[derive(Clone)]
pub struct Window {
    // Configuration parameters
    forward_when: Option<Condition>,
    flush_when: Condition,
    num_events_before: usize,
    num_events_after: usize,

    // Internal variables
    buffer: VecDeque<Event>,
    events_counter: usize,
    events_dropped: Registered<WindowEventsDropped>,
    is_flushing: bool,
}

impl Window {
    pub fn new(
        forward_when: Option<Condition>,
        flush_when: Condition,
        num_events_before: usize,
        num_events_after: usize,
    ) -> crate::Result<Self> {
        let buffer = VecDeque::with_capacity(num_events_before);

        Ok(Window {
            forward_when,
            flush_when,
            num_events_before,
            num_events_after,
            events_dropped: register!(WindowEventsDropped),
            buffer,
            events_counter: 0,
            is_flushing: false,
        })
    }
}

impl FunctionTransform for Window {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        let (pass, event) = match self.forward_when.as_ref() {
            Some(condition) => {
                let (result, event) = condition.check(event);
                (result, event)
            }
            _ => (false, event),
        };

        let (flush, event) = self.flush_when.check(event);

        if self.buffer.capacity() < self.num_events_before {
            self.buffer.reserve(self.num_events_before);
        }

        if pass {
            output.push(event);
        } else if flush {
            if self.num_events_before > 0 {
                self.buffer.drain(..).for_each(|evt| output.push(evt));
            }

            self.events_counter = 0;
            self.is_flushing = true;
            output.push(event);
        } else if self.is_flushing {
            self.events_counter += 1;

            if self.events_counter > self.num_events_after {
                self.events_counter = 0;
                self.is_flushing = false;
                self.events_dropped.emit(Count(1));
            } else {
                output.push(event);
            }
        } else if self.buffer.len() >= self.num_events_before {
            self.buffer.pop_front();
            self.buffer.push_back(event);
            self.events_dropped.emit(Count(1));
        } else if self.num_events_before > 0 {
            self.buffer.push_back(event);
        } else {
            self.events_dropped.emit(Count(1));
        }
    }
}

#[cfg(test)]
mod test {
    use std::ops::RangeInclusive;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio_stream::wrappers::ReceiverStream;
    use vrl::core::Value;

    use crate::conditions::{AnyCondition, ConditionConfig, VrlConfig};
    use crate::transforms::window::config::WindowConfig;
    use crate::{
        event::{Event, LogEvent},
        test_util::components::assert_transform_compliance,
        transforms::test::create_topology,
    };

    #[tokio::test]
    async fn test_flush() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 1, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_event(&tx, "flush").await;
            assert_event("flush", out.recv().await).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_pass() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let forward_when = get_condition("forward");
            let transform_config = get_transform_config(flush_when, Some(forward_when), 1, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_event(&tx, "forward").await;
            assert_event("forward", out.recv().await).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_10_in_50() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 50, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=10)).await;
            send_event(&tx, "flush").await;

            let mut expected: [&str; 11] = [
                "A01", "A02", "A03", "A04", "A05", "A06", "A07", "A08", "A09", "A10", "flush",
            ];

            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_50_in_10() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 10, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=50)).await;
            send_event(&tx, "flush").await;

            let mut expected: [&str; 11] = [
                "A41", "A42", "A43", "A44", "A45", "A46", "A47", "A48", "A49", "A50", "flush",
            ];

            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_before_and_after() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 10, 5);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=50)).await;
            send_event(&tx, "flush").await;
            send_events(&tx, generate_events(51..=70)).await;

            let mut expected: [&str; 16] = [
                "A41", "A42", "A43", "A44", "A45", "A46", "A47", "A48", "A49", "A50", "flush",
                "A51", "A52", "A53", "A54", "A55",
            ];

            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_flush_and_pass() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let forward_when = get_condition("forward");
            let transform_config = get_transform_config(flush_when, Some(forward_when), 50, 5);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=5)).await;
            send_event(&tx, "forward").await;
            send_events(&tx, generate_events(6..=10)).await;
            send_event(&tx, "forward").await;
            send_event(&tx, "flush").await;
            send_event(&tx, "forward").await;
            send_events(&tx, generate_events(11..=15)).await;
            send_event(&tx, "forward").await;
            send_events(&tx, generate_events(16..=20)).await;

            let mut expected: [&str; 20] = [
                "forward", "forward", "A01", "A02", "A03", "A04", "A05", "A06", "A07", "A08",
                "A09", "A10", "flush", "forward", "A11", "A12", "A13", "A14", "A15", "forward",
            ];

            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_zero_before() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 0, 5);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=50)).await;
            send_event(&tx, "flush").await;
            send_events(&tx, generate_events(51..=70)).await;

            let mut expected: [&str; 6] = ["flush", "A51", "A52", "A53", "A54", "A55"];
            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_zero_flush() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let transform_config = get_transform_config(flush_when, None, 0, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            send_events(&tx, generate_events(1..=50)).await;
            send_event(&tx, "flush").await;
            send_events(&tx, generate_events(51..=70)).await;

            let mut expected: [&str; 1] = ["flush"];
            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    #[tokio::test]
    async fn test_zero_pass() {
        assert_transform_compliance(async {
            let flush_when = get_condition("flush");
            let forward_when = get_condition("forward");
            let transform_config = get_transform_config(flush_when, Some(forward_when), 0, 0);

            let (tx, rx) = mpsc::channel(1);
            let (topology, mut out) =
                create_topology(ReceiverStream::new(rx), transform_config).await;

            let events = generate_events(1..=50);
            let more_events = generate_events(51..=70);

            send_events(&tx, events).await;
            send_event(&tx, "forward").await;
            send_event(&tx, "flush").await;
            send_events(&tx, more_events).await;

            let mut expected: [&str; 2] = ["forward", "flush"];
            assert_events(&mut expected, &mut out).await;

            drop(tx);
            topology.stop().await;

            assert_eq!(out.recv().await, None);
        })
        .await;
    }

    const fn get_transform_config(
        flush_when: AnyCondition,
        forward_when: Option<AnyCondition>,
        num_events_before: usize,
        num_events_after: usize,
    ) -> WindowConfig {
        WindowConfig {
            flush_when,
            forward_when,
            num_events_before,
            num_events_after,
        }
    }

    fn get_condition(message: &str) -> AnyCondition {
        AnyCondition::from(ConditionConfig::Vrl(VrlConfig {
            source: format!(r#".message == "{message}""#),
            runtime: Default::default(),
        }))
    }

    fn generate_events(range: RangeInclusive<i32>) -> Vec<Event> {
        range
            .map(|n| format!("A{n:02}"))
            .map(|m| Event::from(LogEvent::from(m)))
            .collect::<Vec<Event>>()
    }

    async fn send_events(tx: &Sender<Event>, events: Vec<Event>) {
        for event in events {
            tx.send(event).await.unwrap();
        }
    }

    async fn send_event(tx: &Sender<Event>, message: &str) {
        tx.send(Event::from(LogEvent::from(message))).await.unwrap();
    }

    async fn assert_event(message: &str, event: Option<Event>) {
        assert_eq!(
            &Value::from(message),
            event.unwrap().as_log().get("message").unwrap()
        );
    }

    async fn assert_events(messages: &mut [&str], out: &mut Receiver<Event>) {
        for message in messages {
            assert_event(message, out.recv().await).await;
        }
    }
}
