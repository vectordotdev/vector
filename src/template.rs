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
    gauge,
    internal_event::GaugeName,
    lookup::lookup_v2::parse_target_path,
};

use crate::{
    config::log_schema,
    event::{EventRef, Metric, Value},
};

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
    #[snafu(display("rendered value {rendered:?} confined: {message}"))]
    Confined { rendered: String, message: String },
}

/// A templated field.
///
/// In many cases, components can be configured so that part of the component's functionality can be
/// customized on a per-event basis. For example, you have a sink that writes events to a file and you want to
/// specify which file an event should go to by using an event field as part of the
/// input to the filename used.
///
/// By using `Template`, users can specify either fixed strings or templated strings. Templated strings use a common syntax to
/// refer to fields in an event that is used as the input data when rendering the template. An example of a fixed string
/// is `my-file.log`. An example of a template string is `my-file-{{key}}.log`, where `{{key}}`
/// is the key's value when the template is rendered into a string.
#[configurable_component]
#[configurable(metadata(docs::templateable))]
#[derive(Clone, Default)]
#[serde(try_from = "String", into = "String")]
pub struct Template {
    src: String,

    #[serde(skip)]
    parts: Vec<Part>,

    #[serde(skip)]
    is_static: bool,

    #[serde(skip)]
    reserve_size: usize,

    #[serde(skip)]
    tz_offset: Option<FixedOffset>,

    /// Optional confinement check attached at build time by sinks that render
    /// templates into security-relevant identifiers (file paths, object-storage
    /// keys, HDFS paths, …). `render_string` runs this check after rendering
    /// and returns [`TemplateRenderingError::Confined`] if it fails.
    ///
    /// Skipped for serialization, hashing, and equality — two templates with
    /// the same source string are the same template regardless of whether a
    /// confinement hook has been attached.
    #[serde(skip)]
    confinement: Option<ConfinementChecker>,
}

impl fmt::Debug for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Template")
            .field("src", &self.src)
            .field("is_static", &self.is_static)
            .field("tz_offset", &self.tz_offset)
            .field("confinement", &self.confinement.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl PartialEq for Template {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src && self.tz_offset == other.tz_offset
    }
}

impl Eq for Template {}

impl Hash for Template {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.src.hash(state);
        self.tz_offset.hash(state);
    }
}

impl TryFrom<&str> for Template {
    type Error = TemplateParseError;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Borrowed(src))
    }
}

impl TryFrom<String> for Template {
    type Error = TemplateParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        Template::try_from(Cow::Owned(src))
    }
}

impl TryFrom<PathBuf> for Template {
    type Error = TemplateParseError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        Template::try_from(p.to_string_lossy().into_owned())
    }
}

impl TryFrom<Cow<'_, str>> for Template {
    type Error = TemplateParseError;

    fn try_from(src: Cow<'_, str>) -> Result<Self, Self::Error> {
        parse_template(&src).map(|parts| {
            let is_static =
                parts.is_empty() || (parts.len() == 1 && matches!(parts[0], Part::Literal(..)));

            // Calculate a minimum size to reserve for rendered string. This doesn't have to be
            // exact, and can't be because of references and time format specifiers. We just want a
            // better starting number than 0 to avoid the first reallocations if possible.
            let reserve_size = parts
                .iter()
                .map(|part| match part {
                    Part::Literal(lit) => lit.len(),
                    // We can't really put a useful number here, assume at least one byte will come
                    // from the input event.
                    Part::Reference(_path) => 1,
                    Part::Strftime(parsed) => parsed.reserve_size(),
                })
                .sum();

            Template {
                parts,
                src: src.into_owned(),
                is_static,
                reserve_size,
                tz_offset: None,
                confinement: None,
            }
        })
    }
}

impl From<Template> for String {
    fn from(template: Template) -> String {
        template.src
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.src.fmt(f)
    }
}

// This is safe because we literally defer to `String` for the schema of `Template`.
impl ConfigurableString for Template {}

