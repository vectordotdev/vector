use std::{
    collections::HashMap,
    error,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use vector_common::{
    EventDataEq,
    byte_size_of::ByteSizeOf,
    internal_event::{
        self, CountByteSize, DEFAULT_OUTPUT, EventsSent, InternalEventHandle as _, Registered,
        register,
    },
    json_size::JsonSize,
};

use crate::{
    config,
    config::{ComponentKey, OutputId},
    event::{EstimatedJsonEncodedSizeOf, Event, EventArray, EventContainer, EventMutRef, EventRef},
    fanout::{self, Fanout},
    schema,
};

struct TransformOutput {
    fanout: Fanout,
    control_channel_open: bool,
    events_sent: Registered<EventsSent>,
    log_schema_definitions: HashMap<OutputId, Arc<schema::Definition>>,
    output_id: Arc<OutputId>,
}

pub struct TransformOutputs {
    outputs_spec: Vec<config::TransformOutput>,
    primary_output: Option<TransformOutput>,
    named_outputs: HashMap<String, TransformOutput>,
}

impl TransformOutputs {
    pub fn new(
        outputs_in: Vec<config::TransformOutput>,
        component_key: &ComponentKey,
    ) -> (Self, HashMap<Option<String>, fanout::ControlChannel>) {
        let outputs_spec = outputs_in.clone();
        let mut primary_output = None;
        let mut named_outputs = HashMap::new();
        let mut controls = HashMap::new();

        for output in outputs_in {
            let (fanout, control) = Fanout::new(component_key.clone());

            let log_schema_definitions = output
                .log_schema_definitions
                .into_iter()
                .map(|(id, definition)| (id, Arc::new(definition)))
                .collect();

            match output.port {
                None => {
                    primary_output = Some(TransformOutput {
                        fanout,
                        control_channel_open: true,
                        events_sent: register(EventsSent::from(internal_event::Output(Some(
                            DEFAULT_OUTPUT.into(),
                        )))),
                        log_schema_definitions,
                        output_id: Arc::new(OutputId {
                            component: component_key.clone(),
                            port: None,
                        }),
                    });
                    controls.insert(None, control);
                }
                Some(name) => {
                    named_outputs.insert(
                        name.clone(),
                        TransformOutput {
                            fanout,
                            control_channel_open: true,
                            events_sent: register(EventsSent::from(internal_event::Output(Some(
                                name.clone().into(),
                            )))),
                            log_schema_definitions,
                            output_id: Arc::new(OutputId {
                                component: component_key.clone(),
                                port: Some(name.clone()),
                            }),
                        },
                    );
                    controls.insert(Some(name.clone()), control);
                }
            }
        }

        let me = Self {
            outputs_spec,
            primary_output,
            named_outputs,
        };

        (me, controls)
    }

    /// Waits for the next control message on any output and applies it.
    ///
    /// Lets an idle transform apply reload control messages without waiting for its next
    /// event, so the senders of removed or changed downstream components are released.
    /// Returns `true` once a message was applied or an output's control channel closed, and
    /// `false` if every output's control channel is already closed.
    pub async fn recv_control_message(&mut self) -> bool {
        std::future::poll_fn(|cx| self.poll_control_message(cx)).await
    }

    fn poll_control_message(&mut self, cx: &mut Context<'_>) -> Poll<bool> {
        let mut any_open = false;
        let mut progressed = false;
        // Poll every open output, so each one registers for a wakeup when we return `Pending`.
        for output in self
            .primary_output
            .iter_mut()
            .chain(self.named_outputs.values_mut())
            .filter(|output| output.control_channel_open)
        {
            any_open = true;
            if let Poll::Ready(alive) = output.fanout.poll_control_message(cx) {
                output.control_channel_open = alive;
                progressed = true;
            }
        }
        if progressed || !any_open {
            Poll::Ready(any_open)
        } else {
            Poll::Pending
        }
    }

    pub fn new_buf_with_capacity(&self, capacity: usize) -> TransformOutputsBuf {
        TransformOutputsBuf::new_with_capacity(self.outputs_spec.clone(), capacity)
    }

    /// Sends the events in the buffer to their respective outputs.
    ///
    /// # Errors
    ///
    /// If an error occurs while sending events to their respective output, an error variant will be
    /// returned detailing the cause.
    pub async fn send(
        &mut self,
        buf: &mut TransformOutputsBuf,
    ) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        if let Some(primary) = self.primary_output.as_mut() {
            let Some(buf) = buf.primary_buffer.as_mut() else {
                unreachable!("mismatched outputs");
            };
            Self::send_single_buffer(buf, primary).await?;
        }
        for (key, buf) in &mut buf.named_buffers {
            let Some(output) = self.named_outputs.get_mut(key) else {
                unreachable!("unknown output");
            };
            Self::send_single_buffer(buf, output).await?;
        }
        Ok(())
    }

    async fn send_single_buffer(
        buf: &mut OutputBuffer,
        output: &mut TransformOutput,
    ) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        for event in buf.events_mut() {
            super::update_runtime_schema_definition(
                event,
                &output.output_id,
                &output.log_schema_definitions,
            );
        }
        let count = buf.len();
        let byte_size = buf.estimated_json_encoded_size_of();
        buf.send(&mut output.fanout).await?;
        output.events_sent.emit(CountByteSize(count, byte_size));
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TransformOutputsBuf {
    pub(super) primary_buffer: Option<OutputBuffer>,
    pub(super) named_buffers: HashMap<String, OutputBuffer>,
}

impl TransformOutputsBuf {
    pub fn new_with_capacity(outputs_in: Vec<config::TransformOutput>, capacity: usize) -> Self {
        let mut primary_buffer = None;
        let mut named_buffers = HashMap::new();

        for output in outputs_in {
            match output.port {
                None => {
                    primary_buffer = Some(OutputBuffer::with_capacity(capacity));
                }
                Some(name) => {
                    named_buffers.insert(name.clone(), OutputBuffer::default());
                }
            }
        }

        Self {
            primary_buffer,
            named_buffers,
        }
    }

    /// Adds a new event to the named output buffer.
    ///
    /// # Panics
    ///
    /// Panics if there is no output with the given name.
    pub fn push(&mut self, name: Option<&str>, event: Event) {
        match name {
            Some(name) => self.named_buffers.get_mut(name),
            None => self.primary_buffer.as_mut(),
        }
        .expect("unknown output")
        .push(event);
    }

    /// Drains the default output buffer.
    ///
    /// # Panics
    ///
    /// Panics if there is no default output.
    pub fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.primary_buffer
            .as_mut()
            .expect("no default output")
            .drain()
    }

    /// Drains the named output buffer.
    ///
    /// # Panics
    ///
    /// Panics if there is no output with the given name.
    pub fn drain_named(&mut self, name: &str) -> impl Iterator<Item = Event> + '_ {
        self.named_buffers
            .get_mut(name)
            .expect("unknown output")
            .drain()
    }

    /// Takes the default output buffer.
    ///
    /// # Panics
    ///
    /// Panics if there is no default output.
    pub fn take_primary(&mut self) -> OutputBuffer {
        std::mem::take(self.primary_buffer.as_mut().expect("no default output"))
    }

    pub fn take_all_named(&mut self) -> HashMap<String, OutputBuffer> {
        std::mem::take(&mut self.named_buffers)
    }

    /// Applies `f` to each [`EventArray`] currently buffered in this outputs buffer.
    ///
    /// This is useful for cross-cutting instrumentation (e.g. latency timestamp propagation)
    /// that needs mutable access to the buffered arrays before they are sent.
    pub fn for_each_array_mut(&mut self, mut f: impl FnMut(&mut EventArray)) {
        if let Some(primary) = self.primary_buffer.as_mut() {
            primary.for_each_array_mut(&mut f);
        }
        for buf in self.named_buffers.values_mut() {
            buf.for_each_array_mut(&mut f);
        }
    }
}

