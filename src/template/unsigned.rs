use super::parsing::{parse_template, render_metric_field, render_timestamp};
use super::*;

/// The source of a `uint` template. May be a constant numeric value or a template string.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[configurable_component]
#[serde(untagged)]
pub(super) enum UnsignedIntTemplateSource {
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
