use log::{Level, LevelFilter};
use once_cell::sync::OnceCell;
use owo_colors::{
    OwoColorize,
    Stream::{Stderr, Stdout},
};
use std::process::Command;

use crate::config::{Config, ConfigFile};

static VERBOSITY: OnceCell<LevelFilter> = OnceCell::new();
static CONFIG_FILE: OnceCell<ConfigFile> = OnceCell::new();
static CONFIG: OnceCell<Config> = OnceCell::new();
static PATH: OnceCell<String> = OnceCell::new();

pub fn verbosity() -> &'static LevelFilter {
    VERBOSITY.get().expect("verbosity is not initialized")
}

pub fn config_file() -> &'static ConfigFile {
    CONFIG_FILE.get().expect("config file is not initialized")
}

pub fn config() -> &'static Config {
    CONFIG.get().expect("config is not initialized")
}

pub fn path() -> &'static String {
    PATH.get().expect("path is not initialized")
}

pub fn display<T: AsRef<str>>(text: T) {
    // Simply bold rather than bright white for terminals with white backgrounds
    println!(
        "{}",
        text.as_ref().if_supports_color(Stdout, |text| text.bold())
    );
}

#[allow(dead_code)]
pub fn display_trace<T: AsRef<str>>(text: T) {
    if Level::Trace <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref().if_supports_color(Stderr, |text| text.bold())
        );
    }
}

#[allow(dead_code)]
pub fn display_debug<T: AsRef<str>>(text: T) {
    if Level::Debug <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref().if_supports_color(Stderr, |text| text.bold())
        );
    }
}

#[allow(dead_code)]
pub fn display_info<T: AsRef<str>>(text: T) {
    if Level::Info <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref().if_supports_color(Stderr, |text| text.bold())
        );
    }
}

#[allow(dead_code)]
pub fn display_success<T: AsRef<str>>(text: T) {
    if Level::Info <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref()
                .if_supports_color(Stderr, |text| text.bright_cyan())
        );
    }
}

#[allow(dead_code)]
pub fn display_waiting<T: AsRef<str>>(text: T) {
    if Level::Info <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref()
                .if_supports_color(Stderr, |text| text.bright_magenta())
        );
    }
}

#[allow(dead_code)]
pub fn display_warning<T: AsRef<str>>(text: T) {
    if Level::Warn <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref()
                .if_supports_color(Stderr, |text| text.bright_yellow())
        );
    }
}

pub fn display_error<T: AsRef<str>>(text: T) {
    if Level::Error <= *verbosity() {
        eprintln!(
            "{}",
            text.as_ref()
                .if_supports_color(Stderr, |text| text.bright_red())
        );
    }
}

pub fn construct_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.current_dir(path());

    command
}

pub fn set_global_verbosity(verbosity: LevelFilter) {
    VERBOSITY.set(verbosity).unwrap()
}

pub fn set_global_config_file(config_file: ConfigFile) {
    CONFIG_FILE.set(config_file).unwrap()
}

pub fn set_global_config(config: Config) {
    CONFIG.set(config).unwrap()
}

pub fn set_global_path(path: String) {
    PATH.set(path).unwrap()
}