impl ByteSizeOf for TransformOutputsBuf {
    fn allocated_bytes(&self) -> usize {
        self.primary_buffer.size_of()
            + self
                .named_buffers
                .values()
                .map(ByteSizeOf::size_of)
                .sum::<usize>()
    }
}

#[derive(Debug, Default, Clone)]
pub struct OutputBuffer(pub(super) Vec<EventArray>);

impl OutputBuffer {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, event: Event) {
        // Coalesce multiple pushes of the same type into one array.
        match (event, self.0.last_mut()) {
            (Event::Log(log), Some(EventArray::Logs(logs))) => {
                logs.push(log);
            }
            (Event::Metric(metric), Some(EventArray::Metrics(metrics))) => {
                metrics.push(metric);
            }
            (Event::Trace(trace), Some(EventArray::Traces(traces))) => {
                traces.push(trace);
            }
            (event, _) => {
                self.0.push(event.into());
            }
        }
    }

    pub fn append(&mut self, events: &mut Vec<Event>) {
        for event in events.drain(..) {
            self.push(event);
        }
    }

    pub fn extend(&mut self, events: impl Iterator<Item = Event>) {
        for event in events {
            self.push(event);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(EventArray::len).sum()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn first(&self) -> Option<EventRef<'_>> {
        self.0.first().and_then(|first| match first {
            EventArray::Logs(l) => l.first().map(Into::into),
            EventArray::Metrics(m) => m.first().map(Into::into),
            EventArray::Traces(t) => t.first().map(Into::into),
        })
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.0.drain(..).flat_map(EventArray::into_events)
    }

    /// Applies `f` to each [`EventArray`] currently held by this buffer.
    pub fn for_each_array_mut(&mut self, mut f: impl FnMut(&mut EventArray)) {
        for array in &mut self.0 {
            f(array);
        }
    }

    async fn send(
        &mut self,
        output: &mut Fanout,
    ) -> Result<(), Box<dyn error::Error + Send + Sync>> {
        let send_start = Some(Instant::now());
        for array in std::mem::take(&mut self.0) {
            output.send(array, send_start).await?;
        }

        Ok(())
    }

    fn iter_events(&self) -> impl Iterator<Item = EventRef<'_>> {
        self.0.iter().flat_map(EventArray::iter_events)
    }

    fn events_mut(&mut self) -> impl Iterator<Item = EventMutRef<'_>> {
        self.0.iter_mut().flat_map(EventArray::iter_events_mut)
    }

    pub fn into_events(self) -> impl Iterator<Item = Event> {
        self.0.into_iter().flat_map(EventArray::into_events)
    }
}

