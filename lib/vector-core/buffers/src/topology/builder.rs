use std::error::Error;

use async_trait::async_trait;
use snafu::{Snafu, ResultExt};
use tokio_stream::wrappers::ReceiverStream;
use tracing::Span;

use crate::buffer_usage_data::{BufferUsage, BufferUsageHandle};
use crate::topology::channel::{BufferReceiver, BufferSender};
use crate::topology::poll_sender::PollSender;
use crate::{WhenFull, Acker};

/// Value that can be used as a stage in a buffer topology.
#[async_trait]
pub trait IntoBuffer<T> {
    /// Converts this value into a sender and receiver pair suitable for use in a buffer topology.
    async fn into_buffer_parts(self: Box<Self>, usage_handle: &BufferUsageHandle) -> Result<(PollSender<T>, ReceiverStream<T>, Option<Acker>), Box<dyn Error + Send + Sync>>;
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
    #[snafu(display("last stage in topology cannot be set to overflow mode"))]
    OverflowWhenLast,
    #[snafu(display("failed to build individual stage {}: {}", stage_idx, source))]
    FailedToBuildStage { stage_idx: usize, source: Box<dyn Error + Send + Sync> },
    #[snafu(display("multiple components with segmented acknowledgements cannot be used in the same buffer"))]
    StackedAcks,
}

struct TopologyStage<T> {
    untransformed: Box<dyn IntoBuffer<T>>,
    when_full: WhenFull,
}

/// Builder for constructing buffer topologies.
pub struct TopologyBuilder<T> {
    stages: Vec<TopologyStage<T>>,
}

impl<T> TopologyBuilder<T> {
    /// Creates a new, empty [`TopologyBuilder`].
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
        }
    }

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
    pub async fn build(self, span: Span) -> Result<(BufferSender<T>, BufferReceiver<T>, Acker), TopologyError> {
        // We pop stages off in reverse order to build from the inside out.
        let mut buffer_usage = BufferUsage::from_span(span);
        let mut current_acker = None;
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
            // sender/receiver/acker.  This is slightly awkward since we just end up actually giving
            // the handle to the `BufferSender`/`BufferReceiver` wrappers, but that's the price we
            // have to pay for letting each stage function in an opaque way when wrapped.
            let usage_handle = buffer_usage.add_stage(stage_idx, stage.when_full);
            let (sender, receiver, acker) = stage.untransformed.into_buffer_parts(&usage_handle).await
                .context(FailedToBuildStage { stage_idx })?;

            // Multiple components with "segmented" acknowledgements cannot be supported at the
            // moment.  Segmented acknowledgements refers to stages which split the
            // acknowledgement of a single event into two parts.
            //
            // As an example, the an in-memory stage would simply pass through an acknowledgement, as the event
            // itself flows through untouched.  Other stages, like the disk stage, have to
            // acknowledge an event when it is written to disk, as the acknowledgement data cannot
            // be serialized to disk and rehydrated on deserialization.  However, the buffer still
            // supports acknowledgments on the read side so that sinks can tell the buffer when a
            // particular event in the buffer is safe to delete from disk, etc.
            //
            // In this way, the acknowledgements of an event for a disk buffer are "segmented".
            // Since we don't have the information to track which stage in a topology has emitted an
            // event to apply acknowledgements in the correct order, we don't support those
            // configurations.
            //
            // In the future, we may opt to support such a configuration.
            if current_acker.is_some() && acker.is_some() {
                return Err(TopologyError::StackedAcks);
            }
            current_acker = acker;

            let next_stage = match current_stage.take() {
                None => (
                    BufferSender::new(sender, stage.when_full),
                    BufferReceiver::new(receiver),
                ),
                Some((current_sender, current_receiver)) => (
                    BufferSender::with_overflow(sender, current_sender),
                    BufferReceiver::with_overflow(receiver, current_receiver),
                ),
            };

            current_stage = Some(next_stage);
        }

        let (sender, receiver) = current_stage.ok_or(TopologyError::EmptyTopology)?;
        let acker = current_acker.unwrap_or_else(|| Acker::Null);

        // Install the buffer usage handler since we successfully created the buffer topology.  This
        // spawns it in the background and periodically emits aggregated metrics about each of the
        // buffer stages.
        buffer_usage.install();

        Ok((sender, receiver, acker))
    }
}

#[cfg(test)]
mod tests {
    use tracing::Span;

    use crate::{
        topology::{builder::TopologyError, test_util::assert_current_send_capacity},
        MemoryBuffer, WhenFull,
    };

    use super::TopologyBuilder;

    #[tokio::test]
    async fn single_stage_topology_block() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::Block);
        let result = builder.build(Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, None);
    }

    #[tokio::test]
    async fn single_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::DropNewest);
        let result = builder.build(Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, None);
    }

    #[tokio::test]
    async fn single_stage_topology_overflow() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::Overflow);
        let result = builder.build(Span::none()).await;
        match result {
            Err(TopologyError::OverflowWhenLast) => {},
            r => panic!("unexpected build result: {:?}", r),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_block() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::Block);
        builder.stage(MemoryBuffer::new(1), WhenFull::Block);
        let result = builder.build(Span::none()).await;
        match result {
            Err(TopologyError::NextStageNotUsed { stage_idx }) => assert_eq!(stage_idx, 0),
            r => panic!("unexpected build result: {:?}", r),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::DropNewest);
        builder.stage(MemoryBuffer::new(1), WhenFull::Block);
        let result = builder.build(Span::none()).await;
        match result {
            Err(TopologyError::NextStageNotUsed { stage_idx }) => assert_eq!(stage_idx, 0),
            r => panic!("unexpected build result: {:?}", r),
        }
    }

    #[tokio::test]
    async fn two_stage_topology_overflow() {
        let mut builder = TopologyBuilder::<u64>::new();
        builder.stage(MemoryBuffer::new(1), WhenFull::Overflow);
        builder.stage(MemoryBuffer::new(1), WhenFull::Block);

        let result = builder.build(Span::none()).await;
        assert!(result.is_ok());

        let (mut sender, _, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, Some(1));
    }

    #[tokio::test]
    async fn assert_span_passed_to_buffer_usage_correctly() {
        todo!();
    }
}
