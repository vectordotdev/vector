use std::{error::Error, num::NonZeroUsize};

use async_trait::async_trait;
use snafu::{ResultExt, Snafu};
use tracing::Span;

use super::channel::{ReceiverAdapter, SenderAdapter};
use crate::{
    buffer_usage_data::{BufferUsage, BufferUsageHandle},
    topology::channel::{BufferReceiver, BufferSender},
    variants::MemoryBuffer,
    Bufferable, WhenFull,
};

/// Value that can be used as a stage in a buffer topology.
#[async_trait]
pub trait IntoBuffer<T: Bufferable>: Send {
    /// Gets whether or not this buffer stage provides its own instrumentation, or if it should be
    /// instrumented from the outside.
    ///
    /// As some buffer stages, like the in-memory channel, never have a chance to catch the values
    /// in the middle of the channel without introducing an unnecessary hop, [`BufferSender`] and
    /// [`BufferReceiver`] can be configured to instrument all events flowing through directly.
    ///
    /// When instrumentation is provided in this way, [`vector_common::byte_size_of::ByteSizeOf`]
    ///  is used to calculate the size of the event going both into and out of the buffer.
    fn provides_instrumentation(&self) -> bool {
        false
    }

    /// Converts this value into a sender and receiver pair suitable for use in a buffer topology.
    async fn into_buffer_parts(
        self: Box<Self>,
        usage_handle: BufferUsageHandle,
    ) -> Result<(SenderAdapter<T>, ReceiverAdapter<T>), Box<dyn Error + Send + Sync>>;
}

#[derive(Debug, Snafu)]
pub enum TopologyError {
    #[snafu(display("buffer topology cannot be empty"))]
    EmptyTopology,
    #[snafu(display(
        "stage {} configured with block/drop newest behavior in front of subsequent stage",
        stage_idx
    ))]
    NextStageNotUsed { stage_idx: usize },
    #[snafu(display("last stage in buffer topology cannot be set to overflow mode"))]
    OverflowWhenLast,
    #[snafu(display("failed to build individual stage {}: {}", stage_idx, source))]
    FailedToBuildStage {
        stage_idx: usize,
        source: Box<dyn Error + Send + Sync>,
    },
    #[snafu(display(
        "multiple components with segmented acknowledgements cannot be used in the same buffer"
    ))]
    StackedAcks,
}

struct TopologyStage<T: Bufferable> {
    untransformed: Box<dyn IntoBuffer<T>>,
    when_full: WhenFull,
}

/// Builder for constructing buffer topologies.
pub struct TopologyBuilder<T: Bufferable> {
    stages: Vec<TopologyStage<T>>,
}

impl<T: Bufferable> TopologyBuilder<T> {
    /// Adds a new stage to the buffer topology.
    ///
    /// The "when full" behavior can be optionally configured here.  If no behavior is specified,
    /// and an overflow buffer is _not_ added to the topology after this, then the "when full"
    /// behavior will use a default value of "block".  If a "when full" behavior is specified, and
    /// an overflow buffer is added to the topology after this, then the specified "when full"
    /// behavior will be ignored and will be set to "overflow" mode.
    ///
    /// Callers can configure what to do when a buffer is full by setting `when_full`.  Three modes
    /// are available -- block, drop newest, and overflow -- which are documented in more detail by
    /// [`BufferSender`].
    ///
    /// Two notes about what modes are not valid in certain scenarios:
    /// - the innermost stage (the last stage given to the builder) cannot be set to "overflow" mode,
    ///   as there is no other stage to overflow to
    /// - a stage cannot use the "block" or "drop newest" mode when there is a subsequent stage, and
    ///   must user the "overflow" mode
    ///
    /// Any occurrence of either of these scenarios will result in an error during build.
    pub fn stage<S>(&mut self, stage: S, when_full: WhenFull) -> &mut Self
    where
        S: IntoBuffer<T> + 'static,
    {
        self.stages.push(TopologyStage {
            untransformed: Box::new(stage),
            when_full,
        });
        self
    }