impl Template {
    /// Set tz offset.
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.tz_offset = tz_offset;
        self
    }

    /// Attach a [`ConfinementChecker`] to this template.
    ///
    /// After rendering, `render_string` passes the result to the checker. On
    /// failure, `render_string` returns [`TemplateRenderingError::Confined`]
    /// and the caller should discard the event as an intentional security drop.
    ///
    /// Called by [`Template::confine`] at build time. Call sites that don't
    /// set a checker are unaffected — [`TemplateRenderingError::Confined`] can
    /// never be produced by a template with no checker attached.
    pub(crate) fn with_confinement_checker(mut self, checker: ConfinementChecker) -> Self {
        self.confinement = Some(checker);
        self
    }

    /// Confine this template to its literal prefix.
    ///
    /// Consumes `self` and returns it with a `PrefixChecker` attached that
    /// fires on every render. Three outcomes:
    ///
    /// - Static template (no event-field references) — returned unchanged.
    /// - Dynamic template with a non-empty literal prefix — checker attached.
    /// - Dynamic template with no derivable prefix — error, unless
    ///   `config.dangerously_allow_unconfined_template_resolution` is `true`,
    ///   in which case a `SECURITY` warning is emitted and `self` is returned
    ///   as-is.
    pub fn confine(
        self,
        config: &ConfinementConfig,
        component_name: &'static str,
        field_name: &'static str,
    ) -> crate::Result<Self> {
        match ConfinementChecker::for_template(&self) {
            Ok(Some(checker)) => {
                // Checker attached — template IS confined regardless of the opt-out flag.
                config.emit_confinement_gauge(true, "sink", component_name, field_name);
                Ok(self.with_confinement_checker(checker))
            }
            Ok(None) => {
                // Static template (no event-field references) — always safe.
                // Emit gauge=0 so a config reload from an unsafe dynamic template
                // to a static one resets any previously-set gauge=1.
                config.emit_confinement_gauge(true, "sink", component_name, field_name);
                Ok(self)
            }
            Err(_) if config.dangerously_allow_unconfined_template_resolution => {
                // No derivable base AND opt-out set — template is NOT confined.
                config.emit_confinement_gauge(false, "sink", component_name, field_name);
                Ok(self)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Renders the given template with data from the event, returning raw bytes.
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<Bytes, TemplateRenderingError> {
        self.render_string(event.into()).map(Into::into)
    }

    /// Renders the given template with data from the event.
    ///
    /// If a confinement check was attached at build time (see
    /// [`Template::confine`]),
    /// it runs after rendering. A confinement failure returns
    /// [`TemplateRenderingError::Confined`] — callers should emit an
    /// intentional-drop event and discard the event, not treat it as a
    /// field-missing error.
    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        let rendered = if self.is_static {
            self.src.clone()
        } else {
            self.render_event(event.into())?
        };
        if let Some(checker) = &self.confinement {
            checker
                .confine(&rendered)
                .map_err(|e| TemplateRenderingError::Confined {
                    rendered: rendered.clone(),
                    message: e.to_string(),
                })?;
        }
        Ok(rendered)
    }

    fn render_event(&self, event: EventRef<'_>) -> Result<String, TemplateRenderingError> {
        let mut missing_keys = Vec::new();
        let mut out = String::with_capacity(self.reserve_size);
        for part in &self.parts {
            match part {
                Part::Literal(lit) => out.push_str(lit),
                Part::Strftime(items) => {
                    out.push_str(&render_timestamp(items, event, self.tz_offset))
                }
                Part::Reference(key) => {
                    out.push_str(
                        &match event {
                            EventRef::Log(log) => log
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                            EventRef::Metric(metric) => {
                                render_metric_field(key, metric).map(Cow::Borrowed)
                            }
                            EventRef::Trace(trace) => trace
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                        }
                        .unwrap_or_else(|| {
                            missing_keys.push(key.to_owned());
                            Cow::Borrowed("")
                        }),
                    );
                }
            }
        }
        if missing_keys.is_empty() {
            Ok(out)
        } else {
            Err(TemplateRenderingError::MissingKeys { missing_keys })
        }
    }

    /// Returns the names of the fields that are rendered in this template.
    pub fn get_fields(&self) -> Option<Vec<String>> {
        let parts: Vec<_> = self
            .parts
            .iter()
            .filter_map(|part| {
                if let Part::Reference(r) = part {
                    Some(r.to_owned())
                } else {
                    None
                }
            })
            .collect();
        (!parts.is_empty()).then_some(parts)
    }

    /// Longest leading substring of the template source that is rendered
    /// verbatim — no `{{ field }}` reference and no strftime specifier.
    ///
    /// Sinks use this to derive a confinement boundary from the
    /// operator-authored portion of the template.
    pub fn literal_prefix(&self) -> &str {
        let bytes = self.src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            // `{{` starts a field reference.
            if bytes[i] == b'{' && bytes.get(i + 1) == Some(&b'{') {
                break;
            }
            // Any `%` may start a strftime sequence. `%%` is an escaped `%`,
            // but in a mixed literal like `/tmp/100%%/%Y/` the whole segment
            // is processed by chrono, which decodes `%%` to `%` and expands
            // `%Y` to the year. We cannot know what chrono will emit without
            // an actual timestamp, so stop at the first `%` unconditionally.
            if bytes[i] == b'%' {
                break;
            }
            i += 1;
        }
        self.src.split_at(i).0
    }

    #[allow(clippy::missing_const_for_fn)] // Adding `const` results in https://doc.rust-lang.org/error_codes/E0015.html
    /// Returns a reference to the template string.
    pub fn get_ref(&self) -> &str {
        &self.src
    }

    /// Returns `true` if this template string has a length of zero, and `false` otherwise.
    pub const fn is_empty(&self) -> bool {
        self.src.is_empty()
    }

    /// A dynamic template string contains sections that depend on the input event or time.
    pub const fn is_dynamic(&self) -> bool {
        !self.is_static
    }
}

/// The source of a `uint` template. May be a constant numeric value or a template string.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[configurable_component]
#[serde(untagged)]
enum UnsignedIntTemplateSource {
    /// A static unsigned number.
    Number(u64),
    /// A string, which may be a template.
    String(String),
}

impl Default for UnsignedIntTemplateSource {
    fn default() -> Self {
        Self::Number(Default::default())
    }
}

impl fmt::Display for UnsignedIntTemplateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(i) => i.fmt(f),
            Self::String(s) => s.fmt(f),
        }
    }
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

impl TryFrom<UnsignedIntTemplateSource> for UnsignedIntTemplate {
    type Error = TemplateParseError;

    fn try_from(src: UnsignedIntTemplateSource) -> Result<Self, Self::Error> {
        match src {
            UnsignedIntTemplateSource::Number(num) => Ok(UnsignedIntTemplate {
                src: UnsignedIntTemplateSource::Number(num),
                parts: Vec::new(),
                tz_offset: None,
            }),
            UnsignedIntTemplateSource::String(s) => UnsignedIntTemplate::try_from(s),
        }
    }
}

impl From<UnsignedIntTemplate> for UnsignedIntTemplateSource {
    fn from(template: UnsignedIntTemplate) -> UnsignedIntTemplateSource {
        template.src
    }
}

impl TryFrom<&str> for UnsignedIntTemplate {
    type Error = TemplateParseError;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        UnsignedIntTemplate::try_from(Cow::Borrowed(src))
    }
}

impl TryFrom<String> for UnsignedIntTemplate {
    type Error = TemplateParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        UnsignedIntTemplate::try_from(Cow::Owned(src))
    }
}

impl From<u64> for UnsignedIntTemplate {
    fn from(num: u64) -> UnsignedIntTemplate {
        UnsignedIntTemplate {
            src: UnsignedIntTemplateSource::Number(num),
            parts: Vec::new(),
            tz_offset: None,
        }
    }
}

impl TryFrom<Cow<'_, str>> for UnsignedIntTemplate {
    type Error = TemplateParseError;

    fn try_from(src: Cow<'_, str>) -> Result<Self, Self::Error> {
        parse_template(&src).and_then(|parts| {
            let is_static =
                parts.is_empty() || (parts.len() == 1 && matches!(parts[0], Part::Literal(..)));

            if is_static {
                match src.parse::<u64>() {
                    Ok(num) => Ok(UnsignedIntTemplate {
                        src: UnsignedIntTemplateSource::Number(num),
                        parts,
                        tz_offset: None,
                    }),
                    Err(_) => Err(TemplateParseError::InvalidNumericTemplate {
                        template: src.into_owned(),
                    }),
                }
            } else {
                Ok(UnsignedIntTemplate {
                    parts,
                    src: UnsignedIntTemplateSource::String(src.into_owned()),
                    tz_offset: None,
                })
            }
        })
    }
}

