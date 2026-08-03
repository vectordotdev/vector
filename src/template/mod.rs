//! Functionality for managing template fields used by Vector's sinks.
mod configurable;
mod confined;
mod confinement;
mod parsing;
mod unconfined;
mod unsigned;

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

use confinement::ConfinementChecker;
use parsing::Part;
use unsigned::UnsignedIntTemplateSource;

#[cfg(test)]
use confinement::{BuildError, ConfineError};

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

/// A templated field.
///
/// In many cases, components can be configured so that part of the component's functionality can be
/// customized on a per-event basis. By using `UnconfinedTemplate`, users can specify either fixed
/// strings or templated strings. Templated strings use a common syntax to refer to fields in an
/// event that is used as the input data when rendering the template.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Default, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct UnconfinedTemplate {
    src: String,

    #[serde(skip)]
    parts: Vec<Part>,

    #[serde(skip)]
    is_static: bool,

    #[serde(skip)]
    reserve_size: usize,

    #[serde(skip)]
    tz_offset: Option<FixedOffset>,
}

/// A template that has passed through prefix-based confinement via [`Template::confine`].
///
/// This is for **non-URI fields** (object-store keys, Kafka topics, Redis keys,
/// tenant IDs, filesystem paths). The template's literal prefix becomes the
/// confinement boundary, with runtime checks for:
/// - Prefix match (rendered value must start with the literal prefix)
/// - No `..` path segments
///
/// For HTTP/HTTPS URI fields, use [`ConfinedUriTemplate`] instead, which is
/// obtained via [`UriTemplate::confine`] and enforces URI-specific checks
/// (authority, path-prefix, query/fragment rejection, encoded separator protection).
///
/// This type is deliberately **not** deserializable: the sole way to obtain one
/// is by confining a [`Template`], so a rendered value can never escape its
/// confinement boundary. The `render` methods enforce the attached confinement
/// checker (if any).
///
/// Both fields are private to this module, so a `ConfinedTemplate` can
/// never be constructed (or deserialized) without going through confinement.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ConfinedTemplate {
    inner: UnconfinedTemplate,
    checker: Option<ConfinementChecker>,
}

/// A template that has passed through URI-specific confinement via [`UriTemplate::confine`].
///
/// This is for **HTTP/HTTPS URI fields only** (e.g., the HTTP sink's `uri` field).
/// URI-specific confinement enforces:
/// - Static authority (host) required
/// - Authority match at render time (catches `@`-injection and host-extension)
/// - Path-prefix match
/// - No `..` segments or encoded variants
/// - No query (`?`) or fragment (`#`) injection
/// - No encoded separators (`%2F`, `%5C`, `%25`) or raw backslashes
///
/// For non-URI fields (object-store keys, Kafka topics, Redis keys, tenant IDs),
/// use [`ConfinedTemplate`] via [`Template::confine`] instead, which enforces
/// prefix-based confinement without URI-specific semantics.
///
/// This type is deliberately **not** deserializable and cannot be constructed from
/// a [`ConfinedTemplate`]. The sole way to obtain one is via [`UriTemplate::confine`],
/// which enforces URI-specific confinement checks.
///
/// This type-level separation prevents accidental URI confinement of non-URI fields
/// like object-store key prefixes that happen to start with `http://` (e.g.,
/// `key_prefix: "http://logs-{{ region }}/"` must use prefix confinement, not URI
/// confinement).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ConfinedUriTemplate {
    inner: ConfinedTemplate,
}

/// The templated field type for HTTP/HTTPS URI config fields.
///
/// `UriTemplate` is serde-able and appears as a plain string in generated configuration
/// schemas, but it exposes **no** `render` method. To render it, a sink must first confine it:
///
/// - Call [`UriTemplate::confine`] → [`ConfinedUriTemplate`] for URI-specific confinement.
///
/// This makes URI confinement unavoidable for URI config fields. For non-URI fields
/// (object-store keys, Kafka topics, Redis keys, tenant IDs), use [`Template`] instead.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct UriTemplate {
    /// Inner template
    #[serde(skip)]
    inner: Template,
}

/// The templated field type stored in sink config structs for non-URI fields.
///
/// `Template` is serde-able and appears as a plain string in generated configuration schemas, but
/// it exposes **no** `render` method. To render it a sink must first confine it:
///
/// - **Non-URI fields** (object-store keys, Kafka topics, Redis keys, tenant IDs, filesystem paths):
///   call [`Template::confine`] → [`ConfinedTemplate`] for prefix-based confinement.
/// - **HTTP/HTTPS URI fields** (e.g., HTTP sink `uri`):
///   use [`UriTemplate`] instead, which confines to [`ConfinedUriTemplate`].
///
/// This makes confinement unavoidable: there is no way to render a sink's configured
/// template without going through confinement first. Transforms and sources, which have no
/// confinement boundary, should store [`UnconfinedTemplate`] directly instead.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Template {
    /// Inner template
    #[serde(skip)]
    inner: UnconfinedTemplate,
}

/// Unsigned integer template.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[serde(
    try_from = "UnsignedIntTemplateSource",
    into = "UnsignedIntTemplateSource"
)]
pub struct UnsignedIntTemplate {
    src: UnsignedIntTemplateSource,

    #[serde(skip)]
    parts: Vec<Part>,

    #[serde(skip)]
    tz_offset: Option<FixedOffset>,
}

/// Serializable config fragment for template confinement.
///
/// Embed this in a component config with `#[serde(flatten)]` to get the
/// `dangerously_allow_unconfined_template_resolution` field. Pass it to:
///
/// - [`Template::confine`] for non-URI fields (object-store keys, Kafka topics, etc.)
/// - [`UriTemplate::confine`] for HTTP/HTTPS URI fields
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ConfinementConfig {
    /// Disable all template confinement checks for this sink.
    ///
    /// **DANGEROUS — disables a security control.**
    ///
    /// Bypasses both startup validation and runtime confinement for every
    /// templated field on this sink. When enabled, a log producer that
    /// controls any field used in a template can write to arbitrary keys,
    /// paths, or routing destinations. This flag is a full opt-out: it
    /// disables confinement even for templates that have a usable static
    /// prefix.
    #[serde(default)]
    pub dangerously_allow_unconfined_template_resolution: bool,
}

#[cfg(test)]
mod tests;
