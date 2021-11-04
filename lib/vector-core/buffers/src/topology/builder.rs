use snafu::Snafu;
use tokio_stream::wrappers::ReceiverStream;

use crate::topology::channel::{BufferReceiver, BufferSender};
use crate::topology::poll_sender::PollSender;
use crate::WhenFull;
/// Value that can be used as a stage in a buffer topology.
pub trait IntoBuffer<T> {
    /// Converts this value into a sender and receiver pair suitable for use in a buffer topology.
    fn into_buffer_parts(self) -> (PollSender<T>, ReceiverStream<T>);
}

#[derive(Debug, Eq, PartialEq, Snafu)]
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
}

struct TopologyStage<T> {
    sender: PollSender<T>,
    receiver: ReceiverStream<T>,
    when_full: WhenFull,
}

/// Builder for constructing buffer topologies.
#[derive(Default)]
pub struct TopologyBuilder<T> {
    stages: Vec<TopologyStage<T>>,
}

impl<T> TopologyBuilder<T> {
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
    /// Any occurence of either of these scenarios will result in an error during build.
    pub fn stage<S>(&mut self, stage: S, when_full: WhenFull) -> &mut Self
    where
        S: IntoBuffer<T>,
    {
        let (sender, receiver) = stage.into_buffer_parts();
        self.stages.push(TopologyStage {
            sender,
            receiver,
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
    pub fn build(self) -> Result<(BufferSender<T>, BufferReceiver<T>), TopologyError> {
        // We pop stages off in reverse order to build from the inside out.
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

            let next_stage = match current_stage.take() {
                None => (
                    BufferSender::new(stage.sender, stage.when_full),
                    BufferReceiver::new(stage.receiver),
                ),
                Some((current_sender, current_receiver)) => (
                    BufferSender::with_overflow(stage.sender, current_sender),
                    BufferReceiver::with_overflow(stage.receiver, current_receiver),
                ),
            };

            current_stage = Some(next_stage);
        }

        current_stage.ok_or(TopologyError::EmptyTopology)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        topology::{
            builder::TopologyError,
            test_util::{assert_current_send_capacity, PassthroughChannel},
        },
        WhenFull,
    };

    use super::TopologyBuilder;

    #[test]
    fn single_stage_topology_block() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::Block);
        let result = builder.build();
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, None);
    }

    #[test]
    fn single_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::DropNewest);
        let result = builder.build();
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, None);
    }

    #[test]
    fn single_stage_topology_overflow() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::Overflow);
        assert_eq!(
            builder.build().unwrap_err(),
            TopologyError::OverflowWhenLast
        );
    }

    #[test]
    fn two_stage_topology_block() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::Block);
        builder.stage(PassthroughChannel::new(1), WhenFull::Block);
        assert_eq!(
            builder.build().unwrap_err(),
            TopologyError::NextStageNotUsed { stage_idx: 0 }
        );
    }

    #[test]
    fn two_stage_topology_drop_newest() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::DropNewest);
        builder.stage(PassthroughChannel::new(1), WhenFull::Block);
        assert_eq!(
            builder.build().unwrap_err(),
            TopologyError::NextStageNotUsed { stage_idx: 0 }
        );
    }

    #[test]
    fn two_stage_topology_overflow() {
        let mut builder = TopologyBuilder::default();
        builder.stage(PassthroughChannel::new(1), WhenFull::Overflow);
        builder.stage(PassthroughChannel::new(1), WhenFull::Block);

        let result = builder.build();
        assert!(result.is_ok());

        let (mut sender, _) = result.unwrap();
        assert_current_send_capacity(&mut sender, 1, Some(1));
    }
}
