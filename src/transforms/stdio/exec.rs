use std::{
    collections::VecDeque,
    fmt, io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures::{Stream, StreamExt, future::Either, stream};
use pin_project::pin_project;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    time::{Instant, MissedTickBehavior, interval_at},
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::{FramedRead, FramedWrite};
use vector_lib::{
    codecs::encoding,
    config::{LogNamespace, log_schema},
    event::Event,
    internal_event::{ComponentEventsDropped, INTENTIONAL, UNINTENTIONAL},
    lookup::{
        metadata_path, path,
        path::{PathPrefix, ValuePath as _},
    },
};

use crate::{
    codecs::{Decoder, Encoder},
    transforms::stdio::{
        config::{CommandConfig, Mode, PerEventConfig, ScheduledConfig, StreamingConfig},
        process::{OsSpawner, Process, Spawner},
        session::{ChildSession, OutputStreamItem, SessionOutcome},
    },
};

/// Type alias for production use.
pub(super) type OsExecTask = ExecTask<OsSpawner>;

#[derive(Clone)]
pub(super) struct ExecTask<S: Spawner> {
    /// Command configuration.
    pub command: CommandConfig,

    /// Operating mode.
    pub mode: Mode,

    /// Scheduled mode configuration.
    pub scheduled: Option<ScheduledConfig>,

    /// Streaming mode configuration.
    pub streaming: Option<StreamingConfig>,

    /// Per-event mode configuration.
    pub per_event: Option<PerEventConfig>,

    /// Whether to capture stderr.
    pub capture_stderr: bool,

    /// Process spawner.
    pub spawner: S,

    /// Encoder for stdin.
    pub stdin_encoder: Encoder<encoding::Framer>,

    /// Decoder for stdout.
    pub stdout_decoder: Decoder,

    /// Decoder for stderr (None if stderr is dropped).
    pub stderr_decoder: Option<Decoder>,
}

impl<S: Spawner> ExecTask<S> {
    pub(super) fn tag_event(&self, event: &mut Event, kind: StdStream) {
        let Event::Log(log) = event else { return };

        let (namespace, tag) = match kind {
            StdStream::Stdout => (&self.stdout_decoder.log_namespace, "stdout"),
            StdStream::Stderr => {
                let Some(decoder) = &self.stderr_decoder else {
                    return;
                };

                (&decoder.log_namespace, "stderr")
            }
        };

        match namespace {
            LogNamespace::Vector => {
                log.insert(metadata_path!("vector", "stream"), tag);
            }
            LogNamespace::Legacy => {
                if let Some(metadata_key) = log_schema().metadata_key() {
                    log.insert(
                        (PathPrefix::Event, metadata_key.concat(path!("stream"))),
                        tag,
                    );
                }
            }
        }
    }

    /// Spawns the process via the spawner. Returns [`SpawnError`] when spawning
    /// fails.
    #[inline]
    fn spawn_process(&self) -> Result<S::Process, SpawnError> {
        self.spawner
            .spawn(&self.command, self.capture_stderr)
            .map_err(SpawnError::Spawn)
    }

    /// Takes a raw spawned process and wraps it into a [`ChildSession`].
    ///
    /// This centralizes the logic for framing Stdin and framing + merging
    /// Stdout/Stderr.
    fn create_child_session(
        &self,
        mut process: S::Process,
    ) -> Result<
        ChildSession<
            S::Process,
            impl Stream<Item = OutputStreamItem> + Send + Unpin + 'static + use<S>,
        >,
        SpawnError,
    > {
        let stdin = process.take_stdin().ok_or(SpawnError::NoStdin)?;
        let stdout = process.take_stdout().ok_or(SpawnError::NoStdout)?;
        let stderr = process.take_stderr();

        let stdin_writer = FramedWrite::new(stdin, self.stdin_encoder.clone());
        let output_stream = self.build_output_stream(stdout, stderr);

        Ok(ChildSession::new(process, stdin_writer, output_stream))
    }

    /// Build the output stream from the stdout and stderr streams.
    ///
    /// If stderr is not configured, it is ignored.
    fn build_output_stream(
        &self,
        stdout: <S::Process as Process>::Stdout,
        stderr: Option<<S::Process as Process>::Stderr>,
    ) -> impl Stream<Item = OutputStreamItem> + Send + Unpin + 'static + use<S>
    where
        S::Process: Process,
    {
        let stdout_stream = FramedRead::new(stdout, self.stdout_decoder.clone())
            .map(|res| (res, StdStream::Stdout));

        match (stderr, self.stderr_decoder.clone()) {
            (Some(stderr), Some(decoder)) => {
                let stderr_stream =
                    FramedRead::new(stderr, decoder).map(|res| (res, StdStream::Stderr));

                Either::Left(stream::select(stdout_stream, stderr_stream))
            }
            _ => Either::Right(stdout_stream),
        }
    }

    /// Handles the "Streaming" logic: spawning, retrying on transient errors,
    /// and giving up on fatal errors.
    ///
    /// Returns `None` if a fatal error occurs (should stop processing).
    async fn spawn_streaming_session(
        &self,
        config: &StreamingConfig,
    ) -> Option<
        ChildSession<S::Process, impl Stream<Item = OutputStreamItem> + Send + Unpin + 'static>,
    > {
        loop {
            match self
                .spawn_process()
                .and_then(|process| self.create_child_session(process))
            {
                Ok(session) => return Some(session),
                Err(SpawnError::Spawn(error)) if is_fatal_io_error(&error) => {
                    error!(
                        error = error.to_string(),
                        "Fatal error spawning process (will stop retrying)"
                    );

                    return None;
                }
                Err(error) => error!(
                    error = error.to_string(),
                    "Failed to spawn process, retrying"
                ),
            }

            // Sleep before retrying transient errors
            tokio::time::sleep(config.respawn_interval()).await;
        }
    }

    /// Centralized logic to decode raw output chunks into tagged Events.
    fn decode_child_output<T>(
        &self,
        output: T,
    ) -> impl Stream<Item = Event> + Send + 'static + use<S, T>
    where
        T: Stream<Item = OutputStreamItem> + Send + 'static,
    {
        let task = Arc::new(self.clone());

        output.flat_map(move |(result, stream_kind)| {
            let task = Arc::clone(&task);
            match result {
                Ok((events, _)) => Either::Left(stream::iter(events).map(move |mut event| {
                    task.tag_event(&mut event, stream_kind);
                    event
                })),
                Err(error) => {
                    warn!(
                        error = error.to_string(),
                        stream = stream_kind.as_str(),
                        "Decoding error"
                    );
                    Either::Right(stream::empty())
                }
            }
        })
    }

    /// Runs the `per_event` mode.
    pub(super) fn run_per_event(
        self,
        input: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> impl Stream<Item = Event> + Send {
        let task = Arc::new(self);

        let concurrency_limit = task
            .per_event
            .map(|c| c.max_concurrent_processes)
            .unwrap_or(50);

        let semaphore = Arc::new(Semaphore::new(concurrency_limit));

        input
            .map(move |event| {
                let task = Arc::clone(&task);
                let semaphore = Arc::clone(&semaphore);

                async move {
                    // Acquire permit before spawning.
                    //
                    // This limits the number of active streams/processes.
                    let Ok(permit) = semaphore.acquire_owned().await else {
                        // Semaphore closed (unreachable?).
                        return Either::Left(stream::empty());
                    };

                    let session = task
                        .spawn_process()
                        .and_then(|process| task.create_child_session(process));

                    let session = match session {
                        Ok(session) => session,
                        Err(error) => {
                            emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                                count: 1,
                                reason: &format!("Failed to spawn child process: {error}."),
                            });

                            tokio::time::sleep(Duration::from_secs(1)).await;
                            return Either::Left(stream::empty());
                        }
                    };

                    let stream = task.decode_child_output(session.into_output_stream(Some(event)));
                    Either::Right(PermitStream { stream, permit })
                }
            })
            .buffered(concurrency_limit)
            .flatten_unordered(concurrency_limit)
    }

    /// Runs the `scheduled` mode.
    pub(super) fn run_scheduled(
        self,
        mut input: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> impl Stream<Item = Event> + Send {
        let task = Arc::new(self);
        let ScheduledConfig {
            exec_interval_secs,
            buffer_size,
        } = task.scheduled.unwrap_or_default();

        let exec_interval = Duration::from_secs_f64(exec_interval_secs);
        let (tx, rx) = mpsc::channel(10);

        tokio::spawn(async move {
            let mut buffer: VecDeque<Event> = VecDeque::with_capacity(buffer_size);
            let start = Instant::now() + exec_interval;
            let mut ticker = interval_at(start, exec_interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let flush_batch = |batch: Vec<Event>| {
                let count = batch.len();
                if count == 0 {
                    return;
                }

                let session = task
                    .spawn_process()
                    .and_then(|p| task.create_child_session(p));

                let session = match session {
                    Ok(session) => session,
                    Err(error) => {
                        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                            count,
                            reason: &format!("Failed to spawn child process: {error}."),
                        });

                        return;
                    }
                };

                let tx = tx.clone();
                let task = Arc::clone(&task);

                // Spawn execution to ensure the main loop keeps buffering.
                tokio::spawn(async move {
                    let mut stream =
                        task.decode_child_output(session.into_output_stream_batch(batch));

                    let _ = tokio::time::timeout(exec_interval, async {
                        while let Some(event) = stream.next().await {
                            if tx.send(event).await.is_err() {
                                break;
                            }
                        }
                    })
                    .await;
                });
            };

            loop {
                // If buffer is empty, wait for input. This avoids consuming a
                // 'tick' when there is no work to do.
                if buffer.is_empty() {
                    match input.next().await {
                        Some(event) => buffer.push_back(event),
                        None => break, // EOF
                    }
                }

                tokio::select! {
                    biased;

                    event = input.next() => match event {
                        Some(event) => {
                            if buffer.len() >= buffer_size {
                                buffer.pop_front();
                                emit!(ComponentEventsDropped::<INTENTIONAL> {
                                    count: 1,
                                    reason: "Buffer was full.",
                                });
                            }
                            buffer.push_back(event);
                        }

                        // Upstream closed.
                        None => {
                            // Ensure we flush the buffer before shutting down.
                            flush_batch(buffer.drain(..).collect());
                            break;
                        }
                    },

                    _ = ticker.tick() => flush_batch(buffer.drain(..).collect()),
                }
            }
        });

        ReceiverStream::new(rx)
    }

    /// Runs the `streaming` mode.
    pub(super) fn run_streaming(
        self,
        input: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> impl Stream<Item = Event> + Send {
        let task = Arc::new(self);
        let config = task.streaming.unwrap_or_default();
        let (tx, rx) = mpsc::channel(10);

        tokio::spawn(async move {
            let mut input = input;

            // spawn_streaming_session handles the retry logic internally. If it
            // returns None, it means a fatal error occurred.
            while let Some(session) = task.spawn_streaming_session(&config).await {
                let outcome = session.run(&mut input, &tx, &task).await;

                match outcome {
                    SessionOutcome::InputExhausted | SessionOutcome::DownstreamClosed => break,
                    SessionOutcome::ChildExited => {
                        if !config.respawn_on_exit {
                            break;
                        }

                        tokio::time::sleep(config.respawn_interval()).await;
                    }
                }
            }
        });

        ReceiverStream::new(rx)
    }
}

/// Determines if an IO error is fatal (should stop the component) or transient
/// (should retry).
fn is_fatal_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound
            | io::ErrorKind::PermissionDenied
            | io::ErrorKind::IsADirectory
            | io::ErrorKind::InvalidInput
    )
}

