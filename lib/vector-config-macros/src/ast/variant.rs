use serde_derive_internals::{ast as serde_ast, Ctxt};
use syn::{Ident, Meta, NestedMeta};

use super::{
    util::{
        duplicate_attribute, err_unexpected_literal, err_unexpected_meta_attribute, get_lit_str,
        try_extract_doc_title_description, try_get_attribute_meta_list,
    },
    Field, Style, Tagging,
};

pub struct Variant<'a> {
    serde: serde_ast::Variant<'a>,
    attrs: Attributes,
    fields: Vec<Field<'a>>,
    tagging: Tagging,
}

impl<'a> Variant<'a> {
    pub fn from_ast(
        context: &Ctxt,
        mut serde: serde_ast::Variant<'a>,
        tagging: Tagging,
    ) -> Variant<'a> {
        let attrs = Attributes::from_ast(context, &serde);

        let fields = serde
            .fields
            .drain(..)
            .map(|field| Field::from_ast(context, field))
            .collect();

        Variant {
            serde,
            attrs,
            fields,
            tagging,
        }
    }

    pub fn original(&self) -> &syn::Variant {
        self.serde.original
    }

    pub fn ident(&self) -> &Ident {
        &self.serde.ident
    }

    pub fn style(&self) -> Style {
        self.serde.style.into()
    }

    pub fn tagging(&self) -> &Tagging {
        &self.tagging
    }

    pub fn fields(&self) -> &[Field<'_>] {
        &self.fields
    }

    pub fn name(&self) -> String {
        self.serde.attrs.name().deserialize_name()
    }

    pub fn title(&self) -> Option<String> {
        self.attrs.title.clone()
    }

    pub fn description(&self) -> Option<String> {
        self.attrs.description.clone()
    }

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated
    }

    pub fn visible(&self) -> bool {
        self.attrs.visible
    }
}

#[derive(Debug, Default)]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    deprecated: bool,
    visible: bool,
}

impl Attributes {
    fn from_ast(context: &Ctxt, variant: &serde_ast::Variant<'_>) -> Self {
        // Construct our `Attributes` and extract any `configurable`-specific attributes that we know about.
        let mut attributes = Attributes::default();
        attributes.extract(context, &variant.original.attrs);

        attributes.visible =
            !variant.attrs.skip_deserializing() || !variant.attrs.skip_serializing();

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        attributes.deprecated = variant
            .original
            .attrs
            .iter()
            .any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they
        // exist. Whether we extract both a title and description, or just description, is
        // documented in more detail in `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) =
            try_extract_doc_title_description(&variant.original.attrs);
        attributes.title = attributes.title.or(doc_title);
        attributes.description = attributes.description.or(doc_description);

        attributes
    }

    fn extract(&mut self, context: &Ctxt, attributes: &[syn::Attribute]) {
        for meta_item in attributes
            .iter()
            .flat_map(|attribute| try_get_attribute_meta_list(attribute, context))
            .flatten()
        {
            match &meta_item {
                // Title set directly via the `configurable` helper.
                NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("title") => {
                    if let Ok(title) = get_lit_str(context, "title", &m.lit) {
                        match self.title {
                            Some(_) => duplicate_attribute(context, m),
                            None => self.title = Some(title.value()),
                        }
                    }
                }

                // Title set directly via the `configurable` helper.
                NestedMeta::Meta(Meta::NameValue(m)) if m.path.is_ident("description") => {
                    if let Ok(description) = get_lit_str(context, "description", &m.lit) {
                        match self.description {
                            Some(_) => duplicate_attribute(context, m),
                            None => self.description = Some(description.value()),
                        }
                    }
                }

                // We've hit a meta item that we don't handle.
                NestedMeta::Meta(meta) => {
                    err_unexpected_meta_attribute(meta, context);
                }

                // We don't support literals in the `configurable` helper at all, so...
                NestedMeta::Lit(lit) => {
                    err_unexpected_literal(context, lit);
                }
            }
        }
    }
}

impl<'a> PartialEq for Variant<'a> {
    fn eq(&self, other: &Self) -> bool {
        // Equality checking between variants is only used to drive conformance checks around making
        // sure no duplicate variants exist when in untagged mode, so all we care about is what
        // distinguishes a variant when it's in its serialized form, which is the shape -- struct vs
        // tuple vs unit -- and the fields therein.

        // It's suboptimal to be allocating strings for the field names here but we need the
        // deserialized name as `serde` observes it, and this only runs at compile-time.
        let self_fields = self
            .fields
            .iter()
            .map(|field| (field.name(), field.ty()))
            .collect::<Vec<_>>();
        let other_fields = other
            .fields
            .iter()
            .map(|field| (field.name(), field.ty()))
            .collect::<Vec<_>>();

        self.style() == other.style()
            && self.tagging == other.tagging
            && self_fields == other_fields
    }
}