    /// Consumes this builder, returning the sender and receiver that can be used by components.
    ///
    /// # Errors
    ///
    /// If there was a configuration error with one of the stages, an error variant will be returned
    /// explaining the issue.
    pub async fn build(
        self,
        buffer_id: String,
        span: Span,
    ) -> Result<(BufferSender<T>, BufferReceiver<T>), TopologyError> {
        // We pop stages off in reverse order to build from the inside out.
        let mut buffer_usage = BufferUsage::from_span(span.clone());
        let mut current_stage = None;

        for (stage_idx, stage) in self.stages.into_iter().enumerate().rev() {
            // Make sure the stage is valid for our current builder state.
            match stage.when_full {
                // The innermost stage can't be set to overflow, there's nothing else to overflow _to_.
                WhenFull::Overflow => {
                    if current_stage.is_none() {
                        return Err(TopologyError::OverflowWhenLast);
                    }
                }
                // If there's already an inner stage, then blocking or dropping the newest events
                // doesn't no sense.  Overflowing is the only valid transition to another stage.
                WhenFull::Block | WhenFull::DropNewest => {
                    if current_stage.is_some() {
                        return Err(TopologyError::NextStageNotUsed { stage_idx });
                    }
                }
            };

            // Create the buffer usage handle for this stage and initialize it as we create the
            // sender/receiver.  This is slightly awkward since we just end up actually giving
            // the handle to the `BufferSender`/`BufferReceiver` wrappers, but that's the price we
            // have to pay for letting each stage function in an opaque way when wrapped.
            let usage_handle = buffer_usage.add_stage(stage_idx);
            let provides_instrumentation = stage.untransformed.provides_instrumentation();
            let (sender, receiver) = stage
                .untransformed
                .into_buffer_parts(usage_handle.clone())
                .await
                .context(FailedToBuildStageSnafu { stage_idx })?;

            let (mut sender, mut receiver) = match current_stage.take() {
                None => (
                    BufferSender::new(sender, stage.when_full),
                    BufferReceiver::new(receiver),
                ),
                Some((current_sender, current_receiver)) => (
                    BufferSender::with_overflow(sender, current_sender),
                    BufferReceiver::with_overflow(receiver, current_receiver),
                ),
            };

            sender.with_send_duration_instrumentation(stage_idx, &span);
            if !provides_instrumentation {
                sender.with_usage_instrumentation(usage_handle.clone());
                receiver.with_usage_instrumentation(usage_handle);
            }

            current_stage = Some((sender, receiver));
        }

        let (sender, receiver) = current_stage.ok_or(TopologyError::EmptyTopology)?;

        // Install the buffer usage handler since we successfully created the buffer topology.  This
        // spawns it in the background and periodically emits aggregated metrics about each of the
        // buffer stages.
        buffer_usage.install(buffer_id.as_str());

        Ok((sender, receiver))
    }
}

impl<T: Bufferable> TopologyBuilder<T> {
    /// Creates a memory-only buffer topology.
    ///
    /// The overflow mode (i.e. `WhenFull`) can be configured to either block or drop the newest
    /// values, but cannot be configured to use overflow mode.  If overflow mode is selected, it
    /// will be changed to blocking mode.
    ///
    /// This is a convenience method for `vector` as it is used for inter-transform channels, and we
    /// can simplifying needing to require callers to do all the boilerplate to create the builder,
    /// create the stage, installing buffer usage metrics that aren't required, and so on.
    ///
    #[allow(clippy::print_stderr)]
    pub async fn standalone_memory(
        max_events: NonZeroUsize,
        when_full: WhenFull,
        receiver_span: &Span,
    ) -> (BufferSender<T>, BufferReceiver<T>) {
        let usage_handle = BufferUsageHandle::noop();

        let memory_buffer = Box::new(MemoryBuffer::new(max_events));
        let (sender, receiver) = memory_buffer
            .into_buffer_parts(usage_handle.clone())
            .await
            .unwrap_or_else(|_| unreachable!("should not fail to directly create a memory buffer"));

        let mode = match when_full {
            WhenFull::Overflow => WhenFull::Block,
            m => m,
        };
        let mut sender = BufferSender::new(sender, mode);
        sender.with_send_duration_instrumentation(0, receiver_span);
        let receiver = BufferReceiver::new(receiver);

        (sender, receiver)
    }

