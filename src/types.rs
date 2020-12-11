use crate::event::Value;
use chrono::{DateTime, Local, ParseError as ChronoParseError, TimeZone, Utc};
use lazy_static::lazy_static;
use snafu::{ResultExt, Snafu};
use std::collections::{HashMap, HashSet};
use std::num::{ParseFloatError, ParseIntError};
use std::path::PathBuf;
use std::str::FromStr;

lazy_static! {
    pub static ref DEFAULT_CONFIG_PATHS: Vec<PathBuf> = vec!["/etc/vector/vector.toml".into()];
}
