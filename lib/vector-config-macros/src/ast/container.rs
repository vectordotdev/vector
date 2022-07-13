use darling::{error::Accumulator, util::Flag, FromAttributes};
use serde_derive_internals::{ast as serde_ast, Ctxt, Derive};
use syn::{DeriveInput, ExprPath, Generics, Ident, Type};
use vector_config_common::attributes::CustomAttribute;

use super::{
    util::{
        err_serde_failed, get_serde_default_value, try_extract_doc_title_description,
        DarlingResultIterator,
    },
    Data, Field, Metadata, Style, Tagging, Variant,
};

const ERR_NO_ENUM_TUPLES: &str = "enum variants cannot be tuples (multiple unnamed fields)";
const ERR_NO_ENUM_VARIANT_DESCRIPTION: &str = "enum variants must have a description i.e. `/// This is a description` or `#[configurable(description = \"This is a description...\")]`";
const ERR_ENUM_UNTAGGED_DUPLICATES: &str = "enum variants must be unique in style/shape when in untagged mode i.e. there cannot be multiple unit variants, or tuple variants with the same fields, etc";
const ERR_NO_UNIT_STRUCTS: &str = "unit structs are not supported by `Configurable`";
const ERR_MISSING_DESC: &str = "all structs/enums must have a description i.e. `/// This is a description` or `#[configurable(description = \"This is a description...\")]`";
const ERR_ASYMMETRIC_SERDE_TYPE_CONVERSION: &str = "any container using `from`/`try_from`/`into` via `#[serde(...)]` must do so symmetrically i.e. the from/into types must match";
const ERR_SERDE_TYPE_CONVERSION_FROM_TRY_FROM: &str = "`#[serde(from)]` and `#[serde(try_from)]` cannot be identical, as it is impossible for an infallible conversion from T to also be fallible";

pub struct Container<'a> {
    original: &'a DeriveInput,
    name: String,
    default_value: Option<ExprPath>,
    data: Data<'a>,
    virtual_newtype: Option<Type>,
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
                context
                    .check()
                    .expect("should not have errors if container was parsed successfully");
                Ok(serde)
            }
            None => Err(err_serde_failed(context)),
        }?;

        let mut accumulator = Accumulator::default();

        // Check if we're dealing with a "virtual" newtype.
        //
        // In some cases, types may (de)serialize themselves as another type, which is entirely normal... but
        // they may do this with `serde` helper attributes rather than with a newtype wrapper or manually
        // converting between types.
        //
        // For types doing this, it could be entirely irrelevant to document all of the internal fields, or at
        // least enforce documenting them, because they don't truly represent the actual schema and all that
        // might get used is the documentation on the type having `Configurable` derived.
        //
        // All of that said, we check to see if the `from`, `try_from`, or `into` helper attributes are being
        // used from `serde`, and make sure the transformation is symmetric (it has to
        // deserialize from T and serialize to T, no halfsies) since we can't express a schema that's
        // half-and-half. Assuming it passes this requirement, we track the actual (de)serialized type and use
        // that for our schema generation instead.
        let virtual_newtype = if serde.attrs.type_from().is_some()
            || serde.attrs.type_try_from().is_some()
            || serde.attrs.type_into().is_some()
        {
            // if any of these are set, we start by checking `into`. If it's set, then that's fine, and we
            // continue verifying. Otherwise, it implies that `from`/`try_from` are set, and we only allow
            // symmetric conversions.
            if let Some(into_ty) = serde.attrs.type_into() {
                // Figure out which of `from` and `try_from` are set. Both cannot be set, because either the
                // types are different -- which means asymmetric conversion -- or they're both the same, which
                // would be a logical fallacy since you can't have a fallible conversion from T if you already
                // have an infallible conversion from T.
                //
                // Similar, at least one of them must be set, otherwise that's an asymmetric conversion.
                match (serde.attrs.type_from(), serde.attrs.type_try_from()) {
                    (None, None) => {
                        accumulator.push(
                            darling::Error::custom(ERR_ASYMMETRIC_SERDE_TYPE_CONVERSION)
                                .with_span(&serde.ident),
                        );
                        None
                    }
                    (Some(_), Some(_)) => {
                        accumulator.push(
                            darling::Error::custom(ERR_SERDE_TYPE_CONVERSION_FROM_TRY_FROM)
                                .with_span(&serde.ident),
                        );
                        None
                    }
                    (Some(from_ty), None) | (None, Some(from_ty)) => {
                        if into_ty == from_ty {
                            Some(into_ty.clone())
                        } else {
                            accumulator.push(
                                darling::Error::custom(ERR_ASYMMETRIC_SERDE_TYPE_CONVERSION)
                                    .with_span(&serde.ident),
                            );
                            None
                        }
                    }
                }
            } else {
                accumulator.push(
                    darling::Error::custom(ERR_ASYMMETRIC_SERDE_TYPE_CONVERSION)
                        .with_span(&serde.ident),
                );
                None
            }
        } else {
            None
        };

        // Once we have the `serde` side of things, we need to collect our own specific attributes for the container
        // and map things to our own `Container`.
        Attributes::from_attributes(&input.attrs)
            .and_then(|attrs| attrs.finalize(&input.attrs))
            // We successfully parsed the derive input through both `serde` itself and our own attribute parsing, so
            // build our data container based on whether or not we have a struct, enum, and do any neccessary
            // validation, etc.
            .and_then(|attrs| {
                let tagging: Tagging = serde.attrs.tag().into();

                let data = match serde.data {
                    serde_ast::Data::Enum(variants) => {
                        let variants = variants
                            .iter()
                            .map(|variant| {
                                Variant::from_ast(
                                    variant,
                                    tagging.clone(),
                                    virtual_newtype.is_some(),
                                )
                            })
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
                                .map(|field| Field::from_ast(field, virtual_newtype.is_some()))
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
                    virtual_newtype,
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

    pub fn virtual_newtype(&self) -> Option<Type> {
        self.virtual_newtype.clone()
    }

    pub fn default_value(&self) -> Option<ExprPath> {
        self.default_value.clone()
    }

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_some()
    }

    pub fn metadata(&self) -> impl Iterator<Item = CustomAttribute> {
        self.attrs
            .metadata
            .clone()
            .into_iter()
            .flat_map(|metadata| metadata.attributes())
    }
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(configurable))]
struct Attributes {
    #[darling(default)]
    title: Option<String>,
    #[darling(default)]
    description: Option<String>,
    #[darling(default)]
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
