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
use derive_more::Display;
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
#[derive(Clone, Default, Display, PartialEq, Eq, Hash)]
#[display("{src}")]
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

/// A template that has passed through confinement via [`Template::confine`].
///
/// This is the only render-capable, confinement-enforcing template type. It is deliberately **not**
/// deserializable: the sole way to obtain one is by confining a [`Template`] or an
/// [`UnconfinedTemplate`], so a rendered value can never escape its confinement boundary. The
/// `render` methods enforce the attached confinement checker (if any).
///
/// Both fields are private to this module, so a `ConfinedTemplate` can
/// never be constructed (or deserialized) without going through confinement.
#[derive(Clone, Display, PartialEq, Eq, Hash)]
#[display("{inner}")]
pub struct ConfinedTemplate {
    inner: UnconfinedTemplate,
    checker: Option<ConfinementChecker>,
}

/// A template confined via [`UriTemplate::confine`], for HTTP/HTTPS URI fields.
///
/// Distinct from [`ConfinedTemplate`] so a prefix-confined template can never be
/// wired into a URI field. Not deserializable; the sole way to obtain one is
/// [`UriTemplate::confine`]. See [`ConfinedTemplate`] for the rendering contract.
pub use confined::ConfinedUriTemplate;

/// The templated field type stored in sink config structs for non-URI fields.
///
/// `Template` is serde-able and appears as a plain string in generated configuration schemas, but
/// it exposes **no** `render` method. To render it a sink must first call [`Template::confine`] in
/// its `build()`, which yields a [`ConfinedTemplate`] that enforces prefix confinement
/// (operator-controlled literal prefix + `..`-segment rejection) at render time. This makes
/// confinement unavoidable: there is no way to render a sink's configured template without going
/// through confinement first.
///
/// For HTTP/HTTPS URI fields, use [`UriTemplate`] instead. Transforms and sources, which have no
/// confinement boundary, should store [`UnconfinedTemplate`] directly instead.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct Template {
    inner: UnconfinedTemplate,
}

/// The templated field type for HTTP/HTTPS URI config fields (e.g., the HTTP sink's `uri`).
///
/// A wrapper around [`Template`] that selects **URI-specific confinement**: [`UriTemplate::confine`]
/// returns a [`ConfinedUriTemplate`], never a [`ConfinedTemplate`], so a URI field can never be
/// wired to a prefix-confined template and vice versa. It appears as a plain string in generated
/// configuration schemas, referencing `Template`'s schema definition.
///
/// There is deliberately no conversion from `Template`: a `UriTemplate` originates only from
/// string parsing, `Default`, or deserialization, so the URI-ness of a field is fixed by its
/// declared config type.
#[derive(Clone, Debug, Default, Display, PartialEq, Eq, Hash)]
#[configurable_component]
#[configurable(metadata(docs::templateable))]
pub struct UriTemplate(Template);

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
/// `dangerously_allow_unconfined_template_resolution` field. Pass it to
/// [`Template::confine`] on each template the component owns, or to
/// [`UriTemplate::confine`] for HTTP/HTTPS URI fields.
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
