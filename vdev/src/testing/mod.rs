pub mod config;
pub mod integration;
pub mod runner;
pub mod state;

pub(self) fn get_rust_version() -> String {
    match config::RustToolchainConfig::parse() {
        Ok(config) => config.channel,
        Err(error) => fatal!("Could not read `rust-toolchain.toml` file: {error}"),
    }
}
