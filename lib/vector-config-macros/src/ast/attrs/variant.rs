use serde_derive_internals::{attr::Variant, Ctxt};
use syn::{Attribute, Meta, NestedMeta};

use super::{
    duplicate_attribute, get_lit_str, path_to_string, try_extract_doc_title_description,
    try_get_attribute_meta_list,
};

/// A collection of attributes relevant to `Configurable`, specific to enum variants.
//
// TODO: figure out what ident vs title should mean for an enum variant
pub struct VariantAttributes {
    pub variant: syn::Variant,
    pub title: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub serde: Variant,
}

impl VariantAttributes {
    /// Creates a new `VariantAttributes` and extracts all relevant attributes.
    pub fn new(context: &Ctxt, variant: &syn::Variant) -> Self {
        // Start off by extract all `serde`-specific attributes for the given field.
        let serde_variant = Variant::from_ast(context, variant);

        // Now, construct our `VariantAttributes` and extra any `configurable`-specific attributes
        // that we know about.
        let mut variant_attributes = VariantAttributes {
            variant: variant.clone(),
            title: None,
            description: None,
            deprecated: false,
            serde: serde_variant,
        };
        variant_attributes.extract(context, &variant.attrs);

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        variant_attributes.deprecated = variant.attrs.iter().any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they
        // exist. Whether we extract both a title and description, or just description, is
        // documented in more detail in `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(&variant.attrs);
        variant_attributes.title = variant_attributes.title.or(doc_title);
        variant_attributes.description = variant_attributes.description.or(doc_description);

        variant_attributes
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

    /// Whether or not this variant should be added to the schema.
    ///
    /// If the variant is marked as being skipped during both serialization _and_ deserialization,
    /// then there is no reason to account for it in the schema.
    pub fn visible(&self) -> bool {
        !self.serde.skip_deserializing() || !self.serde.skip_serializing()
    }
}
