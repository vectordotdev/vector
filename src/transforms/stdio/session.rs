use futures::{SinkExt, Stream, StreamExt, future, stream};
use std::ops::ControlFlow;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::codec::FramedWrite;
use vector_lib::{
    codecs::{decoding, encoding},
    event::Event,
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
};

use crate::{
    codecs::Encoder,
    transforms::stdio::{
        exec::{ExecTask, StdStream},
        process::{Process, Spawner},
    },
};

pub(super) type DecodeResult = Result<(smallvec::SmallVec<[Event; 1]>, usize), decoding::Error>;
pub(super) type OutputStreamItem = (DecodeResult, StdStream);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionOutcome {
    ChildExited,
    InputExhausted,
    DownstreamClosed,
}

pub(super) struct ChildSession<P: Process, O> {
    pub process: P,

    // Sending side of the stdin writer
    pub stdin_tx: Option<mpsc::Sender<Event>>,

    // Handle to the background writer task
    pub stdin_handle: Option<JoinHandle<()>>,

    pub output_stream: O,

    /// True if the input stream returned None (EOF).
    pub input_exhausted: bool,
}

impl<P, O> ChildSession<P, O>
where
    P: Process + 'static,
    O: Stream<Item = OutputStreamItem> + Send + Unpin + 'static,
{
    // Allow for double buffering to reduce context-switching overhead between
    // the main loop and the writer task.
    const STDIN_CHANNEL_CAPACITY: usize = 2;

    pub fn new(
        process: P,
        stdin_writer: FramedWrite<P::Stdin, Encoder<encoding::Framer>>,
        output_stream: O,
    ) -> Self {
        let (stdin_tx, stdin_rx) = mpsc::channel(Self::STDIN_CHANNEL_CAPACITY);
        let stdin_handle = tokio::spawn(Self::stdin_writer_task(stdin_writer, stdin_rx));

        Self {
            process,
            stdin_tx: Some(stdin_tx),
            stdin_handle: Some(stdin_handle),
            output_stream,
            input_exhausted: false,
        }
    }

    pub async fn stdin_writer_task(
        mut writer: FramedWrite<P::Stdin, Encoder<encoding::Framer>>,
        mut rx: mpsc::Receiver<Event>,
    ) {
        while let Some(event) = rx.recv().await {
            let Err(error) = writer.send(event).await else {
                // Successfully sent event, continue
                continue;
            };

            // Close channel to prevent upstream from buffering more items
            rx.close();

            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                count: 1,
                reason: &format!("Failed to write to child stdin: {error}"),
            });

            // Drain remaining items to report accurate drop counts
            let mut dropped = 0;
            while rx.recv().await.is_some() {
                dropped += 1;
            }

            if dropped > 0 {
                emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                    count: dropped,
                    reason: "Child stdin closed unexpectedly",
                });
            }

            break;
        }
    }

    pub async fn run<S: Spawner<Process = P>>(
        mut self,
        input: &mut (impl Stream<Item = Event> + Unpin),
        tx: &mpsc::Sender<Event>,
        task: &ExecTask<S>,
    ) -> SessionOutcome {
        loop {
            let input_pipeline = async {
                match &self.stdin_tx {
                    None => future::pending().await,
                    Some(tx) => match tx.reserve().await {
                        Err(_) => Err(InputStatus::ChannelClosed),
                        Ok(permit) => match input.next().await {
                            Some(event) => {
                                permit.send(event);
                                Ok(())
                            }
                            None => Err(InputStatus::Eof),
                        },
                    },
                }
            };

            tokio::select! {
                biased; // Prioritize reading output to prevent deadlocks on stdout buffers

                output = self.output_stream.next() => match handle_output(output, tx, task).await {
                    ControlFlow::Continue(()) => continue,
                    ControlFlow::Break(SessionOutcome::ChildExited) => {
                        self.wait_for_child().await;
                        return self.determine_exit_reason();
                    }
                    ControlFlow::Break(outcome) => return outcome,
                },

                res = input_pipeline => match res {
                    Ok(()) => {}
                    Err(InputStatus::Eof) => {
                        self.stdin_tx = None;
                        self.input_exhausted = true;
                    }
                    Err(InputStatus::ChannelClosed) => self.stdin_tx = None,
                },
            }
        }
    }

    pub async fn wait_for_child(&mut self) {
        // Drop tx to signal the writer task to stop if it hasn't already
        self.stdin_tx = None;

        if let Some(handle) = self.stdin_handle.take() {
            let _ = handle.await;
        }

        Self::close_process(&mut self.process).await;
    }

    pub const fn determine_exit_reason(&self) -> SessionOutcome {
        if self.input_exhausted {
            SessionOutcome::InputExhausted
        } else {
            SessionOutcome::ChildExited
        }
    }

    /// Consumes the session and returns the output stream.
    ///
    /// This is optimized for short-lived sessions where we want to send a
    /// single input (or no input), close stdin, and stream the results until
    /// the process exits.
    ///
    /// The returned stream includes the logic to wait for the child process and
    /// log exit statuses when the stream ends.
    pub fn into_output_stream(
        mut self,
        input: Option<Event>,
    ) -> impl Stream<Item = OutputStreamItem> {
        if let Some(event) = input
            && let Some(tx) = &self.stdin_tx
        {
            let _ = tx.try_send(event);
        };

        // Close Stdin immediately to signal EOF to the child
        self.stdin_tx = None;

        self.attach_teardown_logic()
    }

    #[allow(dead_code)]
    pub fn into_output_stream_batch(
        mut self,
        batch: Vec<Event>,
    ) -> impl Stream<Item = OutputStreamItem> {
        let mut stdin_tx = self.stdin_tx.take();

        // We must spawn a task to feed the batch because it might be larger
        // than the channel capacity (2).
        tokio::spawn(async move {
            if let Some(tx) = stdin_tx.as_mut() {
                for event in batch {
                    if tx.send(event).await.is_err() {
                        break;
                    }
                }
            }

            // tx is dropped here, closing stdin
        });

        self.attach_teardown_logic()
    }

    fn attach_teardown_logic(self) -> impl Stream<Item = OutputStreamItem> + Unpin {
        let ChildSession {
            output_stream,
            mut stdin_handle,
            mut process,
            ..
        } = self;

        output_stream
            .chain(
                stream::once(async move {
                    // Wait for the writer task to finish.
                    if let Some(handle) = stdin_handle.take() {
                        let _ = handle.await;
                    }

                    Self::close_process(&mut process).await;

                    // Return an empty stream to signify the end.
                    stream::empty()
                })
                .flatten(),
            )
            .boxed()
    }

    /// Close the process and log any errors.
    async fn close_process(process: &mut P) {
        match process.wait().await {
            Ok(status) if !status.is_success() => error!(
                status = status.code().unwrap_or(-1).to_string(),
                "Child process exited with non-zero status"
            ),
            Err(error) => error!(
                error = error.to_string(),
                "Failed to wait for child process"
            ),
            _ => {}
        }
    }
}

