use super::*;

impl fmt::Debug for ConfinedTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfinedTemplate")
            .field("inner", &self.inner)
            .field("confinement", &self.checker.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl fmt::Debug for ConfinedUriTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfinedUriTemplate")
            .field("inner", &self.inner)
            .finish()
    }
}

impl fmt::Display for ConfinedTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl fmt::Display for ConfinedUriTemplate {
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

impl ConfinedUriTemplate {
    /// Set tz offset on the wrapped template.
    pub fn with_tz_offset(mut self, tz_offset: Option<FixedOffset>) -> Self {
        self.inner = self.inner.with_tz_offset(tz_offset);
        self
    }

    /// Run the URI confinement check against a raw string without going through a normal render.
    ///
    /// Callers that bypass template rendering entirely must call this to keep
    /// the confinement contract intact.
    pub fn check_confinement(&self, rendered: &str) -> Result<(), TemplateRenderingError> {
        self.inner.check_confinement(rendered)
    }

    /// Renders the given template with data from the event, returning raw bytes.
    ///
    /// If a confinement checker was attached via [`UriTemplate::confine`], it runs
    /// after rendering and returns [`TemplateRenderingError::Confined`] on failure.
    pub fn render<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<Bytes, TemplateRenderingError> {
        self.render_string(event.into()).map(Into::into)
    }

    /// Renders the given template with data from the event.
    ///
    /// If a confinement checker was attached via [`UriTemplate::confine`], it runs
    /// after rendering and returns [`TemplateRenderingError::Confined`] on failure.
    pub fn render_string<'a>(
        &self,
        event: impl Into<EventRef<'a>>,
    ) -> Result<String, TemplateRenderingError> {
        self.inner.render_string(event)
    }

    /// Returns the fields used by this template for dynamic rendering.
    ///
    /// Delegates to [`UnconfinedTemplate::get_fields`].
    pub fn get_fields(&self) -> Option<Vec<String>> {
        self.inner.get_fields()
    }
}
