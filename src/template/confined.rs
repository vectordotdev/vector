/// A template that has passed through confinement via [`Template::confine`].
///
/// This is the only render-capable, confinement-enforcing template type. It is deliberately **not**
/// deserializable: the sole way to obtain one is by confining a [`Template`] or an
/// [`UnconfinedTemplate`], so a rendered value can never escape its confinement boundary. The
/// `render` methods enforce the attached confinement checker (if any).
///
/// Both fields are private to this module, so a `ConfinedTemplate` can
/// never be constructed (or deserialized) without going through confinement.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ConfinedTemplate {
    inner: UnconfinedTemplate,
    checker: Option<ConfinementChecker>,
}

impl fmt::Debug for ConfinedTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfinedTemplate")
            .field("inner", &self.inner)
            .field("confinement", &self.checker.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl fmt::Display for ConfinedTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl ConfinedTemplate {
    /// Set tz offset on the wrapped template.
    pub const fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.inner.tz_offset = tz_offset;
        self
    }

    /// Run the confinement check against a raw string without going through a normal render.
    ///
    /// Callers that bypass template rendering entirely (e.g. the elasticsearch sink's
    /// `auto_routing` path) must call this to keep the confinement contract intact.
    pub fn check_confinement(&self, rendered: &str) -> Result<(), TemplateRenderingError> {
        if let Some(checker) = &self.checker {
            checker
                .confine(rendered)
                .map_err(|e| TemplateRenderingError::Confined {
                    rendered_preview: confined_preview(rendered),
                    rendered_len: rendered.len(),
                    message: e.to_string(),
                })?;
        }
        Ok(())
    }

    /// Renders the given template with data from the event, returning raw bytes.
    ///
    /// If a confinement checker was attached via [`Template::confine`], it runs after
    /// rendering and returns [`TemplateRenderingError::Confined`] on failure.
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<Bytes, TemplateRenderingError> {
        self.render_string(event.into()).map(Into::into)
    }

    /// Renders the given template with data from the event.
    ///
    /// If a confinement checker was attached via [`Template::confine`], it runs after
    /// rendering and returns [`TemplateRenderingError::Confined`] on failure.
    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        let rendered = self.inner.render_string(event)?;
        self.check_confinement(&rendered)?;
        Ok(rendered)
    }

    /// Returns the fields used by this template for dynamic rendering.
    ///
    /// Delegates to [`UnconfinedTemplate::get_fields`].
    pub fn get_fields(&self) -> Option<Vec<String>> {
        self.inner.get_fields()
    }
}
