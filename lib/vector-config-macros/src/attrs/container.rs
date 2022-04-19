use serde_derive_internals::{attr::Container, Ctxt};
use syn::{Attribute, DeriveInput, Meta, NestedMeta};

use super::{
    duplicate_attribute, get_back_to_back_lit_strs, get_lit_str, path_to_string,
    try_extract_doc_title_description, try_get_attribute_meta_list,
};

/// A collection of attributes relevant to `Configurable`, specific to structs and enums themselves.
pub struct ContainerAttributes {
    pub title: Option<String>,
    pub description: Option<String>,
    pub deprecated: bool,
    pub metadata: Vec<(String, String)>,
    pub serde: Container,
}

impl ContainerAttributes {
    /// Creates a new `ContainerAttributes` and extracts all relevant attributes.
    pub fn new(context: &Ctxt, input: &DeriveInput) -> Self {
        // Start off by extract all `serde`-specific attributes for the given container.
        let serde_container = Container::from_ast(context, input);

        // Now, construct our `ContainerAttributes` and extra any `configurable`-specific attributes
        // that we know about.
        let mut container_attributes = ContainerAttributes {
            title: None,
            description: None,
            deprecated: false,
            metadata: Vec::new(),
            serde: serde_container,
        };
        container_attributes.extract(context, &input.attrs);

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        container_attributes.deprecated = input.attrs.iter().any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they
        // exist. Whether we extract both a title and description, or just description, is
        // documented in more detail in `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(&input.attrs);
        container_attributes.title = container_attributes.title.or(doc_title);
        container_attributes.description = container_attributes.description.or(doc_description);

        container_attributes
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

                // A custom metadata key/value pair.
                NestedMeta::Meta(Meta::List(ml)) if ml.path.is_ident("metadata") => {
                    if let Ok((key, value)) = get_back_to_back_lit_strs(context, "metadata", &ml) {
                        self.metadata.push((key.value(), value.value()));
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
}
