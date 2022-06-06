use darling::{util::Flag, FromAttributes};
use serde_derive_internals::ast as serde_ast;
use syn::{ExprPath, Ident};
use vector_config_common::validation::Validation;

use super::util::{
    err_field_missing_description, get_serde_default_value, try_extract_doc_title_description,
};

pub struct Field<'a> {
    original: &'a syn::Field,
    name: String,
    default_value: Option<ExprPath>,
    attrs: Attributes,
}

impl<'a> Field<'a> {
    pub fn from_ast(serde: &serde_ast::Field<'a>) -> darling::Result<Field<'a>> {
        let original = serde.original;
        let name = serde.attrs.name().deserialize_name();
        let default_value = get_serde_default_value(serde.attrs.default());

        Attributes::from_attributes(&original.attrs)
            .and_then(|attrs| attrs.finalize(serde, &original.attrs))
            .map(|attrs| Field {
                original,
                name,
                default_value,
                attrs,
            })
    }

    pub fn ident(&self) -> Option<&Ident> {
        self.original.ident.as_ref()
    }

    pub fn ty(&self) -> &syn::Type {
        &self.original.ty
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn title(&self) -> Option<&String> {
        self.attrs.title.as_ref()
    }

    pub fn description(&self) -> Option<&String> {
        self.attrs.description.as_ref()
    }

    pub fn default_value(&self) -> Option<ExprPath> {
        self.default_value.clone()
    }

    pub fn transparent(&self) -> bool {
        self.attrs.transparent.is_present()
    }

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_present()
    }

    pub fn validation(&self) -> &[Validation] {
        &self.attrs.validation
    }

    pub fn visible(&self) -> bool {
        self.attrs.visible
    }

    pub fn flatten(&self) -> bool {
        self.attrs.flatten
    }
}

#[derive(Debug, FromAttributes)]
#[darling(attributes(configurable))]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    derived: Flag,
    transparent: Flag,
    deprecated: Flag,
    #[darling(skip)]
    visible: bool,
    #[darling(skip)]
    flatten: bool,
    #[darling(multiple)]
    validation: Vec<Validation>,
}

impl Attributes {
    fn finalize(
        mut self,
        field: &serde_ast::Field<'_>,
        forwarded_attrs: &[syn::Attribute],
    ) -> darling::Result<Self> {
        // Derive any of the necessary fields from the `serde` side of things.
        self.visible = !field.attrs.skip_deserializing() || !field.attrs.skip_serializing();
        self.flatten = field.attrs.flatten();

        // We additionally attempt to extract a title/description from the forwarded doc attributes, if they exist.
        // Whether we extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(forwarded_attrs);
        self.title = self.title.or(doc_title);
        self.description = self.description.or(doc_description);

        // Make sure that if we weren't able to derive a description from the attributes on this field, that they used
        // the `derived` flag, which implies forwarding the description from the underlying type of the field when the
        // field type's schema is being finalized.
        //
        // The goal with doing so here is to be able to raise a compile-time error that points the user towards setting
        // an explicit description unless they opt to derive it from the underlying type, which won't be _rare_, but is
        // the only way for us to surface such a contextual error, as procedural macros can't dive into the given `T` to
        // know if it has a description or not.
        //
        // If a field is flattened, that's also another form of derivation so we don't require a description in that
        // scenario either.
        if self.description.is_none()
            && !self.derived.is_present()
            && !self.transparent.is_present()
            && self.visible
            && !self.flatten
        {
            return Err(err_field_missing_description(&field.original));
        }

        Ok(self)
    }
}
