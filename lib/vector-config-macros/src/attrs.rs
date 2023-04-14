use proc_macro2::Span;
use syn::{Ident, Path};

#[derive(Copy, Clone)]
pub struct AttributeIdent(&'static str);

impl AttributeIdent {
    pub fn as_ident(&self, span: Span) -> Ident {
        Ident::new(self.0, span)
    }
}

pub const NO_SER: AttributeIdent = AttributeIdent("no_ser");
pub const NO_DESER: AttributeIdent = AttributeIdent("no_deser");
pub const ENRICHMENT_TABLE_COMPONENT: AttributeIdent = AttributeIdent("enrichment_table_component");
pub const PROVIDER_COMPONENT: AttributeIdent = AttributeIdent("provider_component");
pub const SECRETS_COMPONENT: AttributeIdent = AttributeIdent("secrets_component");
pub const SINK_COMPONENT: AttributeIdent = AttributeIdent("sink_component");
pub const SOURCE_COMPONENT: AttributeIdent = AttributeIdent("source_component");
pub const TRANSFORM_COMPONENT: AttributeIdent = AttributeIdent("transform_component");

impl PartialEq<AttributeIdent> for Ident {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self == word.0
    }
}

impl<'a> PartialEq<AttributeIdent> for &'a Ident {
    fn eq(&self, word: &AttributeIdent) -> bool {
        *self == word.0
    }
}

impl PartialEq<AttributeIdent> for Path {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self.is_ident(word.0)
    }
}

impl<'a> PartialEq<AttributeIdent> for &'a Path {
    fn eq(&self, word: &AttributeIdent) -> bool {
        self.is_ident(word.0)
    }
}

pub fn path_matches(path: &Path, haystack: &[AttributeIdent]) -> bool {
    haystack.iter().any(|p| path == p)
}