impl From<UnsignedIntTemplate> for String {
    fn from(template: UnsignedIntTemplate) -> String {
        template.src.to_string()
    }
}

impl fmt::Display for UnsignedIntTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.src.fmt(f)
    }
}

impl ConfigurableString for UnsignedIntTemplate {}
impl ConfigurableNumber for UnsignedIntTemplate {
    type Numeric = u64;

    fn class() -> NumberClass {
        NumberClass::Unsigned
    }
}

impl UnsignedIntTemplate {
    /// Renders the given template with data from the event.
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<u64, TemplateRenderingError> {
        match self.src {
            UnsignedIntTemplateSource::Number(num) => Ok(num),
            UnsignedIntTemplateSource::String(_) => self.render_event(event.into()),
        }
    }

    /// set tz offset
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.tz_offset = tz_offset;
        self
    }

    fn render_event(&self, event: EventRef<'_>) -> Result<u64, TemplateRenderingError> {
        let mut missing_keys = Vec::new();
        let mut out = String::with_capacity(20);
        for part in &self.parts {
            match part {
                Part::Literal(lit) => out.push_str(lit),
                Part::Reference(key) => {
                    out.push_str(
                        &match event {
                            EventRef::Log(log) => log
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                            EventRef::Metric(metric) => {
                                render_metric_field(key, metric).map(Cow::Borrowed)
                            }
                            EventRef::Trace(trace) => trace
                                .parse_path_and_get_value(key)
                                .ok()
                                .and_then(|v| v.map(Value::to_string_lossy)),
                        }
                        .unwrap_or_else(|| {
                            missing_keys.push(key.to_owned());
                            Cow::Borrowed("")
                        }),
                    );
                }
                Part::Strftime(items) => {
                    out.push_str(&render_timestamp(items, event, self.tz_offset))
                }
            }
        }
        if missing_keys.is_empty() {
            out.parse::<u64>()
                .map_err(|_| TemplateRenderingError::NotNumeric { input: out })
        } else {
            Err(TemplateRenderingError::MissingKeys { missing_keys })
        }
    }

    /// Returns the names of the fields that are rendered in this template.
    pub fn get_fields(&self) -> Option<Vec<String>> {
        let parts: Vec<_> = self
            .parts
            .iter()
            .filter_map(|part| {
                if let Part::Reference(r) = part {
                    Some(r.to_owned())
                } else {
                    None
                }
            })
            .collect();
        (!parts.is_empty()).then_some(parts)
    }
}

/// One part of the template string after parsing.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Part {
    /// A literal piece of text to be copied verbatim into the output.
    Literal(String),
    /// A literal piece of text containing a time format string.
    Strftime(ParsedStrftime),
    /// A reference to the source event, to be copied from the relevant field or tag.
    Reference(String),
}

// Wrap the parsed time formatter in order to provide `impl Hash` and some convenience functions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ParsedStrftime(Box<[Item<'static>]>);

impl ParsedStrftime {
    fn parse(fmt: &str) -> Result<Self, TemplateParseError> {
        Ok(Self(
            StrftimeItems::new(fmt)
                .map(|item| match item {
                    // Box the references so they outlive the reference
                    Item::Space(space) => Item::OwnedSpace(space.into()),
                    Item::Literal(lit) => Item::OwnedLiteral(lit.into()),
                    // And copy all the others
                    Item::Fixed(f) => Item::Fixed(f),
                    Item::Numeric(num, pad) => Item::Numeric(num, pad),
                    Item::Error => Item::Error,
                    Item::OwnedSpace(space) => Item::OwnedSpace(space),
                    Item::OwnedLiteral(lit) => Item::OwnedLiteral(lit),
                })
                .map(|item| {
                    matches!(item, Item::Error)
                        .then(|| Err(TemplateParseError::StrftimeError))
                        .unwrap_or(Ok(item))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ))
    }

    fn is_dynamic(&self) -> bool {
        self.0.iter().any(|item| match item {
            Item::Fixed(_) => true,
            Item::Numeric(_, _) => true,
            Item::Error
            | Item::Space(_)
            | Item::OwnedSpace(_)
            | Item::Literal(_)
            | Item::OwnedLiteral(_) => false,
        })
    }

    fn as_items(&self) -> impl Iterator<Item = &Item<'static>> + Clone {
        self.0.iter()
    }

    fn reserve_size(&self) -> usize {
        self.0
            .iter()
            .map(|item| match item {
                Item::Literal(lit) => lit.len(),
                Item::OwnedLiteral(lit) => lit.len(),
                Item::Space(space) => space.len(),
                Item::OwnedSpace(space) => space.len(),
                Item::Error => 0,
                Item::Numeric(_, _) => 2,
                Item::Fixed(_) => 2,
            })
            .sum()
    }
}

fn parse_literal(src: &str) -> Result<Part, TemplateParseError> {
    let parsed = ParsedStrftime::parse(src)?;
    Ok(if parsed.is_dynamic() {
        Part::Strftime(parsed)
    } else {
        Part::Literal(src.to_string())
    })
}

// Pre-parse the template string into a series of parts to be filled in at render time.
fn parse_template(src: &str) -> Result<Vec<Part>, TemplateParseError> {
    let mut last_end = 0;
    let mut parts = Vec::new();
    for cap in RE.captures_iter(src) {
        let all = cap.get(0).expect("Capture 0 is always defined");
        if all.start() > last_end {
            #[expect(
                clippy::string_slice,
                reason = "indices come from regex match positions, always char boundaries"
            )]
            parts.push(parse_literal(&src[last_end..all.start()])?);
        }

        let path = cap[1].trim().to_owned();

        // This checks the syntax, but doesn't yet store it for use later
        // see: https://github.com/vectordotdev/vector/issues/14864
        if parse_target_path(&path).is_err() {
            return Err(TemplateParseError::InvalidPathSyntax { path });
        }

        parts.push(Part::Reference(path));
        last_end = all.end();
    }
    if src.len() > last_end {
        #[expect(
            clippy::string_slice,
            reason = "last_end comes from a regex match end position, always a char boundary"
        )]
        parts.push(parse_literal(&src[last_end..])?);
    }

    Ok(parts)
}