/// A stream wrapper that holds a semaphore permit.
///
/// The permit is dropped when the stream is dropped.
#[pin_project]
struct PermitStream<S> {
    #[pin]
    stream: S,
    permit: OwnedSemaphorePermit,
}

impl<S: Stream> Stream for PermitStream<S> {
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().stream.poll_next(cx)
    }
}

/// The standard-stream from which an event was captured.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) enum StdStream {
    /// The stdout stream.
    Stdout,

    /// The stderr stream.
    Stderr,
}

impl StdStream {
    /// Returns the string representation of the stream.
    pub const fn as_str(self) -> &'static str {
        match self {
            StdStream::Stdout => "stdout",
            StdStream::Stderr => "stderr",
        }
    }
}

/// Error type returned when spawning a process.
#[derive(Debug)]
pub(super) enum SpawnError {
    /// Failed to spawn the process.
    Spawn(io::Error),

    /// Failed to capture stdin.
    NoStdin,

    /// Failed to capture stdout.
    NoStdout,
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SpawnError::Spawn(e) => Some(e),
            SpawnError::NoStdin => None,
            SpawnError::NoStdout => None,
        }
    }
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpawnError::Spawn(_) => write!(f, "failed to spawn process"),
            SpawnError::NoStdin => write!(f, "failed to capture stdin"),
            SpawnError::NoStdout => write!(f, "failed to capture stdout"),
        }
    }
}
