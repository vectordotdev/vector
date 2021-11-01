use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::PollSender;

use crate::WhenFull;

use super::channel::{BufferReceiver, BufferSender};

/// Value that can be used as a stage in a buffer topology.
pub trait IntoBuffer<T> {
	/// Converts this value into a sender and receiver pair suitable for use in a buffer topology.
	fn into_buffer_parts(self) -> (PollSender<T>, ReceiverStream<T>);
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
	/// One specific note is that using the "overflow" mode has no effect if this stage is the inner
	/// most.  In that case, the behavior will default to "block".  It is also invalid to specify a
	/// mode other than "overflow" when the topology already has configured stages.
	/// 
	/// Errors related to misconfiguration of the "when full" behavior are deferred until building
	/// the entire topology.
	pub fn stage<S>(&mut self, stage: S, when_full: WhenFull) -> &mut Self
	where
		S: IntoBuffer<T>,
	{
		let (sender, receiver) = stage.into_buffer_parts();
		self.stages.push(TopologyStage { sender, receiver, when_full });
		self
	}

	/// Consumes this builder, returning the sender and receiver that can be used by components.
	/// 
	/// # Errors
	/// If there was a configuration error with one of the stages, an error variant will be returned
	/// explaining the issue.
	pub fn build(self) -> Result<(BufferSender<T>, BufferReceiver<T>), String> {
		// We pop stages off in reverse order to build from the inside out.
		let mut current_stage = None;

		for stage in self.stages {
			let when_full = match stage.when_full {
				// If there's no inner stage already, then set the "when full" mode to "block".
				WhenFull::Overflow => match current_stage {
					// TODO: should this actually raise an error instead?  might better surface the behavior
					None => WhenFull::Block,
					Some(_) => WhenFull::Overflow,
				},
				// If there's already an inner stage, then not overflowing to it is a configuration error.
				w @ WhenFull::Block | w @ WhenFull::DropNewest => if current_stage.is_some() {
					return Err("invalid to specify block/drop newest behavior in front of another topology stage".into())
				} else {
					w
				}
			};

			let next_stage = match current_stage.take() {
				None => {
					(
						BufferSender::new(stage.sender, when_full),
						BufferReceiver::new(stage.receiver)
					)
				},
				Some((current_sender, current_receiver)) => {
					(
						BufferSender::with_overflow(stage.sender, current_sender),
						BufferReceiver::with_overflow(stage.receiver, current_receiver),
					)
				},
			};

			current_stage = Some(next_stage);
		}

		current_stage.ok_or("no stage was defined".into())
	}
}
