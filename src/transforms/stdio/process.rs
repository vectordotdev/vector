use std::{future::Future, io, process::Stdio};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
};

use super::config::CommandConfig;

/// Abstraction over process exit status.
///
/// We need this because [`std::process::ExitStatus`] cannot be constructed
/// directly, making it impossible to use in tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExitStatus {
    code: Option<i32>,
}

impl ExitStatus {
    #[cfg(test)]
    const fn success() -> Self {
        Self { code: Some(0) }
    }

    #[cfg(test)]
    const fn from_code(code: i32) -> Self {
        Self { code: Some(code) }
    }

    pub(super) const fn code(self) -> Option<i32> {
        self.code
    }

    pub(super) const fn is_success(self) -> bool {
        matches!(self.code, Some(0))
    }
}

impl From<std::process::ExitStatus> for ExitStatus {
    fn from(status: std::process::ExitStatus) -> Self {
        Self {
            code: status.code(),
        }
    }
}

/// Trait for spawning processes.
pub trait Spawner: Clone + Send + Sync + 'static {
    type Process: Process;

    /// Spawn a process.
    fn spawn(&self, config: &CommandConfig, capture_stderr: bool) -> io::Result<Self::Process>;
}

/// Trait representing a running process with I/O handles.
pub trait Process: Send {
    type Stdin: AsyncWrite + Unpin + Send + 'static;
    type Stdout: AsyncRead + Unpin + Send + 'static;
    type Stderr: AsyncRead + Unpin + Send + 'static;

    fn take_stdin(&mut self) -> Option<Self::Stdin>;
    fn take_stdout(&mut self) -> Option<Self::Stdout>;
    fn take_stderr(&mut self) -> Option<Self::Stderr>;

    /// Wait for the process to exit.
    fn wait(&mut self) -> impl Future<Output = io::Result<ExitStatus>> + Send;
}

/// Production spawner using [`tokio::process`].
#[derive(Clone, Copy, Debug, Default)]
pub struct OsSpawner;

impl Spawner for OsSpawner {
    type Process = OsProcess;

    #[inline]
    fn spawn(&self, config: &CommandConfig, capture_stderr: bool) -> io::Result<Self::Process> {
        if config.command.is_empty() {
            return Err(io::Error::other(
                "command must contain at least one element",
            ));
        }

        let mut cmd = Command::new(&config.command[0]);

        if config.command.len() > 1 {
            cmd.args(&config.command[1..]);
        }

        if config.clear_environment {
            cmd.env_clear();
        }

        if let Some(envs) = &config.environment {
            cmd.envs(envs);
        }

        if let Some(dir) = &config.working_directory {
            cmd.current_dir(dir);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(if capture_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        cmd.kill_on_drop(true);

        cmd.spawn().map(OsProcess)
    }
}

/// Wrapper around [`tokio::process::Child`].
pub struct OsProcess(Child);

impl Process for OsProcess {
    type Stdin = ChildStdin;
    type Stdout = ChildStdout;
    type Stderr = ChildStderr;

    #[inline]
    fn take_stdin(&mut self) -> Option<Self::Stdin> {
        self.0.stdin.take()
    }

    #[inline]
    fn take_stdout(&mut self) -> Option<Self::Stdout> {
        self.0.stdout.take()
    }

    #[inline]
    fn take_stderr(&mut self) -> Option<Self::Stderr> {
        self.0.stderr.take()
    }

    async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.0.wait().await.map(ExitStatus::from)
    }
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };
    use tokio::{io::DuplexStream, sync::oneshot};

    /// Handle returned to test code for controlling a mock process.
    pub struct ProcessHandle {
        /// Write here to provide data that the task will read from "stdout".
        pub stdout: DuplexStream,

        /// Write here to provide data that the task will read from "stderr".
        pub stderr: Option<DuplexStream>,

        /// Read here to see what the task wrote to "stdin".
        pub stdin: DuplexStream,

        /// Send here to make the process "exit".
        pub exit_tx: oneshot::Sender<ExitStatus>,
    }

