use std::env;
use std::process;

use protoc_bin_vendored::protoc_bin_path;

fn main() {
    let protoc_bin_path = match protoc_bin_path() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("protoc binary not found: {}", e);
            process::exit(11);
        }
    };
    let mut command = match process::Command::new(protoc_bin_path)
        .args(env::args_os().skip(1))
        .spawn()
    {
        Ok(command) => command,
        Err(e) => {
            eprintln!("failed to spawn protoc: {}", e);
            process::exit(12);
        }
    };
    let exit_status = command.wait().unwrap();
    process::exit(exit_status.code().unwrap_or(13));
}
