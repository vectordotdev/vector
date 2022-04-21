use core::fmt;

use serde_derive_internals::{attr::Field, Ctxt};
use syn::{Attribute, Meta, NestedMeta};

use super::{
    container::ContainerAttributes, duplicate_attribute, get_lit_str, path_to_string,
    try_extract_doc_title_description, try_get_attribute_meta_list, validation::ValidationDef,
    VariantAttributes,
};

/// A collection of attributes relevant to `Configurable`, specific to fields on structs and enums.
pub struct FieldAttributes {
    pub field: syn::Field,
    pub title: Option<String>,
    pub description: Option<String>,
    pub derived: bool,
    pub transparent: bool,
    pub deprecated: bool,
    pub validation: Vec<ValidationDef>,
    pub serde: Field,
}

impl FieldAttributes {
    /// Creates a new `FieldAttributes` for a field originating in a struct container.
    pub fn from_container(
        context: &Ctxt,
        container: &ContainerAttributes,
        field: &syn::Field,
        index: usize,
    ) -> Self {
        // Start off by extract all `serde`-specific attributes for the given field.
        let mut serde_field = Field::from_ast(context, index, field, None, container.serde.default());
        serde_field.rename_by_rules(container.serde.rename_all_rules());

        Self::with_serde(context, field, serde_field, index)
    }

    /// Creates a new `FieldAttributes` for a field originating in an enum variant.
    pub fn from_variant(
        context: &Ctxt,
        variant: &VariantAttributes,
        field: &syn::Field,
        index: usize,
    ) -> Self {
        // Start off by extract all `serde`-specific attributes for the given field.
        let serde_field = Field::from_ast(
            context,
            index,
            field,
            Some(&variant.serde),
            &serde_derive_internals::attr::Default::None,
        );

        Self::with_serde(context, field, serde_field, index)
    }

    fn with_serde(context: &Ctxt, field: &syn::Field, serde_field: Field, index: usize) -> Self {
        // Construct our `FieldAttributes` and extract any `configurable`-specific attributes that we know about.
        let mut field_attributes = FieldAttributes {
            field: field.clone(),
            title: None,
            description: None,
            derived: false,
            transparent: false,
            deprecated: false,
            validation: Vec::new(),
            serde: serde_field,
        };
        field_attributes.extract(context, &field.attrs);

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        field_attributes.deprecated = field.attrs.iter().any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they
        // exist. Whether we extract both a title and description, or just description, is
        // documented in more detail in `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(&field.attrs);
        field_attributes.title = field_attributes.title.or(doc_title);
        field_attributes.description = field_attributes.description.or(doc_description);

        // Make sure that if we weren't able to derive a description from the attributes on this
        // field, that they used the `derived` flag, which implies forwarding the description from
        // the underlying type of the field when the field type's schema is being finalized.
        //
        // The goal with doing so here is to be able to raise a compile-time error that points the
        // user towards setting an explicit description unless they opt to derive it from the
        // underlying type, which won't be _rare_, but is the only way for us to surface such a
        // contextual error, as procedural macros can't dive into the given `T` to know if it has a
        // description or not.
        if field_attributes.description.is_none()
            && !field_attributes.derived
            && !field_attributes.transparent
            && field_attributes.visible()
        {
            context.error_spanned_by(
                field,
                format!(
                    "field {} must have a description -- i.e. `/// Description of variant...` or `#[configurable(description = \"Description of variant...\")] -- or derive it from the underlying type of the field by specifying `#[configurable(derived)]`",
                    field.ident.as_ref()
                        .map(|ident| format!("`{}`", ident))
                        .unwrap_or_else(|| index.to_string()),
                )
            );
        }

        field_attributes
    }

    fn extract(&mut self, context: &Ctxt, attributes: &[Attribute]) {
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
                    let path = meta.path();
                    context.error_spanned_by(
                        path,
                        format!("unknown configurable attribute `{}`", path_to_string(path)),
                    );
                }

                // We don't support literals in the `configurable` helper at all, so...
                NestedMeta::Lit(lit) => {
                    context.error_spanned_by(lit, "unexpected literal in configurable attribute");
                }
            }
        }
    }

    /// Whether or not this field should be added to the schema.
    ///
    /// If the field is marked as being skipped during both serialization _and_ deserialization,
    /// then there is no reason to account for it in the schema.
    pub fn visible(&self) -> bool {
        !self.serde.skip_deserializing() || !self.serde.skip_serializing()
    }
}

impl fmt::Debug for FieldAttributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FieldAttributes")
            .field("field", &self.field)
            .field("title", &self.title)
            .field("description", &self.description)
            .field("deprecated", &self.deprecated)
            .field("validation", &self.validation)
            //.field("serde", &self.serde)
            .finish()
    }
}