/// Process output from the child.
///
/// Returns [`ControlFlow::Break`] if the session should end,
/// [`ControlFlow::Continue`] otherwise.
async fn handle_output<P, S>(
    output: Option<OutputStreamItem>,
    tx: &mpsc::Sender<Event>,
    task: &ExecTask<S>,
) -> ControlFlow<SessionOutcome, ()>
where
    P: Process + 'static,
    S: Spawner<Process = P>,
{
    let Some((result, stream_kind)) = output else {
        return ControlFlow::Break(SessionOutcome::ChildExited);
    };

    match result {
        Ok((events, _byte_size)) => {
            for mut event in events {
                task.tag_event(&mut event, stream_kind);
                if tx.send(event).await.is_err() {
                    return ControlFlow::Break(SessionOutcome::DownstreamClosed);
                }
            }
        }
        Err(error) => {
            warn!(
                error = error.to_string(),
                stream = stream_kind.as_str(),
                "Decoding error"
            );
        }
    }

    ControlFlow::Continue(())
}

/// Status of the input stream.
enum InputStatus {
    /// The upstream channel has no more items to feed the process.
    ///
    /// We can close the stdin stream and allow the process to exit, while still
    /// reading from the output stream for as long as it is still open.
    Eof,

    /// The input stream was closed.
    ///
    /// This does NOT mean that the process has exited, or that the output
    /// stream has closed as well. We can still receive more events from the
    /// process, but we can't send more events to the process.
    ChannelClosed,
}
