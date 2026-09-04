use super::*;

impl Template {
    /// Confine this template to its literal prefix, returning a [`ConfinedTemplate`] that enforces
    /// the confinement invariant at render time.
    ///
    /// Most sinks reach this through [`Template::confine`]; call this directly only when a sink
    /// constructs an [`UnconfinedTemplate`] itself (e.g. from a default value) and needs to confine
    /// it.
    pub fn confine(
        self,
        config: &ConfinementConfig,
        component_name: &'static str,
        field_name: &'static str,
    ) -> crate::Result<ConfinedTemplate> {
        // Full opt-out: bypass all confinement for this template (startup AND
        // runtime). The `vector_security_confinement_disabled` gauge is owned by
        // the topology, not emitted here.
        if config.dangerously_allow_unconfined_template_resolution {
            ConfinementConfig::warn_unconfined_template("sink", component_name, field_name);
            return Ok(ConfinedTemplate {
                inner: self.inner,
                checker: None,
            });
        }
        ConfinementChecker::for_prefix_template(&self)
            .map(|checker| ConfinedTemplate {
                inner: self.inner,
                checker,
            })
            .map_err(Into::into)
    }

    /// Set the tz offset used when rendering strftime specifiers.
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.inner.tz_offset = tz_offset;
        self
    }

    /// Returns the names of the fields referenced by this template, if any.
    ///
    /// This is a read-only inspection that does not render, so it is available before confinement
    /// (e.g. for topology field detection).
    pub fn get_fields(&self) -> Option<Vec<String>> {
        self.inner.get_fields()
    }

    /// Longest leading substring of the template source that is rendered
    /// verbatim — no `{{ field }}` reference and no strftime specifier.
    ///
    /// Sinks use this to derive a confinement boundary from the
    /// operator-authored portion of the template.
    pub fn literal_prefix(&self) -> &str {
        self.inner.literal_prefix()
    }

    /// Returns a reference to the template source string.
    pub const fn get_ref(&self) -> &str {
        self.inner.get_ref()
    }

    /// Returns `true` if the template source is empty.
    pub const fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns `true` if the template depends on the input event or time.
    pub const fn is_dynamic(&self) -> bool {
        self.inner.is_dynamic()
    }
}

impl From<UnconfinedTemplate> for Template {
    fn from(inner: UnconfinedTemplate) -> Self {
        Template { inner }
    }
}

impl TryFrom<String> for Template {
    type Error = TemplateParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(s).map(|inner| Template { inner })
    }
}

impl TryFrom<&str> for Template {
    type Error = TemplateParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(s).map(|inner| Template { inner })
    }
}

impl TryFrom<PathBuf> for Template {
    type Error = TemplateParseError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(p).map(|inner| Template { inner })
    }
}

impl From<Template> for String {
    fn from(t: Template) -> String {
        t.inner.src
    }
}

// This is safe because we literally defer to `String` for the schema of `Template`.
impl ConfigurableString for Template {}

impl UriTemplate {
    /// Confine this URI template for **HTTP/HTTPS URI fields**, returning a
    /// [`ConfinedUriTemplate`] that enforces URI-specific confinement checks.
    ///
    /// URI confinement enforces:
    /// - Authority (scheme + host) must match the operator-configured prefix
    /// - Path must start with the static path prefix
    /// - No `..` path traversal, percent-encoded or otherwise
    /// - No injected query strings or fragments via field values
    /// - Template must start with `http://` or `https://` and include a static host
    ///
    /// The confinement semantics are determined by the type (URI confinement), not
    /// by inspecting template content. The return type [`ConfinedUriTemplate`] is
    /// distinct from [`ConfinedTemplate`], making it impossible to accidentally wire
    /// a prefix-confined template into a URI field.
    pub fn confine(
        self,
        config: &ConfinementConfig,
        component_name: &'static str,
        field_name: &'static str,
    ) -> crate::Result<ConfinedUriTemplate> {
        if config.dangerously_allow_unconfined_template_resolution {
            ConfinementConfig::warn_unconfined_template("sink", component_name, field_name);
            return Ok(ConfinedUriTemplate::new(ConfinedTemplate {
                inner: self.0.inner,
                checker: None,
            }));
        }
        ConfinementChecker::for_uri_template(&self.0)
            .map(|checker| {
                ConfinedUriTemplate::new(ConfinedTemplate {
                    inner: self.0.inner,
                    checker,
                })
            })
            .map_err(Into::into)
    }
}

impl UriTemplate {
    /// Set the tz offset used when rendering strftime specifiers.
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.0.inner.tz_offset = tz_offset;
        self
    }

    /// Returns the names of the fields referenced by this template, if any.
    ///
    /// This is a read-only inspection that does not render, so it is available before confinement.
    pub fn get_fields(&self) -> Option<Vec<String>> {
        self.0.get_fields()
    }

    /// Returns a reference to the template source string.
    pub const fn get_ref(&self) -> &str {
        self.0.get_ref()
    }

    /// Returns `true` if the template source is empty.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if the template depends on the input event or time.
    pub const fn is_dynamic(&self) -> bool {
        self.0.is_dynamic()
    }
}

impl TryFrom<String> for UriTemplate {
    type Error = TemplateParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Template::try_from(s).map(UriTemplate)
    }
}

impl TryFrom<&str> for UriTemplate {
    type Error = TemplateParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Template::try_from(s).map(UriTemplate)
    }
}

impl From<UriTemplate> for String {
    fn from(t: UriTemplate) -> String {
        String::from(t.0)
    }
}