fn render_metric_field<'a>(key: &str, metric: &'a Metric) -> Option<&'a str> {
    match key {
        "name" => Some(metric.name()),
        "namespace" => metric.namespace(),
        _ if let Some(tag_key) = key.strip_prefix("tags.") => {
            metric.tags().and_then(|tags| tags.get(tag_key))
        }
        _ => None,
    }
}

fn render_timestamp(
    items: &ParsedStrftime,
    event: EventRef<'_>,
    tz_offset: Option<FixedOffset>,
) -> String {
    let timestamp = match event {
        EventRef::Log(log) => log.get_timestamp().and_then(Value::as_timestamp).copied(),
        EventRef::Metric(metric) => metric.timestamp(),
        EventRef::Trace(trace) => {
            log_schema()
                .timestamp_key_target_path()
                .and_then(|timestamp_key| {
                    trace
                        .get(timestamp_key)
                        .and_then(Value::as_timestamp)
                        .copied()
                })
        }
    }
    .unwrap_or_else(Utc::now);

    match tz_offset {
        Some(offset) => timestamp
            .with_timezone(&offset)
            .format_with_items(items.as_items())
            .to_string(),
        None => timestamp
            .with_timezone(&chrono::Utc)
            .format_with_items(items.as_items())
            .to_string(),
    }
}

use crate::sinks::util::path_confinement::MAX_RENDERED_PATH_LEN;

#[derive(Debug, Snafu)]
#[snafu(module(build_error))]
pub(crate) enum BuildError {
    /// Template has event-field references but no literal prefix to confine them to.
    #[snafu(display(
        "template references event fields ({fields:?}) but has no \
         literal string prefix to derive a confinement base from. Add a static \
         prefix to your template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    NoDerivableBase {
        /// The event fields referenced by the template.
        fields: Vec<String>,
    },

    /// The only derivable prefix is a bare root (`/`), which would allow writes
    /// to any path under the server's namespace root.
    #[snafu(display(
        "template has only `\"/\"` as its literal prefix (from {prefix:?}), \
         which would permit writes anywhere in the namespace root. Add a \
         non-root static prefix to your template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    DerivedBaseIsRoot {
        /// The literal prefix that resolved to root.
        prefix: String,
    },

    /// The template is an HTTP/HTTPS URI but the static prefix ends before the
    /// authority (host + optional port), so the rendered URL's destination host
    /// is entirely event-controlled. Supply a static scheme + host, or set
    /// `dangerously_allow_unconfined_template_resolution: true` to opt out.
    #[snafu(display(
        "HTTP/HTTPS template {prefix:?} has no static authority (host): the \
         destination host would be fully event-controlled. Add a static host to \
         your URI template, or set \
         `dangerously_allow_unconfined_template_resolution: true` to opt out."
    ))]
    NoStaticUriAuthority {
        /// The literal prefix that contained no host.
        prefix: String,
    },
}

#[derive(Debug, Snafu)]
#[snafu(module(confine_error))]
pub(crate) enum ConfineError {
    /// Rendered value contains a NUL byte.
    #[snafu(display("rendered value contains a NUL byte"))]
    NulByte,

    /// Rendered value exceeds the maximum allowed byte length.
    #[snafu(display("rendered value is {len} bytes; maximum allowed is {max}"))]
    TooLong {
        /// Actual length of the rendered value in bytes.
        len: usize,
        /// Maximum allowed length in bytes.
        max: usize,
    },

    /// Rendered value does not start with the required base prefix.
    #[snafu(display("rendered value {rendered:?} does not start with the base prefix {base:?}"))]
    OutsideBase {
        /// The rendered value that failed confinement.
        rendered: String,
        /// The required base prefix.
        base: String,
    },

    /// Rejected because a `..` segment could escape the namespace root on
    /// filesystem-like protocols (e.g. WebHDFS) even when the string prefix
    /// check passes (e.g. `safe/../../escape` starts with `safe/`).
    #[snafu(display("rendered value {rendered:?} contains a `..` path segment"))]
    DotDotSegment {
        /// The rendered value that contained the `..` segment.
        rendered: String,
    },

    /// Rendered URI could not be parsed.
    #[snafu(display("rendered value {rendered:?} is not a valid URI"))]
    UriParseFailed {
        /// The rendered value that could not be parsed.
        rendered: String,
    },

    /// Rendered URI has a different scheme or authority (host + port) than the
    /// operator-configured base. This covers both `@`-userinfo injection
    /// (`trusted.host@evil.com`) and host-extension attacks
    /// (`trusted.host.evil.com`).
    #[snafu(display(
        "rendered URI {rendered:?} has authority {actual:?} but the confined base \
         requires {expected:?}"
    ))]
    UriAuthorityMismatch {
        /// The rendered value that failed confinement.
        rendered: String,
        /// The authority that was required.
        expected: String,
        /// The authority that was actually present.
        actual: String,
    },
}

/// Confinement checker stored on a [`Template`] at build time.
///
/// Dispatches to `PrefixChecker` for non-URI fields (Kafka topics, Redis
/// keys, tenant IDs, …) or `UriChecker` for HTTP/HTTPS URI fields. Common
/// guards (NUL bytes, length limit) run before dispatching.
#[derive(Clone, Debug)]
pub(crate) enum ConfinementChecker {
    Prefix(PrefixChecker),
    Uri(UriChecker),
}

impl ConfinementChecker {
    pub(crate) fn for_template(tpl: &Template) -> Result<Option<Self>, BuildError> {
        let fields = match tpl.get_fields() {
            Some(f) => f,
            None => return Ok(None),
        };
        let prefix = tpl.literal_prefix();
        if prefix.is_empty() {
            return Err(BuildError::NoDerivableBase { fields });
        }
        if prefix == "/" {
            return Err(BuildError::DerivedBaseIsRoot {
                prefix: prefix.to_string(),
            });
        }
        // Only treat as a URI if the prefix literally starts with http:// or https://.
        // Testing for the scheme token alone (without "://") would misfire on
        // templates like `http_{{tenant}}` whose prefix is `http_`.
        let lp = prefix.to_ascii_lowercase();
        if lp.starts_with("http://") || lp.starts_with("https://") {
            UriChecker::from_prefix(prefix).map(|c| Some(Self::Uri(c)))
        } else {
            Ok(Some(Self::Prefix(PrefixChecker {
                base: prefix.to_string(),
            })))
        }
    }

    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        if rendered.contains('\0') {
            return Err(ConfineError::NulByte);
        }
        if rendered.len() > MAX_RENDERED_PATH_LEN {
            return Err(ConfineError::TooLong {
                len: rendered.len(),
                max: MAX_RENDERED_PATH_LEN,
            });
        }
        match self {
            Self::Prefix(c) => c.confine(rendered),
            Self::Uri(c) => c.confine(rendered),
        }
    }
}

