use log::{Level, LevelFilter};
use owo_colors::{
    OwoColorize,
    Stream::{Stderr, Stdout},
};
use std::env;

use crate::config::{Config, ConfigFile};
use crate::platform::Platform;
use crate::repo::core::Repository;

pub struct Application {
    pub(crate) config_file: ConfigFile,
    pub(crate) config: Config,
    pub(crate) repo: Repository,
    pub(crate) platform: Platform,
    verbosity: LevelFilter,
}

impl Application {
    pub fn new(verbosity: LevelFilter) -> Application {
        let platform = Platform::new();
        let config_file = ConfigFile::new();
        let config_model = config_file.load();

        // Set the path to the repository for the entire application
        let path = if !config_model.repo.is_empty() {
            config_model.repo.to_string()
        } else {
            match env::current_dir() {
                Ok(p) => p.display().to_string(),
                Err(_) => ".".to_string(),
            }
        };

        Application {
            config_file: config_file,
            config: config_model,
            repo: Repository::new(path),
            platform: platform,
            verbosity: verbosity,
        }
    }

    pub fn exit(&self, code: i32) {
        std::process::exit(code);
    }

    pub fn abort<T: AsRef<str>>(&self, text: T) {
        self.display_error(text);
        self.exit(1);
    }

    pub fn display<T: AsRef<str>>(&self, text: T) {
        // Simply bold rather than bright white for terminals with white backgrounds
        println!(
            "{}",
            text.as_ref().if_supports_color(Stdout, |text| text.bold())
        );
    }

    #[allow(dead_code)]
    pub fn display_trace<T: AsRef<str>>(&self, text: T) {
        if Level::Trace <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref().if_supports_color(Stderr, |text| text.bold())
            );
        }
    }

    #[allow(dead_code)]
    pub fn display_debug<T: AsRef<str>>(&self, text: T) {
        if Level::Debug <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref().if_supports_color(Stderr, |text| text.bold())
            );
        }
    }

    #[allow(dead_code)]
    pub fn display_info<T: AsRef<str>>(&self, text: T) {
        if Level::Info <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref().if_supports_color(Stderr, |text| text.bold())
            );
        }
    }

    #[allow(dead_code)]
    pub fn display_success<T: AsRef<str>>(&self, text: T) {
        if Level::Info <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref()
                    .if_supports_color(Stderr, |text| text.bright_cyan())
            );
        }
    }

    #[allow(dead_code)]
    pub fn display_waiting<T: AsRef<str>>(&self, text: T) {
        if Level::Info <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref()
                    .if_supports_color(Stderr, |text| text.bright_magenta())
            );
        }
    }

    #[allow(dead_code)]
    pub fn display_warning<T: AsRef<str>>(&self, text: T) {
        if Level::Warn <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref()
                    .if_supports_color(Stderr, |text| text.bright_yellow())
            );
        }
    }

    pub fn display_error<T: AsRef<str>>(&self, text: T) {
        if Level::Error <= self.verbosity {
            eprintln!(
                "{}",
                text.as_ref()
                    .if_supports_color(Stderr, |text| text.bright_red())
            );
        }
    }
}
