use std::{collections::VecDeque, io, mem, task::Poll};

use futures::{future::BoxFuture, Future, FutureExt};
use tokio_test::task::{spawn, Spawn};

use super::{
    action::Action,
    common::{ReaderResult, TestReader, TestWriter, WriterResult},
    record::Record,
};

/// A wrapper that can track whether or not a future has already been polled.
///
/// In a true asynchronous runtime. a future that was polled once but returned pending would not be
/// polled again unless its waker was awoken.  We use `TrackedFuture` to know when we should poll a
/// future again as it can track both if it's been polled at all, as well as if it's been woken up,
/// allowing us to emulate driving it as a true asynchronous runtime would do.
struct TrackedFuture<T> {
    polled_once: bool,
    fut: Spawn<BoxFuture<'static, T>>,
}

impl<T> TrackedFuture<T> {
    fn from_future<F>(fut: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        Self {
            polled_once: false,
            fut: spawn(fut.boxed()),
        }
    }

    fn should_poll(&self) -> bool {
        !self.polled_once || self.fut.is_woken()
    }

    fn poll(&mut self) -> Poll<T> {
        self.polled_once = true;
        self.fut.poll()
    }
}

#[allow(clippy::large_enum_variant)]
enum ReadState {
    Inconsistent,
    Idle(TestReader),
    PendingRead(TrackedFuture<(TestReader, ReaderResult<Option<Record>>)>),
}

impl ReadState {
    fn is_idle(&self) -> bool {
        matches!(self, ReadState::Idle(_))
    }

