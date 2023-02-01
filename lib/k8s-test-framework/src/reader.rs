//! Read process output.

use std::process::{ExitStatus, Stdio};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, ChildStdout, Command},
};

/// Keeps track of the command invocation, proving the interface to
/// read the output and send a termination signal.
#[derive(Debug)]
pub struct Reader {
    child: Child,
    reader: BufReader<ChildStdout>,
}

impl Reader {
    /// Spawn a command and provide a [`Reader`].
    pub fn spawn(mut command: Command) -> std::io::Result<Self> {
        Self::prepare_stdout(&mut command);
        let child = command.spawn()?;
        Ok(Self::new(child))
    }

    fn prepare_stdout(command: &mut Command) {
        command.stdout(Stdio::piped());
    }

    fn new(mut child: Child) -> Self {
        let stdout = child.stdout.take().unwrap();
        let reader = BufReader::new(stdout);
        Reader { child, reader }
    }

    /// Wait for the `kubectl logs` process to exit and return the exit code.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Send a termination signal to the `kubectl logs` process.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill().await
    }

    /// Read one line from the stdout of the `kubectl logs` process.
    pub async fn read_line(&mut self) -> Option<String> {
        let mut s = String::new();
        let result = self.reader.read_line(&mut s).await;
        match result {
            Ok(0) => None,
            Ok(_) => Some(s),
            Err(err) => panic!("{}", err),
        }
    }
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::*;

    async fn collect(reader: &mut Reader) -> Vec<String> {
        let mut list = Vec::new();
        while let Some(line) = reader.read_line().await {
            list.push(line)
        }
        list
    }

    #[tokio::test]
    async fn test_reader_finite() {
        let mut command = Command::new("echo");
        command.arg("test");

        let mut reader = Reader::spawn(command).expect("unable to spawn");

        // Collect all line, expect stream to finish.
        let lines = collect(&mut reader).await;
        // Assert we got all the lines we expected.
        assert_eq!(lines, vec!["test\n".to_owned()]);

        // Ensure wait doesn't fail, and that we exit status is success.
        let exit_status = reader.wait().await.expect("wait failed");
        assert!(exit_status.success());
    }

    #[tokio::test]
    async fn test_reader_infinite() {
        let mut command = Command::new("bash");
        command.arg("-c");
        command.arg(r#"NUM=0; while true; do echo "Line $NUM"; NUM=$((NUM+=1)); sleep 0.01; done"#);

        let mut reader = Reader::spawn(command).expect("unable to spawn");

        // Read the lines and at some point ask the command we're reading from
        // to stop.
        let mut expected_num = 0;
        while let Some(line) = reader.read_line().await {
            // Assert we're getting expected lines.
            assert_eq!(line, format!("Line {}\n", expected_num));

            // On line 100 issue a `kill` to stop the infinite stream.
            if expected_num == 100 {
                reader.kill().await.expect("process already stopped")
            }

            // If we are past 200 it means we issued `kill` at 100 and it wasn't
            // effective. This is problem, fail the test.
            // We don't to this immediately after `kill` to allow for some
            // potential race condition. That kind of race is not just ok, but
            // is desirable in the real-life usage to read-up the whole stdout
            // buffer.
            if expected_num > 200 {
                panic!("Went too far without stop being effective");
            }

            // Bump the expected num for the next iteration.
            expected_num += 1;
        }

        // Ensure wait doesn't fail. We killed the process, so expect
        // a non-success exit code.
        let exit_status = reader.wait().await.expect("wait failed");
        assert!(!exit_status.success());
    }
}
