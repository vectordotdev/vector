//! Utility modules for vdev
//!
//! This module provides various utilities organized by functionality:
//! - `paths`: Path operations and repository root detection
//! - `cargo`: Cargo.toml parsing and version management
//! - `git`: Git operations
//! - `command`: Command execution helpers
//! - `config`: Configuration management
//! - `environment`: Environment utilities
//! - `features`: Feature detection and management
//! - `macros`: Utility macros
//! - `platform`: Platform-specific utilities

#![allow(clippy::print_stderr)]
#![allow(clippy::print_stdout)]

use std::{io::IsTerminal, sync::LazyLock};

#[macro_use]
pub mod macros;

pub mod cargo;
pub mod command;
pub mod environment;
pub mod features;
pub mod git;
pub mod paths;
pub mod platform;

/// Check if stdout is connected to a TTY
pub static IS_A_TTY: LazyLock<bool> = LazyLock::new(|| std::io::stdout().is_terminal());