/// Confinement for non-URI templates (Kafka topics, Redis keys, tenant IDs, …).
///
/// Enforces that the rendered value starts with the operator-controlled literal
/// prefix and contains no `..` path segments.
#[derive(Clone, Debug)]
pub(crate) struct PrefixChecker {
    base: String,
}

impl PrefixChecker {
    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        // Reject `..` segments: on filesystem-like protocols (e.g. WebHDFS) a
        // value like `safe/../../escape` passes `starts_with("safe/")` but
        // resolves outside the namespace root on the server.
        if rendered.split('/').any(|seg| seg == "..") {
            return Err(ConfineError::DotDotSegment {
                rendered: rendered.to_string(),
            });
        }
        if !rendered.starts_with(&self.base) {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: self.base.clone(),
            });
        }
        Ok(())
    }
}

/// Confinement for HTTP/HTTPS URI templates.
///
/// At build time the operator-authored static prefix is parsed with
/// `http::Uri` and its scheme, authority, and path are stored normalised
/// (lowercased). At render time the rendered value is also parsed with
/// `http::Uri` and the structured fields are compared, which avoids all the
/// pitfalls of raw-string heuristics (case sensitivity, percent-encoding,
/// `@`-injection inside the authority component, etc.).
///
/// Three checks run on every render:
///
/// 1. **Authority check** — the rendered URI's scheme and authority must match
///    the operator-authored values exactly. This catches `@`-userinfo injection
///    (`trusted.host@evil.com`) and host-extension attacks
///    (`trusted.host.evil.com`).
///
/// 2. **Path-prefix check** — the rendered URI's path must start with the
///    static path portion derived from the template prefix.
///
/// 3. **Dot-dot segment check** — no path segment may be `..`, `.%2e`,
///    `%2e.`, or `%2e%2e` (case-insensitive). This catches path traversal
///    within the same host even when the prefix check passes.
#[derive(Clone, Debug)]
pub(crate) struct UriChecker {
    /// Lowercased scheme, e.g. `"https"`.
    scheme: String,
    /// Lowercased authority (host + optional port), e.g. `"api.internal"`.
    authority: String,
    /// Static path portion from the template prefix, e.g. `"/ingest/"`.
    path_prefix: String,
}

impl UriChecker {
    pub(crate) fn from_prefix(prefix: &str) -> Result<Self, BuildError> {
        let uri = prefix
            .parse::<Uri>()
            .map_err(|_| BuildError::NoStaticUriAuthority {
                prefix: prefix.to_string(),
            })?;
        let scheme = uri
            .scheme_str()
            .expect("scheme present because prefix starts with http(s)://")
            .to_ascii_lowercase();
        match uri.authority() {
            Some(auth) if !auth.as_str().is_empty() => Ok(Self {
                scheme,
                authority: auth.as_str().to_ascii_lowercase(),
                path_prefix: uri.path().to_string(),
            }),
            _ => Err(BuildError::NoStaticUriAuthority {
                prefix: prefix.to_string(),
            }),
        }
    }

    pub(crate) fn confine(&self, rendered: &str) -> Result<(), ConfineError> {
        // Parse with http::Uri so all structural checks use the same tokeniser
        // that built the baseline — no raw-string heuristics.
        let uri = rendered
            .parse::<Uri>()
            .map_err(|_| ConfineError::UriParseFailed {
                rendered: rendered.to_string(),
            })?;

        // 1. Authority check: scheme + host must exactly match the base.
        //    Catches @-userinfo injection and host-extension attacks.
        //    Both sides are lowercased so the comparison is case-insensitive.
        let actual_scheme = uri.scheme_str().unwrap_or("").to_ascii_lowercase();
        let actual_authority = uri
            .authority()
            .map(|a| a.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        if actual_scheme != self.scheme || actual_authority != self.authority {
            return Err(ConfineError::UriAuthorityMismatch {
                rendered: rendered.to_string(),
                expected: format!("{}://{}", self.scheme, self.authority),
                actual: format!("{actual_scheme}://{actual_authority}"),
            });
        }

        // 2. Path-prefix check: catches path escape when the template includes
        //    a static path (e.g. `https://api.internal/ingest/{{ tenant }}`).
        let path = uri.path();
        if !path.starts_with(&self.path_prefix) {
            return Err(ConfineError::OutsideBase {
                rendered: rendered.to_string(),
                base: format!("{}://{}{}", self.scheme, self.authority, self.path_prefix),
            });
        }

        // 3. Dot-dot segment check: catches within-prefix path traversal.
        //    Also rejects percent-encoded variants that some servers decode
        //    before resolving the path (e.g. `/ingest/%2e%2e/admin`).
        for segment in path.split('/') {
            if segment == ".."
                || segment.eq_ignore_ascii_case("%2e%2e")
                || segment.eq_ignore_ascii_case(".%2e")
                || segment.eq_ignore_ascii_case("%2e.")
            {
                return Err(ConfineError::DotDotSegment {
                    rendered: rendered.to_string(),
                });
            }
        }

        // 4. Reject percent-encoded path separators: `%2f` is a literal `/` per
        //    RFC 3986, but many servers decode it before path normalization.
        //    `%5c` (backslash) is similarly decoded on Windows-backed services.
        //    Either can turn an otherwise-safe segment into a traversal vector,
        //    e.g. `%2e%2e%2fadmin` is one raw segment but resolves as `../admin`.
        let path_lc = path.to_ascii_lowercase();
        if path_lc.contains("%2f") || path_lc.contains("%5c") {
            return Err(ConfineError::DotDotSegment {
                rendered: rendered.to_string(),
            });
        }

        Ok(())
    }
}

/// Serializable config fragment for template confinement.
///
/// Embed this in a component config with `#[serde(flatten)]` to get the
/// `dangerously_allow_unconfined_template_resolution` field. Pass it to
/// [`Template::confine`] on each template the component owns.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct ConfinementConfig {
    /// Disable template confinement when no static prefix can be derived.
    ///
    /// **DANGEROUS — disables a security control.**
    ///
    /// Suppresses the startup error when a template references event fields
    /// but has no static literal prefix to derive a confinement base from.
    /// When enabled, a log producer that controls any field used in the
    /// template can write to arbitrary keys or paths.
    #[serde(default)]
    pub dangerously_allow_unconfined_template_resolution: bool,
}