impl ByteSizeOf for OutputBuffer {
    fn allocated_bytes(&self) -> usize {
        self.0.iter().map(ByteSizeOf::size_of).sum()
    }
}

impl EventDataEq<Vec<Event>> for OutputBuffer {
    fn event_data_eq(&self, other: &Vec<Event>) -> bool {
        struct Comparator<'a>(EventRef<'a>);

        impl PartialEq<&Event> for Comparator<'_> {
            fn eq(&self, that: &&Event) -> bool {
                self.0.event_data_eq(that)
            }
        }

        self.iter_events().map(Comparator).eq(other.iter())
    }
}

impl EstimatedJsonEncodedSizeOf for OutputBuffer {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.0
            .iter()
            .map(EstimatedJsonEncodedSizeOf::estimated_json_encoded_size_of)
            .sum()
    }
}

impl From<Vec<Event>> for OutputBuffer {
    fn from(events: Vec<Event>) -> Self {
        let mut result = Self::default();
        result.extend(events.into_iter());
        result
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, num::NonZeroUsize};

    use futures::StreamExt;
    use tokio_test::{assert_pending, assert_ready, task::spawn};
    use tracing::Span;
    use vector_buffers::{
        WhenFull,
        topology::{
            builder::TopologyBuilder,
            channel::{BufferReceiver, BufferSender},
        },
    };

    use super::TransformOutputs;
    use crate::{
        config::{self, ComponentKey, DataType},
        event::EventArray,
        fanout::{ControlChannel, ControlMessage},
    };

    const NAMED: &str = "named";

    fn build_sender_pair() -> (BufferSender<EventArray>, BufferReceiver<EventArray>) {
        TopologyBuilder::standalone_memory(
            NonZeroUsize::new(2).expect("capacity must be nonzero"),
            WhenFull::Block,
            &Span::current(),
            None,
            None,
        )
    }

    fn sink_key() -> ComponentKey {
        ComponentKey::from("sink")
    }

    /// Builds outputs with a primary and a named output, each connected to one sink.
    async fn outputs_with_sinks() -> (
        TransformOutputs,
        HashMap<Option<String>, ControlChannel>,
        HashMap<Option<String>, BufferReceiver<EventArray>>,
    ) {
        let (mut outputs, controls) = TransformOutputs::new(
            vec![
                config::TransformOutput::new(DataType::all_bits(), HashMap::new()),
                config::TransformOutput::new(DataType::all_bits(), HashMap::new()).with_port(NAMED),
            ],
            &ComponentKey::from("transform"),
        );
        let mut receivers = HashMap::new();
        for (port, control) in &controls {
            let (sender, receiver) = build_sender_pair();
            control
                .send(ControlMessage::Add(sink_key(), sender))
                .expect("control channel should be open");
            assert!(outputs.recv_control_message().await);
            receivers.insert(port.clone(), receiver);
        }
        (outputs, controls, receivers)
    }

    fn assert_closed(receiver: BufferReceiver<EventArray>) {
        let mut stream = receiver.into_stream();
        let mut next = spawn(stream.next());
        assert!(assert_ready!(next.poll()).is_none(), "no events were sent");
    }

    #[tokio::test]
    async fn recv_control_message_applies_to_named_output_while_idle() {
        let (mut outputs, controls, mut receivers) = outputs_with_sinks().await;
        let named = Some(NAMED.to_string());

        let mut recv = spawn(outputs.recv_control_message());
        assert_pending!(recv.poll());
        controls[&named]
            .send(ControlMessage::Remove(sink_key()))
            .expect("control channel should be open");
        assert!(recv.is_woken());
        assert!(assert_ready!(recv.poll()));
        drop(recv);

        assert_closed(receivers.remove(&named).expect("named receiver"));
    }

    #[tokio::test]
    async fn recv_control_message_skips_closed_control_channels() {
        let (mut outputs, mut controls, mut receivers) = outputs_with_sinks().await;

        // Closing the named output's channel is reported once, then that output is skipped.
        drop(controls.remove(&Some(NAMED.to_string())));
        assert!(outputs.recv_control_message().await);

        let mut recv = spawn(outputs.recv_control_message());
        assert_pending!(recv.poll());
        controls[&None]
            .send(ControlMessage::Remove(sink_key()))
            .expect("control channel should be open");
        assert!(recv.is_woken());
        assert!(assert_ready!(recv.poll()));
        drop(recv);
        assert_closed(receivers.remove(&None).expect("primary receiver"));

        drop(controls);
        assert!(outputs.recv_control_message().await);
        assert!(!outputs.recv_control_message().await);
    }
}
