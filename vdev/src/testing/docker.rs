use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::app::VDevCommand;

pub static CONTAINER_TOOL: LazyLock<OsString> =
    LazyLock::new(|| env::var_os("CONTAINER_TOOL").unwrap_or_else(detect_container_tool));

pub(super) static DOCKER_SOCKET: LazyLock<PathBuf> = LazyLock::new(detect_docker_socket);

pub fn docker_command<I: AsRef<OsStr>>(args: impl IntoIterator<Item = I>) -> VDevCommand {
    VDevCommand::new(&*CONTAINER_TOOL).args(args)
}

fn detect_container_tool() -> OsString {
    for tool in ["docker", "podman"] {
        if VDevCommand::new(tool)
            .arg("version")
            .run()
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