impl ConfinementConfig {
    /// Emit the `vector_security_confinement_disabled` gauge.
    ///
    /// Call this on every confinement code-path (both confined and opt-out) for
    /// templates that have dynamic fields, so that a config reload which removes
    /// `dangerously_allow_unconfined_template_resolution` resets the gauge to 0.
    ///
    /// `actually_confined` must be `true` when a checker was successfully
    /// attached (gauge → 0) and `false` when the opt-out suppressed an error
    /// and no checker was attached (gauge → 1, warn emitted).
    pub fn emit_confinement_gauge(
        &self,
        actually_confined: bool,
        component_kind: &'static str,
        component_type: &'static str,
        field: &'static str,
    ) {
        let value = if !actually_confined {
            warn!(
                message = "SECURITY: component has `dangerously_allow_unconfined_template_resolution` \
                           enabled — template is NOT confined. A log producer that controls any \
                           field used in the template can write to arbitrary keys.",
                component_kind, component_type, field,
            );
            1.0
        } else {
            0.0
        };
        gauge!(
            GaugeName::SecurityConfinementDisabled,
            "component_kind" => component_kind,
            "component_type" => component_type,
            "field" => field,
        )
        .set(value);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Offset, TimeZone, Utc};
    use chrono_tz::Tz;
    use vector_lib::{
        config::LogNamespace,
        lookup::{PathPrefix, metadata_path},
        metric_tags,
    };
    use vrl::event_path;

    use super::*;
    use crate::event::{Event, LogEvent, MetricKind, MetricValue};

    #[test]
    fn get_fields() {
        let f1 = Template::try_from("{{ foo }}")
            .unwrap()
            .get_fields()
            .unwrap();
        let f2 = Template::try_from("{{ foo }}-{{ bar }}")
            .unwrap()
            .get_fields()
            .unwrap();
        let f3 = Template::try_from("nofield").unwrap().get_fields();
        let f4 = Template::try_from("%F").unwrap().get_fields();
        let f5 = UnsignedIntTemplate::try_from("{{ foo }}-{{ bar }}")
            .unwrap()
            .get_fields()
            .unwrap();
        let f6 = UnsignedIntTemplate::from(123u64).get_fields();
        let f7 = UnsignedIntTemplate::try_from("%s").unwrap().get_fields();

        assert_eq!(f1, vec!["foo"]);
        assert_eq!(f2, vec!["foo", "bar"]);
        assert_eq!(f3, None);
        assert_eq!(f4, None);
        assert_eq!(f5, vec!["foo", "bar"]);
        assert_eq!(f6, None);
        assert_eq!(f7, None);
    }

    #[test]
    fn literal_prefix() {
        let cases = [
            ("/var/log/app.log", "/var/log/app.log"),
            ("/var/log/{{ host }}/app.log", "/var/log/"),
            ("/var/log/%Y/{{ host }}.log", "/var/log/"),
            ("/srv-{{ id }}.log", "/srv-"),
            ("{{ full_path }}", ""),
            ("/{{ tenant }}/app.log", "/"),
            // `%%` stops the prefix scan — in mixed segments like
            // `100%%/%Y/{{ x }}` chrono decodes `%%` while expanding `%Y`,
            // so we cannot determine the rendered prefix without a timestamp.
            ("100%%-literal/{{ x }}", "100"),
            ("no-template-at-all", "no-template-at-all"),
            ("only-strftime-%F.log", "only-strftime-"),
            // single `{` is not a field opener
            ("a{b/{{ c }}", "a{b/"),
        ];
        for (src, expected) in cases {
            let tpl = Template::try_from(src).unwrap();
            assert_eq!(tpl.literal_prefix(), expected, "src = {src:?}");
        }
    }

    #[test]
    fn is_dynamic() {
        assert!(Template::try_from("/kube-demo/%F").unwrap().is_dynamic());
        assert!(!Template::try_from("/kube-demo/echo").unwrap().is_dynamic());
        assert!(
            Template::try_from("/kube-demo/{{ foo }}")
                .unwrap()
                .is_dynamic()
        );
        assert!(
            Template::try_from("/kube-demo/{{ foo }}/%F")
                .unwrap()
                .is_dynamic()
        );
    }

    #[test]
    fn render_log_static() {
        let event = Event::Log(LogEvent::from("hello world"));
        let template = Template::try_from("foo").unwrap();

        assert_eq!(Ok(Bytes::from("foo")), template.render(&event))
    }

    #[test]
    fn render_log_unsigned_number() {
        let event = Event::Log(LogEvent::from("hello world"));
        let template = UnsignedIntTemplate::from(123);

        assert_eq!(Ok(123), template.render(&event))
    }

