use super::parsing::{parse_template, render_metric_field, render_timestamp};
use super::*;

impl fmt::Debug for UnconfinedTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnconfinedTemplate")
            .field("src", &self.src)
            .field("is_static", &self.is_static)
            .field("tz_offset", &self.tz_offset)
            .finish()
    }
}

impl TryFrom<&str> for UnconfinedTemplate {
    type Error = TemplateParseError;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(Cow::Borrowed(src))
    }
}

impl TryFrom<String> for UnconfinedTemplate {
    type Error = TemplateParseError;

    fn try_from(src: String) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(Cow::Owned(src))
    }
}

impl TryFrom<PathBuf> for UnconfinedTemplate {
    type Error = TemplateParseError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(p.to_string_lossy().into_owned())
    }
}

impl TryFrom<Cow<'_, str>> for UnconfinedTemplate {
    type Error = TemplateParseError;

    fn try_from(src: Cow<'_, str>) -> Result<Self, Self::Error> {
        parse_template(&src).map(|parts| {
            let is_static =
                parts.is_empty() || (parts.len() == 1 && matches!(parts[0], Part::Literal(..)));

            let reserve_size = parts
                .iter()
                .map(|part| match part {
                    Part::Literal(lit) => lit.len(),
                    Part::Reference(_path) => 1,
                    Part::Strftime(parsed) => parsed.reserve_size(),
                })
                .sum();

            UnconfinedTemplate {
                parts,
                src: src.into_owned(),
                is_static,
                reserve_size,
                tz_offset: None,
            }
        })
    }
}

impl From<UnconfinedTemplate> for String {
    fn from(template: UnconfinedTemplate) -> String {
        template.src
    }
}

// This is safe because we literally defer to `String` for the schema of `UnconfinedTemplate`.
impl ConfigurableString for UnconfinedTemplate {}

impl UnconfinedTemplate {
    /// Set tz offset.
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.tz_offset = tz_offset;
        self
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
    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        if self.is_static {
            Ok(self.src.clone())
        } else {
            self.render_event(event.into())
        }
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

    /// Returns a reference to the template string.
    pub const fn get_ref(&self) -> &str {
        self.src.as_str()
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
