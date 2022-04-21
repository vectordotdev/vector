use serde_derive_internals::{ast as serde_ast, attr as serde_attr, Ctxt, Derive};
use syn::{DeriveInput, ExprPath, Generics, Ident, Meta, NestedMeta};

use super::{
    util::{
        duplicate_attribute, err_unexpected_literal, err_unexpected_meta_attribute,
        get_back_to_back_lit_strs, get_default_exprpath, get_lit_str,
        try_extract_doc_title_description, try_get_attribute_meta_list,
    },
    Data, Field, Style, Tagging, Variant,
};

const ERR_NO_ENUM_TUPLES: &str = "enum variants cannot be tuples (multiple unnamed fields)";
const ERR_NO_ENUM_NEWTYPE_INTERNAL_TAG: &str = "newtype variants (i.e. `enum SomeEnum { SomeVariant(T) }`) cannot be used with tag-only mode as the type inside may or may not support embedding the tag field";
const ERR_NO_ENUM_VARIANT_DESCRIPTION: &str = "enum variants must have a description i.e. `/// This is a description` or `#[configurable(description = \"This is a description...\")]`";
const ERR_ENUM_UNTAGGED_DUPLICATES: &str = "enum variants must be unique in style/shape when in untagged mode i.e. there cannot be multiple unit variants, or tuple variants with the same fields, etc";
const ERR_NO_UNIT_STRUCTS: &str = "unit structs are not supported by `Configurable`";

pub struct Container<'a> {
    referencable_name: String,
    default_value: Option<ExprPath>,
    data: Data<'a>,
    attrs: Attributes,
    original: &'a DeriveInput,
}

impl<'a> Container<'a> {
    pub fn from_derive_input(context: &Ctxt, input: &'a DeriveInput) -> Option<Container<'a>> {
        // We can't do anything unless `serde` can also handle this container. We specifically only care about
        // deserialization here, because the schema tells us what we can _give_ to Vector.
        serde_ast::Container::from_ast(context, input, Derive::Deserialize)
            // Once we have the `serde` side of things, we need to collect our own specific attributes for the container
            // and map things to our own `Container`.
            .and_then(|serde| {
                let attrs = Attributes::from_ast(context, input);

                let data = match serde.data {
                    serde_ast::Data::Enum(variants) => {
                        let variants = variants
                            .into_iter()
                            .map(|variant| {
                                Variant::from_ast(context, variant, serde.attrs.tag().into())
                            })
                            .collect::<Vec<_>>();

                        // Check the generated variants for conformance. We do this at a per-variant and per-enum level.
                        // Not all enum variant styles are compatible with the various tagging types that `serde`
                        // supports, and additionally, we have some of our own constraints that we want to enforce.
                        let mut had_error = false;
                        for variant in &variants {
                            // We don't support tuple variants.
                            if variant.style() == Style::Tuple {
                                context.error_spanned_by(variant.original(), ERR_NO_ENUM_TUPLES);
                                had_error = true;
                            }

                            // We don't support internal tag for newtype variants, because `serde` doesn't support it.
                            if variant.style() == Style::Newtype
                                && matches!(variant.tagging(), Tagging::Internal { .. })
                            {
                                context.error_spanned_by(
                                    variant.original(),
                                    ERR_NO_ENUM_NEWTYPE_INTERNAL_TAG,
                                );
                                had_error = true;
                            }

                            // All variants must have a description.  No derived/transparent mode.
                            if variant.description().is_none() {
                                context.error_spanned_by(
                                    variant.original(),
                                    ERR_NO_ENUM_VARIANT_DESCRIPTION,
                                );
                                had_error = true;
                            }
                        }

                        // If we're in untagged mode, there can be no duplicate variants.
                        if Tagging::from(serde.attrs.tag()) == Tagging::None {
                            for (i, variant) in variants.iter().enumerate() {
                                for (k, other_variant) in variants.iter().enumerate() {
                                    if variant == other_variant && i != k {
                                        context.error_spanned_by(
                                            variant.original(),
                                            ERR_ENUM_UNTAGGED_DUPLICATES,
                                        );
                                        had_error = true;
                                    }
                                }
                            }
                        }

                        if had_error {
                            return None;
                        }

                        Data::Enum(variants)
                    }
                    serde_ast::Data::Struct(style, fields) => match style {
                        serde_ast::Style::Struct
                        | serde_ast::Style::Tuple
                        | serde_ast::Style::Newtype => {
                            let fields = fields
                                .into_iter()
                                .map(|field| Field::from_ast(context, field))
                                .collect();

                            Data::Struct(Style::Struct, fields)
                        }
                        serde_ast::Style::Unit => {
                            context.error_spanned_by(input, ERR_NO_UNIT_STRUCTS);
                            return None;
                        }
                    },
                };

                let default_value = match serde.attrs.default() {
                    serde_attr::Default::None => None,
                    serde_attr::Default::Default => Some(get_default_exprpath()),
                    serde_attr::Default::Path(path) => Some(path.clone()),
                };

                Some(Container {
                    referencable_name: serde.attrs.name().deserialize_name(),
                    default_value,
                    data,
                    attrs,
                    original: input,
                })
            })
    }

    pub fn original(&self) -> &DeriveInput {
        self.original
    }

    pub fn ident(&self) -> &Ident {
        &self.original.ident
    }

    pub fn generics(&self) -> &Generics {
        &self.original.generics
    }

    pub fn data(&self) -> &Data {
        &self.data
    }

    pub fn referencable_name(&self) -> &str {
        self.referencable_name.as_str()
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

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated
    }

    pub fn metadata(&self) -> &[(String, String)] {
        &self.attrs.metadata
    }
}

/// A collection of attributes relevant to containers i.e. structs and enums.
#[derive(Default)]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    deprecated: bool,
    metadata: Vec<(String, String)>,
}

impl Attributes {
    /// Creates a new `Container` based on the attributes present on the given container AST.
    fn from_ast(context: &Ctxt, input: &DeriveInput) -> Self {
        // Construct our `Container` and extra any valid `configurable`-specific attributes.
        let mut attributes = Attributes::default();
        attributes.extract(context, &input.attrs);

        // Parse any helper-less attributes, such as `deprecated`, which are part of Rust itself.
        attributes.deprecated = input.attrs.iter().any(|a| a.path.is_ident("deprecated"));

        // We additionally attempt to extract a title/description from the doc attributes, if they exist. Whether we
        // extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(&input.attrs);
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

                // A custom metadata key/value pair.
                NestedMeta::Meta(Meta::List(ml)) if ml.path.is_ident("metadata") => {
                    if let Ok((key, value)) = get_back_to_back_lit_strs(context, "metadata", &ml) {
                        self.metadata.push((key.value(), value.value()));
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