    /// Creates a memory-only buffer topology with the given buffer usage handle.
    ///
    /// This is specifically required for the tests that occur under `buffers`, as we assert things
    /// like channel capacity left, which cannot be done on in-memory v1 buffers as they use the
    /// more abstract `Sink`-based adapters.
    ///
    /// The overflow mode (i.e. `WhenFull`) can be configured to either block or drop the newest
    /// values, but cannot be configured to use overflow mode.  If overflow mode is selected, it
    /// will be changed to blocking mode.
    ///
    /// This is a convenience method for `vector` as it is used for inter-transform channels, and we
    /// can simplifying needing to require callers to do all the boilerplate to create the builder,
    /// create the stage, installing buffer usage metrics that aren't required, and so on.
    #[cfg(test)]
    pub async fn standalone_memory_test(
        max_events: NonZeroUsize,
        when_full: WhenFull,
        usage_handle: BufferUsageHandle,
    ) -> (BufferSender<T>, BufferReceiver<T>) {
        let memory_buffer = Box::new(MemoryBuffer::new(max_events));
        let (sender, receiver) = memory_buffer
            .into_buffer_parts(usage_handle.clone())
            .await
            .unwrap_or_else(|_| unreachable!("should not fail to directly create a memory buffer"));

        let mode = match when_full {
            WhenFull::Overflow => WhenFull::Block,
            m => m,
        };
        let mut sender = BufferSender::new(sender, mode);
        let mut receiver = BufferReceiver::new(receiver);

        sender.with_usage_instrumentation(usage_handle.clone());
        receiver.with_usage_instrumentation(usage_handle);

        (sender, receiver)
    }
}

impl<T: Bufferable> Default for TopologyBuilder<T> {
    fn default() -> Self {
        Self { stages: Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use tracing::Span;

    use super::TopologyBuilder;
    use crate::{
        topology::builder::TopologyError,
        topology::test_util::{assert_current_send_capacity, Sample},
        variants::MemoryBuffer,
        WhenFull,
    };

    #[tokio::test]
    async fn single_stage_topology_block() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Block,
        );
        let result = builder.build(String::from("test"), Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, Some(1), None);
    }

    #[tokio::test]
    async fn single_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::DropNewest,
        );
        let result = builder.build(String::from("test"), Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, Some(1), None);
    }

    #[tokio::test]
    async fn single_stage_topology_overflow() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Overflow,
        );
        let result = builder.build(String::from("test"), Span::none()).await;
        match result {
            Err(TopologyError::OverflowWhenLast) => {}
            r => panic!("unexpected build result: {r:?}"),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_block() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Block,
        );
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Block,
        );
        let result = builder.build(String::from("test"), Span::none()).await;
        match result {
            Err(TopologyError::NextStageNotUsed { stage_idx }) => assert_eq!(stage_idx, 0),
            r => panic!("unexpected build result: {r:?}"),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::DropNewest,
        );
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Block,
        );
        let result = builder.build(String::from("test"), Span::none()).await;
        match result {
            Err(TopologyError::NextStageNotUsed { stage_idx }) => assert_eq!(stage_idx, 0),
            r => panic!("unexpected build result: {r:?}"),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_overflow() {
        let mut builder = TopologyBuilder::<Sample>::default();
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Overflow,
        );
        builder.stage(
            MemoryBuffer::new(NonZeroUsize::new(1).unwrap()),
            WhenFull::Block,
        );

        let result = builder.build(String::from("test"), Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, Some(1), Some(1));
    }
}
