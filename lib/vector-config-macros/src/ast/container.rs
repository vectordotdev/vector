use darling::{
    error::Accumulator,
    util::{path_to_string, Flag},
    FromAttributes, FromMeta,
};
use serde_derive_internals::{ast as serde_ast, Ctxt, Derive};
use syn::{DeriveInput, ExprPath, Generics, Ident, NestedMeta};

use super::{
    util::{
        err_serde_failed, get_serde_default_value, try_extract_doc_title_description,
        DarlingResultIterator,
    },
    Data, Field, Style, Tagging, Variant,
};

const ERR_NO_ENUM_TUPLES: &str = "enum variants cannot be tuples (multiple unnamed fields)";
const ERR_NO_ENUM_VARIANT_DESCRIPTION: &str = "enum variants must have a description i.e. `/// This is a description` or `#[configurable(description = \"This is a description...\")]`";
const ERR_ENUM_UNTAGGED_DUPLICATES: &str = "enum variants must be unique in style/shape when in untagged mode i.e. there cannot be multiple unit variants, or tuple variants with the same fields, etc";
const ERR_NO_UNIT_STRUCTS: &str = "unit structs are not supported by `Configurable`";
const ERR_MISSING_DESC: &str = "all structs/enums must have a description i.e. `/// This is a description` or `#[configurable(description = \"This is a description...\")]`";

pub struct Container<'a> {
    original: &'a DeriveInput,
    name: String,
    default_value: Option<ExprPath>,
    data: Data<'a>,
    attrs: Attributes,
}

impl<'a> Container<'a> {
    pub fn from_derive_input(input: &'a DeriveInput) -> darling::Result<Container<'a>> {
        // We can't do anything unless `serde` can also handle this container. We specifically only care about
        // deserialization here, because the schema tells us what we can _give_ to Vector.
        let context = Ctxt::new();
        let serde = match serde_ast::Container::from_ast(&context, input, Derive::Deserialize) {
            Some(serde) => {
                // This `serde_derive_internals` helper will panic if `check` isn't _always_ called, so we also have to
                // call it on the success path.
                let _ = context
                    .check()
                    .expect("should not have errors if container was parsed successfully");
                Ok(serde)
            }
            None => Err(err_serde_failed(context)),
        }?;

        // Once we have the `serde` side of things, we need to collect our own specific attributes for the container
        // and map things to our own `Container`.
        Attributes::from_attributes(&input.attrs)
            .and_then(|attrs| attrs.finalize(&input.attrs))
            // We successfully parsed the derive input through both `serde` itself and our own attribute parsing, so
            // build our data container based on whether or not we have a struct, enum, and do any neccessary
            // validation, etc.
            .and_then(|attrs| {
                let mut accumulator = Accumulator::default();
                let tagging: Tagging = serde.attrs.tag().into();

                let data = match serde.data {
                    serde_ast::Data::Enum(variants) => {
                        let variants = variants
                            .iter()
                            .map(|variant| Variant::from_ast(variant, tagging.clone()))
                            .collect_darling_results(&mut accumulator);

                        // Check the generated variants for conformance. We do this at a per-variant and per-enum level.
                        // Not all enum variant styles are compatible with the various tagging types that `serde`
                        // supports, and additionally, we have some of our own constraints that we want to enforce.
                        for variant in &variants {
                            // We don't support tuple variants.
                            if variant.style() == Style::Tuple {
                                accumulator.push(
                                    darling::Error::custom(ERR_NO_ENUM_TUPLES).with_span(variant),
                                );
                            }

                            // All variants must have a description.  No derived/transparent mode.
                            if variant.description().is_none() {
                                accumulator.push(
                                    darling::Error::custom(ERR_NO_ENUM_VARIANT_DESCRIPTION)
                                        .with_span(variant),
                                );
                            }
                        }

                        // If we're in untagged mode, there can be no duplicate variants.
                        if tagging == Tagging::None {
                            for (i, variant) in variants.iter().enumerate() {
                                for (k, other_variant) in variants.iter().enumerate() {
                                    if variant == other_variant && i != k {
                                        accumulator.push(
                                            darling::Error::custom(ERR_ENUM_UNTAGGED_DUPLICATES)
                                                .with_span(variant),
                                        );
                                    }
                                }
                            }
                        }

                        Data::Enum(variants)
                    }
                    serde_ast::Data::Struct(style, fields) => match style {
                        serde_ast::Style::Struct
                        | serde_ast::Style::Tuple
                        | serde_ast::Style::Newtype => {
                            let fields = fields
                                .iter()
                                .map(Field::from_ast)
                                .collect_darling_results(&mut accumulator);

                            Data::Struct(style.into(), fields)
                        }
                        serde_ast::Style::Unit => {
                            // This is a little ugly but we can't drop the accumulator without finishing it, otherwise
                            // it will panic to let us know we didn't assert whether there were errors or not... so add
                            // our error and just return a dummy value.
                            accumulator
                                .push(darling::Error::custom(ERR_NO_UNIT_STRUCTS).with_span(input));
                            Data::Struct(Style::Unit, Vec::new())
                        }
                    },
                };

                // All containers must have a description: no ifs, ands, or buts.
                //
                // The compile-time errors are a bit too inscrutable otherwise, and inscrutable errors are not very
                // helpful when using procedural macros.
                if attrs.description.is_none() {
                    accumulator
                        .push(darling::Error::custom(ERR_MISSING_DESC).with_span(&serde.ident));
                }

                let original = input;
                let name = serde.attrs.name().deserialize_name();
                let default_value = get_serde_default_value(serde.attrs.default());

                let container = Container {
                    original,
                    name,
                    default_value,
                    data,
                    attrs,
                };

                accumulator.finish_with(container)
            })
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

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_present()
    }

    pub fn metadata(&self) -> impl Iterator<Item = &(String, String)> {
        self.attrs
            .metadata
            .iter()
            .flat_map(|metadata| &metadata.pairs)
    }
}

#[derive(Debug, FromAttributes)]
#[darling(attributes(configurable))]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    deprecated: Flag,
    #[darling(multiple)]
    metadata: Vec<Metadata>,
}

