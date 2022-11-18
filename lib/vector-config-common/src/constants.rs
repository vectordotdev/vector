// TODO: Actually use this from `vector-config-macros` so that it's properly centralized.

/// Well-known component types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentType {
    EnrichmentTable,
    Provider,
    Secrets,
    Sink,
    Source,
    Transform,
}

impl ComponentType {
    /// Gets the type of this component as a string.
    pub const fn as_str(&self) -> &'static str {
        match self {
            ComponentType::EnrichmentTable => "enrichment_table",
            ComponentType::Provider => "provider",
            ComponentType::Secrets => "secrets",
            ComponentType::Sink => "sink",
            ComponentType::Source => "source",
            ComponentType::Transform => "transform",
        }
    }
}

impl<'a> TryFrom<&'a str> for ComponentType {
    type Error = ();

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        match value {
            "enrichment_table" => Ok(ComponentType::EnrichmentTable),
            "provider" => Ok(ComponentType::Provider),
            "secrets" => Ok(ComponentType::Secrets),
            "sink" => Ok(ComponentType::Sink),
            "source" => Ok(ComponentType::Source),
            "transform" => Ok(ComponentType::Transform),
            _ => Err(()),
        }
    }
}