    impl ProcessHandle {
        /// Signal exit with a specific code.
        pub fn exit(self, code: i32) {
            let _ = self.exit_tx.send(ExitStatus::from_code(code));
        }
    }

    /// Builder for setting up mock processes.
    #[derive(Default)]
    pub struct MockSpawnerBuilder {
        setups: VecDeque<Result<ProcessSetup, io::Error>>,
    }

    /// A collection of streams and channels of a mocked process.
    struct ProcessSetup {
        stdin: DuplexStream,
        stdout: DuplexStream,
        stderr: Option<DuplexStream>,
        exit_rx: oneshot::Receiver<ExitStatus>,
    }

    impl MockSpawnerBuilder {
        /// Create a new builder.
        pub fn new() -> Self {
            Self::default()
        }

        /// Queue a process to be spawned. Returns a handle for the test to
        /// control it.
        pub fn expect_spawn(&mut self, with_stderr: bool) -> ProcessHandle {
            let (stdin_task, stdin_test) = tokio::io::duplex(8192);
            let (stdout_test, stdout_task) = tokio::io::duplex(8192);
            let (stderr_test, stderr_task) = if with_stderr {
                let (a, b) = tokio::io::duplex(8192);
                (Some(a), Some(b))
            } else {
                (None, None)
            };

            let (exit_tx, exit_rx) = oneshot::channel();

            self.setups.push_back(Ok(ProcessSetup {
                stdin: stdin_task,
                stdout: stdout_task,
                stderr: stderr_task,
                exit_rx,
            }));

            ProcessHandle {
                stdin: stdin_test,
                stdout: stdout_test,
                stderr: stderr_test,
                exit_tx,
            }
        }

        /// Queue a spawn failure. The next time the component tries to spawn,
        /// it will receive this error immediately.
        pub fn expect_spawn_error(&mut self, error: io::Error) {
            self.setups.push_back(Err(error));
        }

        /// Create a new [`MockSpawner`] from the configured setups.
        pub fn build(self) -> MockSpawner {
            MockSpawner {
                setups: Arc::new(Mutex::new(self.setups)),
            }
        }
    }

    /// Mock spawner that returns pre-configured mock processes.
    #[derive(Clone)]
    pub struct MockSpawner {
        setups: Arc<Mutex<VecDeque<Result<ProcessSetup, io::Error>>>>,
    }

    impl Spawner for MockSpawner {
        type Process = MockProcess;

        fn spawn(
            &self,
            _config: &CommandConfig,
            _capture_stderr: bool,
        ) -> io::Result<Self::Process> {
            self.setups
                .lock()
                .unwrap()
                .pop_front()
                .transpose()?
                .map(|setup| MockProcess {
                    stdin: Some(setup.stdin),
                    stdout: Some(setup.stdout),
                    stderr: setup.stderr,
                    exit_rx: Some(setup.exit_rx),
                })
                .ok_or(io::Error::other("no more mock processes configured"))
        }
    }

    pub struct MockProcess {
        stdin: Option<DuplexStream>,
        stdout: Option<DuplexStream>,
        stderr: Option<DuplexStream>,
        exit_rx: Option<oneshot::Receiver<ExitStatus>>,
    }

    impl Process for MockProcess {
        type Stdin = DuplexStream;
        type Stdout = DuplexStream;
        type Stderr = DuplexStream;

        fn take_stdin(&mut self) -> Option<Self::Stdin> {
            self.stdin.take()
        }

        fn take_stdout(&mut self) -> Option<Self::Stdout> {
            self.stdout.take()
        }

        fn take_stderr(&mut self) -> Option<Self::Stderr> {
            self.stderr.take()
        }

        async fn wait(&mut self) -> io::Result<ExitStatus> {
            match self.exit_rx.take() {
                Some(rx) => rx.await.map_err(|_| {
                    io::Error::new(io::ErrorKind::BrokenPipe, "process handle dropped")
                }),
                None => Ok(ExitStatus::success()),
            }
        }
    }
}