impl Attributes {
    fn finalize(mut self, forwarded_attrs: &[syn::Attribute]) -> darling::Result<Self> {
        // We additionally attempt to extract a title/description from the forwarded doc attributes, if they exist.
        // Whether we extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(forwarded_attrs);
        self.title = self.title.or(doc_title);
        self.description = self.description.or(doc_description);

        Ok(self)
    }
}

#[derive(Debug)]
struct Metadata {
    pairs: Vec<(String, String)>,
}

impl FromMeta for Metadata {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut errors = Accumulator::default();

        // Can't be empty.
        if items.is_empty() {
            errors.push(darling::Error::too_few_items(1));
        }

        errors = errors.checkpoint()?;

        // Can't be anything other than name/value pairs.
        let pairs = items
            .iter()
            .filter_map(|nmeta| match nmeta {
                NestedMeta::Meta(meta) => match meta {
                    syn::Meta::Path(_) => {
                        errors.push(darling::Error::unexpected_type("path").with_span(nmeta));
                        None
                    }
                    syn::Meta::List(_) => {
                        errors.push(darling::Error::unexpected_type("list").with_span(nmeta));
                        None
                    }
                    syn::Meta::NameValue(nv) => match &nv.lit {
                        syn::Lit::Str(s) => Some((path_to_string(&nv.path), s.value())),
                        lit => {
                            errors.push(darling::Error::unexpected_lit_type(lit));
                            None
                        }
                    },
                },
                NestedMeta::Lit(_) => {
                    errors.push(darling::Error::unexpected_type("literal").with_span(nmeta));
                    None
                }
            })
            .collect::<Vec<(String, String)>>();

        errors.finish_with(Metadata { pairs })
    }
}
