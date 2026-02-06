use std::{collections::HashMap, error, sync::Arc, time::Instant};

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
            let (fanout, control) = Fanout::new();

            let log_schema_definitions = output
                .log_schema_definitions
                .into_iter()
                .map(|(id, definition)| (id, Arc::new(definition)))
                .collect();

            match output.port {
                None => {
                    primary_output = Some(TransformOutput {
                        fanout,
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