    fn state_name(&self) -> &'static str {
        match self {
            ReadState::Inconsistent => "inconsistent",
            ReadState::Idle(_) => "idle",
            ReadState::PendingRead(_) => "pending_read",
        }
    }

    fn transition_to_read(&mut self) {
        let new_state = match mem::replace(self, ReadState::Inconsistent) {
            ReadState::Idle(mut reader) => {
                let fut = async move {
                    let result = reader.next().await;
                    (reader, result)
                };
                let tracked = TrackedFuture::from_future(fut);
                ReadState::PendingRead(tracked)
            }
            s => panic!(
                "tried to transition to pending read from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }
}

#[allow(clippy::large_enum_variant)]
enum WriteState {
    Inconsistent,
    Idle(TestWriter),
    PendingWrite(Record, TrackedFuture<(TestWriter, WriterResult<usize>)>),
    PendingFlush(TrackedFuture<(TestWriter, io::Result<()>)>),
    Closed,
}

impl WriteState {
    fn is_idle(&self) -> bool {
        matches!(self, WriteState::Idle(_))
    }

    fn is_closed(&self) -> bool {
        matches!(self, WriteState::Closed)
    }

    fn state_name(&self) -> &'static str {
        match self {
            WriteState::Inconsistent => "inconsistent",
            WriteState::Idle(_) => "idle",
            WriteState::PendingWrite(_, _) => "pending_write",
            WriteState::PendingFlush(_) => "pending_flush",
            WriteState::Closed => "closed",
        }
    }

    fn transition_to_write(&mut self, record: Record) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                let cloned_record = record.clone();
                let fut = async move {
                    let result = writer.write_record(record).await;
                    (writer, result)
                };
                let tracked = TrackedFuture::from_future(fut);
                WriteState::PendingWrite(cloned_record, tracked)
            }
            s => panic!(
                "tried to transition to pending write from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }

    fn transition_to_flush(&mut self) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                let fut = async move {
                    let result = writer.flush().await;
                    (writer, result)
                };
                let tracked = TrackedFuture::from_future(fut);
                WriteState::PendingFlush(tracked)
            }
            s => panic!(
                "tried to transition to pending flush from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }

    fn transition_to_closed(&mut self) {
        let new_state = match mem::replace(self, WriteState::Inconsistent) {
            WriteState::Idle(mut writer) => {
                // Technically, dropping the writer alone would also close the writer, logically,
                // but I'm doing it explicitly here for my own sanity when reading the code.
                writer.close();
                WriteState::Closed
            }
            // Already closed, nothing else to do.
            WriteState::Closed => WriteState::Closed,
            s => panic!(
                "tried to transition to closed from state other than idle: {}",
                s.state_name()
            ),
        };
        *self = new_state;
    }
}

/// Result of a read operation.
#[derive(Debug)]
pub enum ReadActionResult {
    Read(ReaderResult<Option<Record>>),
}

/// Result of a write operation.
#[derive(Debug)]
pub enum WriteActionResult {
    Write(WriterResult<usize>),
    Flush(io::Result<()>),
}

/// Action sequencer for the system under test.
///
/// As the system under test is asynchronous, some operations are expected to block on the
/// completion of other operations.  In order to model this in a step-wise fashion, the
/// `ActionSequencer` takes the actions generated by proptest and stages them in a valid order.
///
/// For example, based on the interface of the reader and writer of the SUT, mutable access is
/// required, so only one operation can be in-flight for either at a time.  If the writer is still
/// running a write operation, we cannot start another write operation, nor can we start another
/// read operation if the reader is still running a read operation.
///
/// Likewise, some operations can be run while writes and reads are in-flight, but they require
/// certain progress to have been made thus far, such as acknowledging a read, where acknowledgement
/// can happen at any time so long as there is at least one outstanding read that has not yet been acknowledged.
///
/// In this way, `ActionSequencer` enforces the real constraints of the SUT based on implicit
/// behavior codified through the type system as well as behavior that is part of the SUT usage
/// contract i.e. ack after read.
pub struct ActionSequencer {
    actions: Vec<Action>,
    read_state: ReadState,
    write_state: WriteState,
    unacked_events: VecDeque<Record>,
}

impl ActionSequencer {
    /// Creates a new `ActionSequencer` for the given SUT components.
    pub fn new(actions: Vec<Action>, reader: TestReader, writer: TestWriter) -> Self {
        Self {
            actions,
            read_state: ReadState::Idle(reader),
            write_state: WriteState::Idle(writer),
            unacked_events: VecDeque::default(),
        }
    }

    /// Whether or not there are any further write actions or any in-flight write operations.
    pub fn all_write_operations_finished(&self) -> bool {
        (self.write_state.is_idle() || self.write_state.is_closed())
            && self
                .actions
                .iter()
                .all(|a| !matches!(a, Action::WriteRecord(_) | Action::FlushWrites))
    }

    /// Transition the writer to the closed state.
    pub fn close_writer(&mut self) {
        self.write_state.transition_to_closed();
    }

    fn get_next_runnable_action(&self) -> Option<usize> {
        let allow_write = self.write_state.is_idle();
        let allow_read = self.read_state.is_idle();

        self.actions.iter().position(|a| match a {
            Action::WriteRecord(_) | Action::FlushWrites => allow_write,
            Action::ReadRecord => allow_read,
            Action::AcknowledgeRead => !self.unacked_events.is_empty(),
        })
    }

    /// Whether or not any runnable actions remain.
    pub fn has_remaining_runnable_actions(&self) -> bool {
        self.get_next_runnable_action().is_some()
    }

    /// Triggers the next runnable action.
    ///
    /// If an action is eligible to run, then it will be automatically run and the action itself
    /// will be returned to the caller so it may be applied against the model.  If none of the
    /// remaining actions are eligible to run, then `None` is returned.
    ///
    /// For example, if there's an in-flight write, we can't execute another write, or a flush.
    /// Likewise, we can't execute another read if there's an in-flight read.  Acknowledgements
    /// always happen out-of-band, though, and so are always eligible.
    pub fn trigger_next_runnable_action(&mut self) -> Option<Action> {
        let pos = self.get_next_runnable_action();

        if let Some(action) = pos.map(|i| self.actions.remove(i)) {
            match action {
                Action::WriteRecord(record) => {
                    assert!(
                        self.write_state.is_idle(),
                        "got write action when write state is not idle"
                    );

                    self.write_state.transition_to_write(record.clone());
                    Some(Action::WriteRecord(record))
                }
                a @ Action::FlushWrites => {
                    assert!(
                        self.write_state.is_idle(),
                        "got flush action when write state is not idle"
                    );

                    self.write_state.transition_to_flush();
                    Some(a)
                }
                a @ Action::ReadRecord => {
                    assert!(
                        self.read_state.is_idle(),
                        "got read action when read state is not idle"
                    );

                    self.read_state.transition_to_read();
                    Some(a)
                }
                Action::AcknowledgeRead => {
                    drop(self.unacked_events.pop_front().expect("FIXME"));
                    Some(Action::AcknowledgeRead)
                }
            }
        } else {
            None
        }
    }

    /// Gets the result of pending write action, if one is in-flight.
    ///
    /// If a write action (either a record write or a flush) is in-flight, we attempt to poll it to
    /// see if it is still pending or can successfully complete.  If it completes, information about
    /// the action, and operation, is passed back so that it can also be run through the model to
    /// check for consistency.
    pub fn get_pending_write_action(&mut self) -> Option<(Action, Poll<WriteActionResult>)> {
        let write_state = mem::replace(&mut self.write_state, WriteState::Inconsistent);
        let (new_write_state, result) = match write_state {
            // No in-flight write operation.
            s @ (WriteState::Idle(_) | WriteState::Closed) => (s, None),
            // We have an in-flight `write_record` call.
            WriteState::PendingWrite(record, mut fut) => {
                if fut.should_poll() {
                    match fut.poll() {
                        // No change yet.
                        Poll::Pending => (
                            WriteState::PendingWrite(record.clone(), fut),
                            Some((Action::WriteRecord(record), Poll::Pending)),
                        ),
                        // The `write_record` call completed.
                        Poll::Ready((writer, result)) => (
                            WriteState::Idle(writer),
                            Some((
                                Action::WriteRecord(record),
                                Poll::Ready(WriteActionResult::Write(result)),
                            )),
                        ),
                    }
                } else {
                    (
                        WriteState::PendingWrite(record.clone(), fut),
                        Some((Action::WriteRecord(record), Poll::Pending)),
                    )
                }
            }
            // We have an in-flight `flush` call.
            WriteState::PendingFlush(mut fut) => match fut.poll() {
                // No change yet.
                Poll::Pending => (
                    WriteState::PendingFlush(fut),
                    Some((Action::FlushWrites, Poll::Pending)),
                ),
                // The `flush` call completed.
                Poll::Ready((writer, result)) => (
                    WriteState::Idle(writer),
                    Some((
                        Action::FlushWrites,
                        Poll::Ready(WriteActionResult::Flush(result)),
                    )),
                ),
            },
            WriteState::Inconsistent => panic!("should never start from inconsistent write state"),
        };

        self.write_state = new_write_state;
        result
    }

    /// Gets the result of pending read action, if one is in-flight.
    ///
    /// If a read action is in-flight, we attempt to poll it to see if it is still pending or can
    /// successfully complete.  If it completes, information about the action, and operation, is
    /// passed back so that it can also be run through the model to check for consistency.
    pub fn get_pending_read_action(&mut self) -> Option<(Action, Poll<ReadActionResult>)> {
        let read_state = mem::replace(&mut self.read_state, ReadState::Inconsistent);
        let (new_read_state, result) = match read_state {
            // No in-flight read operation.
            s @ ReadState::Idle(_) => (s, None),
            // We have an in-flight `read` call.
            ReadState::PendingRead(mut fut) => {
                if fut.should_poll() {
                    match fut.poll() {
                        // No change yet.
                        Poll::Pending => (
                            ReadState::PendingRead(fut),
                            Some((Action::ReadRecord, Poll::Pending)),
                        ),
                        // The `read` call completed.
                        Poll::Ready((reader, result)) => {
                            // If a record was actually read back, track it as an unacknowledged read.
                            if let Ok(Some(record)) = &result {
                                self.unacked_events.push_back(record.clone());
                            }

                            (
                                ReadState::Idle(reader),
                                Some((
                                    Action::ReadRecord,
                                    Poll::Ready(ReadActionResult::Read(result)),
                                )),
                            )
                        }
                    }
                } else {
                    (
                        ReadState::PendingRead(fut),
                        Some((Action::ReadRecord, Poll::Pending)),
                    )
                }
            }
            ReadState::Inconsistent => panic!("should never start from inconsistent read state"),
        };

        self.read_state = new_read_state;
        result
    }
}