    #[test]
    fn render_log_unsigned_number_dynamic() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("foo"), 123);

        let template = UnsignedIntTemplate::try_from("{{ foo }}").unwrap();
        assert_eq!(Ok(123), template.render(&event))
    }

    #[test]
    fn render_log_dynamic() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(event_path!("log_stream"), "stream");
        let template = Template::try_from("{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("stream")), template.render(&event))
    }

    #[test]
    fn render_log_metadata() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(metadata_path!("metadata_key"), "metadata_value");
        let template = Template::try_from("{{%metadata_key}}").unwrap();

        assert_eq!(Ok(Bytes::from("metadata_value")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_prefix() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(event_path!("log_stream"), "stream");
        let template = Template::try_from("abcd-{{log_stream}}").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-stream")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_with_postfix() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(event_path!("log_stream"), "stream");
        let template = Template::try_from("{{log_stream}}-abcd").unwrap();

        assert_eq!(Ok(Bytes::from("stream-abcd")), template.render(&event))
    }

    #[test]
    fn render_log_dynamic_missing_key() {
        let event = Event::Log(LogEvent::from("hello world"));
        let template = Template::try_from("{{log_stream}}-{{foo}}").unwrap();

        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["log_stream".to_string(), "foo".to_string()]
            }),
            template.render(&event)
        );
    }

    #[test]
    fn render_log_dynamic_multiple_keys() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("foo"), "bar");
        event.as_mut_log().insert(event_path!("baz"), "quux");
        let template = Template::try_from("stream-{{foo}}-{{baz}}.log").unwrap();

        assert_eq!(
            Ok(Bytes::from("stream-bar-quux.log")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_weird_junk() {
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("foo"), "bar");
        event.as_mut_log().insert(event_path!("baz"), "quux");
        let template = Template::try_from(r"{stream}{\{{}}}-{{foo}}-{{baz}}.log").unwrap();

        assert_eq!(
            Ok(Bytes::from(r"{stream}{\{{}}}-bar-quux.log")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_timestamp_strftime_style() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

        let template = Template::try_from("abcd-%F").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_log_timestamp_strftime_style_namespace() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("@timestamp"), ts);
        // use Vector namespace instead of legacy
        LogNamespace::Vector.insert_vector_metadata(
            event.as_mut_log(),
            Some(vrl::path!("foo")),
            vrl::path!("foo"),
            "bar",
        );
        let new_schema = event
            .as_mut_log()
            .metadata()
            .schema_definition()
            .as_ref()
            .clone()
            .with_meaning(parse_target_path("@timestamp").unwrap(), "timestamp");
        event
            .as_mut_log()
            .metadata_mut()
            .set_schema_definition(&std::sync::Arc::new(new_schema));

        let template = Template::try_from("abcd-%F").unwrap();

        assert_eq!(Ok(Bytes::from("abcd-2001-02-03")), template.render(&event))
    }

    #[test]
    fn render_log_timestamp_multiple_strftime_style() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key_target_path().unwrap(), ts);

        let template = Template::try_from("abcd-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("abcd-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("foo"), "butts");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("{{ foo }}-%F_%T").unwrap();

        assert_eq!(
            Ok(Bytes::from("butts-2001-02-03_04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_nested_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("format"), "%F");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("nested {{ format }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested %F 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_log_dynamic_with_reverse_nested_strftime() {
        let ts = Utc
            .with_ymd_and_hms(2001, 2, 3, 4, 5, 6)
            .single()
            .expect("invalid timestamp");

        let mut event = Event::Log(LogEvent::from("hello world"));
        event
            .as_mut_log()
            .insert(&parse_target_path("\"%F\"").unwrap(), "foo");
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let template = Template::try_from("nested {{ \"%F\" }} %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("nested foo 04:05:06")),
            template.render(&event)
        )
    }

    #[test]
    fn render_metric_timestamp() {
        let template = Template::try_from("timestamp %F %T").unwrap();

        assert_eq!(
            Ok(Bytes::from("timestamp 2002-03-04 05:06:07")),
            template.render(&sample_metric())
        );
    }

    #[test]
    fn render_metric_with_tags() {
        let template = Template::try_from("name={{name}} component={{tags.component}}").unwrap();
        let metric = sample_metric().with_tags(Some(metric_tags!(
            "test" => "true",
            "component" => "template",
        )));
        assert_eq!(
            Ok(Bytes::from("name=a-counter component=template")),
            template.render(&metric)
        );
    }

    #[test]
    fn render_metric_without_tags() {
        let template = Template::try_from("name={{name}} component={{tags.component}}").unwrap();
        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["tags.component".into()]
            }),
            template.render(&sample_metric())
        );
    }

    #[test]
    fn render_metric_with_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric().with_namespace(Some("vector-test"));
        assert_eq!(
            Ok(Bytes::from("namespace=vector-test name=a-counter")),
            template.render(&metric)
        );
    }

    #[test]
    fn render_metric_without_namespace() {
        let template = Template::try_from("namespace={{namespace}} name={{name}}").unwrap();
        let metric = sample_metric();
        assert_eq!(
            Err(TemplateRenderingError::MissingKeys {
                missing_keys: vec!["namespace".into()]
            }),
            template.render(&metric)
        );
    }

    #[test]
    fn render_log_with_timezone() {
        let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

        let template = Template::try_from("vector-%Y-%m-%d-%H.log").unwrap();
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            ts,
        );

        let tz: Tz = "Asia/Singapore".parse().unwrap();
        let offset = Some(Utc::now().with_timezone(&tz).offset().fix());
        assert_eq!(
            Ok(Bytes::from("vector-2001-02-03-12.log")),
            template.with_tz_offset(offset).render(&event)
        );
    }

    #[test]
    fn render_log_unsigned_int_with_timezone() {
        let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

        let template = UnsignedIntTemplate::try_from("%Y%m%d%H").unwrap();
        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("timestamp"), ts);

        let tz: Tz = "Asia/Singapore".parse().unwrap();
        let offset = Some(Utc::now().with_timezone(&tz).offset().fix());

        assert_eq!(
            Ok(2001020312),
            template.with_tz_offset(offset).render(&event)
        );
    }

    fn sample_metric() -> Metric {
        Metric::new(
            "a-counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.1 },
        )
        .with_timestamp(Some(
            Utc.with_ymd_and_hms(2002, 3, 4, 5, 6, 7)
                .single()
                .expect("invalid timestamp"),
        ))
    }

    #[test]
    fn strftime_error() {
        assert_eq!(
            Template::try_from("%E").unwrap_err(),
            TemplateParseError::StrftimeError
        );
    }

    #[test]
    fn strftime_non_int_result() {
        let template = UnsignedIntTemplate::try_from("a-%s").unwrap();
        let ts = Utc.with_ymd_and_hms(2001, 2, 3, 4, 5, 6).unwrap();

        let mut event = Event::Log(LogEvent::from("hello world"));
        event.as_mut_log().insert(event_path!("timestamp"), ts);

        assert_eq!(
            Err(TemplateRenderingError::NotNumeric {
                input: "a-981173106".to_owned()
            }),
            template.render(&event)
        );
    }

    #[test]
    fn dotdot_bypass_rejected() {
        // `safe/../../escape` passes a naive starts_with("safe/") check but must
        // be rejected — on filesystem-like protocols (WebHDFS) the server resolves
        // `..` and the value escapes the intended namespace.
        let tpl = Template::try_from("safe/{{ tenant }}/").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

        assert!(c.confine("safe/legit/").is_ok());
        assert!(matches!(
            c.confine("safe/../../escape/").unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("safe/../escape/").unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        // `..` only as a literal substring (not a full segment) is fine
        assert!(c.confine("safe/not..dotdot/").is_ok());
    }

    #[test]
    fn uri_path_traversal_rejected() {
        // `https://api.internal/ingest/{{ tenant }}` with `../../admin` passes the
        // authority check (same host) but must be caught by the path-prefix +
        // `..` checks.
        let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

        assert!(c.confine("https://api.internal/ingest/acme").is_ok());
        assert!(matches!(
            c.confine("https://api.internal/ingest/../../admin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        // Path that doesn't start with /ingest/ is also rejected.
        assert!(matches!(
            c.confine("https://api.internal/admin/secret").unwrap_err(),
            ConfineError::OutsideBase { .. }
        ));
    }

    #[test]
    fn uri_query_only_suffix_accepted() {
        // `https://host{{ query }}` with `query = "?x=1"` should pass.
        let tpl = Template::try_from("https://api.internal{{ query }}").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
        assert!(c.confine("https://api.internal?x=1").is_ok());
        // http::Uri silently strips fragments per RFC 7230 (fragments are not sent
        // to servers), so the fragment is invisible to the authority + path checks
        // and the rendered value is accepted.
        assert!(c.confine("https://api.internal#fragment").is_ok());
    }

    #[test]
    fn uri_percent_encoded_dotdot_rejected() {
        // Servers that decode percent-encoding before path resolution can be
        // tricked by `%2e%2e` instead of `..`.  All encoded variants must be
        // rejected alongside the literal `..`.
        let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

        assert!(c.confine("https://api.internal/ingest/legit").is_ok());
        assert!(matches!(
            c.confine("https://api.internal/ingest/%2e%2e/admin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/%2E%2E/admin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/.%2e/admin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/%2e./admin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
    }

    #[test]
    fn uri_encoded_slash_traversal_rejected() {
        // `%2f` is a percent-encoded `/`. RFC 3986 treats it as a literal slash
        // character inside a segment, not a path separator — so `http::Uri` keeps
        // `%2e%2e%2fadmin` as a single segment and the segment-level dot-dot checks
        // alone would miss it. Many HTTP servers decode `%2f` before resolving the
        // path, turning the single segment into `../admin` and escaping the prefix.
        // We must also reject `%5c` (encoded backslash) for Windows-backed services.
        let tpl = Template::try_from("https://api.internal/ingest/{{ tenant }}").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

        assert!(matches!(
            c.confine("https://api.internal/ingest/%2e%2e%2fadmin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/%2e%2e%2Fadmin")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/safe%2fpath")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
        assert!(matches!(
            c.confine("https://api.internal/ingest/safe%5cpath")
                .unwrap_err(),
            ConfineError::DotDotSegment { .. }
        ));
    }

    #[test]
    fn uri_authority_mismatch_rejected() {
        let tpl = Template::try_from("https://trusted.example.com{{ path }}").unwrap();
        let c = ConfinementChecker::for_template(&tpl).unwrap().unwrap();

        // Normal path extension is fine.
        assert!(c.confine("https://trusted.example.com/api/v1").is_ok());

        // `@`-userinfo injection: `logs.example.com` becomes the username
        // and `evil.com` becomes the actual host.
        assert!(matches!(
            c.confine("https://trusted.example.com@evil.com/steal")
                .unwrap_err(),
            ConfineError::UriAuthorityMismatch { .. }
        ));

        // Host-extension attack: appending to the hostname routes to a
        // different host entirely.
        assert!(matches!(
            c.confine("https://trusted.example.com.evil.com/steal")
                .unwrap_err(),
            ConfineError::UriAuthorityMismatch { .. }
        ));

        // `@` in a URI path is fine — it's structurally after the authority.
        assert!(
            c.confine("https://trusted.example.com/path/%40user")
                .is_ok()
        );
        assert!(c.confine("https://trusted.example.com/path/@user").is_ok());

        // Wrong scheme is rejected.
        assert!(matches!(
            c.confine("http://trusted.example.com/api/v1").unwrap_err(),
            ConfineError::UriAuthorityMismatch { .. }
        ));
    }

    #[test]
    fn no_static_uri_authority_rejected() {
        // `https://{{ host }}/ingest` has prefix `https://` — any host can be
        // rendered, so the confinement is meaningless. Must be rejected at build.
        for template_str in &["https://{{ host }}/ingest", "http://{{ host }}/path"] {
            let tpl = Template::try_from(*template_str).unwrap();
            assert!(
                matches!(
                    ConfinementChecker::for_template(&tpl).unwrap_err(),
                    BuildError::NoStaticUriAuthority { .. }
                ),
                "expected NoStaticUriAuthority for {template_str}"
            );
        }
        // A template with a static host is accepted.
        let tpl = Template::try_from("https://trusted.example.com{{ path }}").unwrap();
        assert!(ConfinementChecker::for_template(&tpl).unwrap().is_some());
    }

    #[test]
    fn root_only_prefix_rejected() {
        // `/{{ tenant }}/` has a literal prefix of `/` — every rendered value
        // trivially starts with it, so it provides no useful confinement.
        let tpl = Template::try_from("/{{ tenant }}/").unwrap();
        assert!(matches!(
            ConfinementChecker::for_template(&tpl).unwrap_err(),
            BuildError::DerivedBaseIsRoot { .. }
        ));
    }

    #[test]
    fn non_root_slash_prefix_accepted() {
        // `/data/{{ tenant }}/` has a literal prefix of `/data/` — non-root, valid.
        let tpl = Template::try_from("/data/{{ tenant }}/").unwrap();
        let checker = ConfinementChecker::for_template(&tpl).unwrap().unwrap();
        assert!(checker.confine("/data/tenant-a/").is_ok());
        assert!(checker.confine("/other/tenant-a/").is_err());
    }
}
