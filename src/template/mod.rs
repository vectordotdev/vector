//! Functionality for managing template fields used by Vector's sinks.
use std::{borrow::Cow, convert::TryFrom, fmt, hash::Hash, path::PathBuf, sync::LazyLock};

use bytes::Bytes;
use chrono::{
    FixedOffset, Utc,
    format::{Item, strftime::StrftimeItems},
};
use http::Uri;
use regex::Regex;

use snafu::Snafu;
use tracing::warn;
use vector_lib::{
    configurable::{ConfigurableNumber, ConfigurableString, NumberClass, configurable_component},
    lookup::lookup_v2::parse_target_path,
};

use crate::{
    config::log_schema,
    event::{EventRef, Metric, Value},
};

/// Maximum byte length of a rendered value before it is rejected.
///
/// Bounds per-event cost and provides a coarse cap on memory blow-up from
/// attacker-controlled fields.
pub const MAX_RENDERED_PATH_LEN: usize = 1024;

static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(?P<key>[^\}]+)\}\}").unwrap());

/// Errors raised whilst parsing a Template field.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateParseError {
    #[snafu(display("Invalid strftime item"))]
    StrftimeError,
    #[snafu(display(
        "Invalid field path in template {:?} (see https://vector.dev/docs/reference/configuration/template-syntax/)",
        path
    ))]
    InvalidPathSyntax { path: String },
    #[snafu(display("Invalid numeric template"))]
    InvalidNumericTemplate { template: String },
}

/// Errors raised whilst rendering a Template.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq, Snafu)]
pub enum TemplateRenderingError {
    #[snafu(display("Missing fields on event: {:?}", missing_keys))]
    MissingKeys { missing_keys: Vec<String> },
    #[snafu(display("Not numeric: {:?}", input))]
    NotNumeric { input: String },
    /// The rendered value was rejected by the confinement check attached to
    /// this template — the event should be dropped as an intentional discard.
    ///
    /// `rendered_preview` is bounded to [`CONFINED_PREVIEW_BYTES`] bytes to
    /// avoid two problems: leaking secrets in fields that carry credentials
    /// (e.g. `Authorization: Bearer ...` header templates), and amplifying
    /// attacker-controlled oversized field values into logs.
    #[snafu(display(
        "rendered value ({rendered_len} bytes, preview {rendered_preview:?}) \
         confined: {message}"
    ))]
    Confined {
        rendered_preview: String,
        rendered_len: usize,
        message: String,
    },
}

/// Maximum number of bytes of a rejected rendered value to include in a
/// [`TemplateRenderingError::Confined`] error. Kept small so log lines
/// remain bounded even under attacker-controlled input.
pub const CONFINED_PREVIEW_BYTES: usize = 32;

/// Build a bounded preview of a rendered value for inclusion in
/// [`TemplateRenderingError::Confined`]. Truncates on a UTF-8 char boundary.
pub fn confined_preview(rendered: &str) -> String {
    if rendered.len() <= CONFINED_PREVIEW_BYTES {
        return rendered.to_string();
    }
    let mut end = CONFINED_PREVIEW_BYTES;
    while end > 0 && !rendered.is_char_boundary(end) {
        end -= 1;
    }
    rendered.get(..end).unwrap_or("").to_string()
}

include!("unconfined.rs");
include!("confined.rs");
include!("configurable.rs");
include!("unsigned.rs");
include!("parsing.rs");
include!("confinement.rs");

#[cfg(test)]
mod tests;
