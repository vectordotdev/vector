use super::*;

impl Template<false> {
    /// Confine this template to its literal prefix for **non-URI fields**, returning
    /// a [`ConfinedTemplate`] that enforces prefix confinement at render time.
    ///
    /// Use this for object-store keys, Kafka topics, Redis keys, tenant IDs, and
    /// other non-URI fields. For HTTP/HTTPS URI fields, use [`UriTemplate`] instead.
    ///
    /// The confinement semantics are determined by caller intent (this method
    /// chooses prefix confinement), not by inspecting template content. A template
    /// like `"http://logs-{{ region }}/"` will use prefix confinement because the
    /// caller chose `confine`, not URI confinement.
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
        match ConfinementChecker::for_prefix_template(&self) {
            Ok(Some(checker)) => Ok(ConfinedTemplate {
                inner: self.inner,
                checker: Some(checker),
            }),
            Ok(None) => Ok(ConfinedTemplate {
                inner: self.inner,
                checker: None,
            }),
            Err(e) => Err(e.into()),
        }
    }
}

impl<const URI: bool> Template<URI> {
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
    pub fn get_ref(&self) -> &str {
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

impl<const URI: bool> fmt::Display for Template<URI> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<const URI: bool> From<UnconfinedTemplate> for Template<URI> {
    fn from(inner: UnconfinedTemplate) -> Self {
        Template { inner }
    }
}

impl<const URI: bool> TryFrom<String> for Template<URI> {
    type Error = TemplateParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(s).map(|inner| Template { inner })
    }
}

impl<const URI: bool> TryFrom<&str> for Template<URI> {
    type Error = TemplateParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(s).map(|inner| Template { inner })
    }
}

impl<const URI: bool> TryFrom<PathBuf> for Template<URI> {
    type Error = TemplateParseError;

    fn try_from(p: PathBuf) -> Result<Self, Self::Error> {
        UnconfinedTemplate::try_from(p).map(|inner| Template { inner })
    }
}

impl<const URI: bool> From<Template<URI>> for String {
    fn from(t: Template<URI>) -> String {
        t.inner.src
    }
}

// This is safe because we literally defer to `String` for the schema of `Template`.
impl<const URI: bool> ConfigurableString for Template<URI> {}

/// URI-specific confinement, implemented only for [`UriTemplate`] (that is,
/// `Template<true>`).
///
/// This lives in a trait rather than in an `impl UriTemplate` block on purpose.
/// `Template::confine` (prefix confinement) is an inherent method on
/// `Template<false>`, and a second *inherent* `confine` on `Template<true>` would make
/// every `Template::try_from(..).confine(..)` call ambiguous: a const generic parameter
/// default is not applied during inference, so the receiver is `Template<_>` and both
/// inherent candidates would apply. Inherent methods take priority over trait methods
/// during probing, so keeping this one in a trait means `Template<_>` resolves to the
/// inherent (prefix) method and infers `URI = false`, while a receiver already known to
/// be `UriTemplate` falls through to this impl.
pub trait ConfineUri {
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
    fn confine(
        self,
        config: &ConfinementConfig,
        component_name: &'static str,
        field_name: &'static str,
    ) -> crate::Result<ConfinedUriTemplate>;
}

impl ConfineUri for UriTemplate {
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
    fn confine(
        self,
        config: &ConfinementConfig,
        component_name: &'static str,
        field_name: &'static str,
    ) -> crate::Result<ConfinedUriTemplate> {
        // Full opt-out: bypass all confinement for this template (startup AND
        // runtime). The `vector_security_confinement_disabled` gauge is owned by
        // the topology, not emitted here. Return ConfinedUriTemplate without a
        // checker so the type is correct even when opting out.
        if config.dangerously_allow_unconfined_template_resolution {
            ConfinementConfig::warn_unconfined_template("sink", component_name, field_name);
            return Ok(ConfinedUriTemplate {
                inner: ConfinedTemplate {
                    inner: self.inner,
                    checker: None,
                },
            });
        }
        match ConfinementChecker::for_uri_template(&self) {
            Ok(Some(checker)) => Ok(ConfinedUriTemplate {
                inner: ConfinedTemplate {
                    inner: self.inner,
                    checker: Some(checker),
                },
            }),
            Ok(None) => Ok(ConfinedUriTemplate {
                inner: ConfinedTemplate {
                    inner: self.inner,
                    checker: None,
                },
            }),
            Err(e) => Err(e.into()),
        }
    }
}
