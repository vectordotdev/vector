use serde_derive_internals::{ast as serde_ast, attr as serde_attr, Ctxt};
use syn::{ExprPath, Ident, Member, Meta, NestedMeta};

use super::{
    util::{
        duplicate_attribute, err_field_missing_description, err_unexpected_literal,
        err_unexpected_meta_attribute, get_default_exprpath, get_lit_str,
        try_extract_doc_title_description, try_get_attribute_meta_list,
    },
    validation::ValidationDef,
};

pub struct Field<'a> {
    serde: serde_ast::Field<'a>,
    attrs: Attributes,
}

impl<'a> Field<'a> {
    pub fn from_ast(context: &Ctxt, serde: serde_ast::Field<'a>) -> Field<'a> {
        let attrs = Attributes::from_ast(context, &serde);

        Field { serde, attrs }
    }

    pub fn original(&self) -> &syn::Field {
        self.serde.original
    }

    pub fn ident(&self) -> Option<&Ident> {
        match &self.serde.member {
            Member::Named(id) => Some(id),
            Member::Unnamed(_) => None,
        }
    }

    pub fn ty(&self) -> &syn::Type {
        self.serde.ty
    }

    pub fn name(&self) -> String {
        self.serde.attrs.name().deserialize_name()
    }

    pub fn title(&self) -> Option<&String> {
        self.attrs.title.as_ref()
    }

    pub fn description(&self) -> Option<&String> {
        self.attrs.description.as_ref()
    }

    pub fn default_value(&self) -> Option<ExprPath> {
        match self.serde.attrs.default() {
            serde_attr::Default::None => None,
            serde_attr::Default::Default => Some(get_default_exprpath()),
            serde_attr::Default::Path(path) => Some(path.clone()),
        }
    }

    pub fn derived(&self) -> bool {
        self.attrs.derived
    }

    pub fn transparent(&self) -> bool {
        self.attrs.transparent
    }

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated
    }

    pub fn validation(&self) -> &[ValidationDef] {
        &self.attrs.validation
    }

    pub fn visible(&self) -> bool {
        self.attrs.visible
    }
}

/// A collection of attributes relevant to `Configurable`, specific to fields on structs and enums.
#[derive(Debug, Default)]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    derived: bool,
    transparent: bool,
    deprecated: bool,
    visible: bool,
    validation: Vec<ValidationDef>,
}

impl Attributes {
    fn from_ast(context: &Ctxt, field: &serde_ast::Field<'_>) -> Self {
        // Construct our `Attributes` and extract any `configurable`-specific attributes that we know about.
        let mut attributes = Attributes::default();
        attributes.extract(context, &field.original.attrs);

        attributes.visible = !field.attrs.skip_deserializing() || !field.attrs.skip_serializing();

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        attributes.deprecated = field
            .original
            .attrs
            .iter()
            .any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they exist. Whether we
        // extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(&field.original.attrs);
        attributes.title = attributes.title.or(doc_title);
        attributes.description = attributes.description.or(doc_description);

        // Make sure that if we weren't able to derive a description from the attributes on this
        // field, that they used the `derived` flag, which implies forwarding the description from
        // the underlying type of the field when the field type's schema is being finalized.
        //
        // The goal with doing so here is to be able to raise a compile-time error that points the
        // user towards setting an explicit description unless they opt to derive it from the
        // underlying type, which won't be _rare_, but is the only way for us to surface such a
        // contextual error, as procedural macros can't dive into the given `T` to know if it has a
        // description or not.
        if attributes.description.is_none()
            && !attributes.derived
            && !attributes.transparent
            && attributes.visible
        {
            err_field_missing_description(context, field);
        }

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

                // Description derived from the `Configurable` implementation of the underlying
                // field type.
                NestedMeta::Meta(Meta::Path(p)) if p.is_ident("derived") => {
                    self.derived = true;
                }

                // Description provided at a higher-level and should not be applied to this field as
                // it would otherwise clutter the output, or provide duplicate information.
                NestedMeta::Meta(Meta::Path(p)) if p.is_ident("transparent") => {
                    self.transparent = true;
                }

                // Validators for the field.
                NestedMeta::Meta(Meta::List(ml)) if ml.path.is_ident("validation") => {
                    if let Ok(validation_defs) =
                        ValidationDef::parse_defs(context, ml.nested.iter())
                    {
                        // TODO: check if there's any duplicates within the defs we just got back,
                        // and between what we just got back and the defs we already have
                        self.validation.extend(validation_defs);
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
