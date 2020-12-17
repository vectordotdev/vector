use crate::event::{LookupBuf, Value};
use chrono::{DateTime, Local, ParseError as ChronoParseError, TimeZone, Utc};
use lazy_static::lazy_static;
use std::path::PathBuf;

pub use shared::conversion::*;

lazy_static! {
    pub static ref DEFAULT_CONFIG_PATHS: Vec<PathBuf> = vec!["/etc/vector/vector.toml".into()];
}
