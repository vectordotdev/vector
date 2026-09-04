//! Command execution utilities

use std::{
    ffi::{OsStr, OsString},
    process::{self, Command},
};

/// Trait for chaining command arguments
pub trait ChainArgs {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString>;
}

impl<T: AsRef<OsStr>> ChainArgs for Vec<T> {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString> {
        self.iter()
            .map(Into::into)
            .chain(args.into_iter().map(Into::into))
            .collect()
    }
}

impl<T: AsRef<OsStr>> ChainArgs for [T] {
    fn chain_args<I: Into<OsString>>(&self, args: impl IntoIterator<Item = I>) -> Vec<OsString> {
        self.iter()
            .map(Into::into)
            .chain(args.into_iter().map(Into::into))
            .collect()
    }
}

/// Run a shell command and return its output or exit on failure
pub fn run_command(cmd: &str) -> String {
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .expect("Failed to execute command");

    if !output.status.success() {
        eprintln!(
            "Command failed: {cmd} - Error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        process::exit(1);
    }

    String::from_utf8_lossy(&output.stdout).to_string()
}
