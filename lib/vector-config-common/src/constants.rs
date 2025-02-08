use serde_json::Value;
use syn::Path;

pub const COMPONENT_TYPE_API: &str = "api";
pub const COMPONENT_TYPE_ENRICHMENT_TABLE: &str = "enrichment_table";
pub const COMPONENT_TYPE_PROVIDER: &str = "provider";
pub const COMPONENT_TYPE_SECRETS: &str = "secrets";
pub const COMPONENT_TYPE_SINK: &str = "sink";
pub const COMPONENT_TYPE_SOURCE: &str = "source";
pub const COMPONENT_TYPE_TRANSFORM: &str = "transform";
pub const COMPONENT_TYPE_GLOBAL_OPTION: &str = "global_option";
pub const DOCS_META_ADDITIONAL_PROPS_DESC: &str = "docs::additional_props_description";
pub const DOCS_META_ADVANCED: &str = "docs::advanced";
pub const DOCS_META_COMPONENT_BASE_TYPE: &str = "docs::component_base_type";
pub const DOCS_META_COMPONENT_NAME: &str = "docs::component_name";
pub const DOCS_META_COMPONENT_TYPE: &str = "docs::component_type";
pub const DOCS_META_ENUM_CONTENT_FIELD: &str = "docs::enum_content_field";
pub const DOCS_META_ENUM_TAG_DESCRIPTION: &str = "docs::enum_tag_description";
pub const DOCS_META_ENUM_TAG_FIELD: &str = "docs::enum_tag_field";
pub const DOCS_META_ENUM_TAGGING: &str = "docs::enum_tagging";
pub const DOCS_META_EXAMPLES: &str = "docs::examples";
pub const DOCS_META_HIDDEN: &str = "docs::hidden";
pub const DOCS_META_HUMAN_NAME: &str = "docs::human_name";
pub const DOCS_META_NUMERIC_TYPE: &str = "docs::numeric_type";
pub const DOCS_META_OPTIONAL: &str = "docs::optional";
pub const DOCS_META_COMMON: &str = "docs::common";
pub const DOCS_META_REQUIRED: &str = "docs::required";
pub const DOCS_META_SYNTAX_OVERRIDE: &str = "docs::syntax_override";
pub const DOCS_META_TEMPLATEABLE: &str = "docs::templateable";
pub const DOCS_META_TYPE_OVERRIDE: &str = "docs::type_override";
pub const DOCS_META_TYPE_UNIT: &str = "docs::type_unit";
pub const LOGICAL_NAME: &str = "logical_name";
pub const METADATA: &str = "_metadata";

/// Well-known component types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentType {
    Api,
    EnrichmentTable,
    GlobalOption,
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
            ComponentType::Api => COMPONENT_TYPE_API,
            ComponentType::EnrichmentTable => COMPONENT_TYPE_ENRICHMENT_TABLE,
            ComponentType::GlobalOption => COMPONENT_TYPE_GLOBAL_OPTION,
            ComponentType::Provider => COMPONENT_TYPE_PROVIDER,
            ComponentType::Secrets => COMPONENT_TYPE_SECRETS,
            ComponentType::Sink => COMPONENT_TYPE_SINK,
            ComponentType::Source => COMPONENT_TYPE_SOURCE,
            ComponentType::Transform => COMPONENT_TYPE_TRANSFORM,
        }
    }

    pub fn is_valid_type(path: &Path) -> bool {
        ComponentType::try_from(path).is_ok()
    }
}

impl<'a> TryFrom<&'a str> for ComponentType {
    type Error = ();

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        match value {
            COMPONENT_TYPE_API => Ok(ComponentType::Api),
            COMPONENT_TYPE_ENRICHMENT_TABLE => Ok(ComponentType::EnrichmentTable),
            COMPONENT_TYPE_GLOBAL_OPTION => Ok(ComponentType::GlobalOption),
            COMPONENT_TYPE_PROVIDER => Ok(ComponentType::Provider),
            COMPONENT_TYPE_SECRETS => Ok(ComponentType::Secrets),
            COMPONENT_TYPE_SINK => Ok(ComponentType::Sink),
            COMPONENT_TYPE_SOURCE => Ok(ComponentType::Source),
            COMPONENT_TYPE_TRANSFORM => Ok(ComponentType::Transform),
            _ => Err(()),
        }
    }
}

impl<'a> TryFrom<&'a Path> for ComponentType {
    type Error = ();

    fn try_from(path: &'a Path) -> Result<Self, Self::Error> {
        path.get_ident()
            .ok_or(())
            .map(|id| id.to_string())
            .and_then(|s| Self::try_from(s.as_str()))
    }
}

impl From<&ComponentType> for Value {
    fn from(value: &ComponentType) -> Self {
        Value::String(value.as_str().to_string())
    }
}
