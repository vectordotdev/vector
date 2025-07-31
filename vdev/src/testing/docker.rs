use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

pub static CONTAINER_TOOL: LazyLock<OsString> =
    LazyLock::new(|| env::var_os("CONTAINER_TOOL").unwrap_or_else(detect_container_tool));

pub(super) static DOCKER_SOCKET: LazyLock<PathBuf> = LazyLock::new(detect_docker_socket);

pub fn docker_command<I: AsRef<OsStr>>(args: impl IntoIterator<Item = I>) -> Command {
    let mut command = Command::new(&*CONTAINER_TOOL);
    command.args(args);
    command
}

fn detect_container_tool() -> OsString {
    for tool in ["docker", "podman"] {
        if Command::new(tool)
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut child| child.wait())
            .is_ok_and(|status| status.success())
        {
            return OsString::from(String::from(tool));
        }
    }
    fatal!("No container tool could be detected.");
}

fn detect_docker_socket() -> PathBuf {
    match env::var_os("DOCKER_HOST") {
        Some(host) => host
            .into_string()
            .expect("Invalid value in $DOCKER_HOST")
            .strip_prefix("unix://")
            .expect("$DOCKER_HOST is not a socket path")
            .into(),
        None => "/var/run/docker.sock".into(),
    }
}
