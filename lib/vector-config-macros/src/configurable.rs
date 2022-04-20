use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde_derive_internals::{attr::TagType, Ctxt};
use syn::{
    parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, Error, Fields, FieldsNamed,
    FieldsUnnamed, GenericParam, Ident, Lifetime, LifetimeDef,
};

use crate::{
    attrs::{ContainerAttributes, FieldAttributes, VariantAttributes},
    errors_to_tokenstream,
};

pub fn derive_configurable_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let context = Ctxt::new();

    // Extract all of the container attributes from the input.
    let container = ContainerAttributes::new(&context, &input);

    // We build the "impl" generics separately from the "type" generics, because the lifetime for
    // `Configurable` only matters to `impl`, not to the type that `Configurable` is being
    // implemented on, and we can't add it after calling `split_for_impl`.
    //
    // Essentially, we want to see this:
    //
    //    impl<'conf, ['a, 'b, ...]> Configurable<'conf> for Struct<['a, 'b, ...]>
    //
    // but if we added `'conf` to `generics` first, we would actually end up with the following:
    //
    //     impl<'conf, ['a, 'b, ...]> Configurable<'conf> for Struct<'conf, ['a, 'b, ...]>
    //
    // which isn't right because `'conf` is not actually a part of `Struct`.
    let mut modified_generics = input.generics.clone();
    let (clt, clt_def) = get_configurable_lifetime();
    modified_generics
        .params
        .push(GenericParam::Lifetime(clt_def));
    let (impl_generics, _, _) = modified_generics.split_for_impl();
    let (_, ty_generics, where_clause) = input.generics.split_for_impl();

    // Build the `shape` and `generate_schema` functions, and then slap it all together for the
    // final `Configurable` impl.  This varies depending on whether we're deriving for an enum or a
    // struct with named fields.
    let generate_schema_fn = match input.data {
        Data::Struct(DataStruct {
            fields: Fields::Named(fields),
            ..
        }) => {
            let fields = fields
                .named
                .iter()
                .enumerate()
                .map(|(i, field)| FieldAttributes::from_container(&context, &container, field, i))
                .collect::<Vec<_>>();

            build_struct_generate_schema_fn(&container, &fields)
        }
        Data::Enum(DataEnum { variants, .. }) => {
            let variants = variants
                .iter()
                .map(|variant| VariantAttributes::new(&context, variant))
                .collect::<Vec<_>>();

            build_enum_generate_schema_fn(&context, &container, &variants)
        }
        _ => {
            return Error::new(
                Span::call_site(),
                "`Configurable` can only be derived on structs with named fields and enums",
            )
            .into_compile_error()
            .into()
        }
    };
    let metadata_fn = build_metadata_fn(&context, &container);

    if let Err(errs) = context.check() {
        return errors_to_tokenstream(errs);
    }

    // All of the input so far has been valid, so we can simply output our implementation of `Configurable`.
    let name = input.ident;
    let ref_name = name.to_string();
    let configurable_impl = quote! {
        const _: () = {
            #[automatically_derived]
            impl #impl_generics ::vector_config::Configurable<#clt> for #name #ty_generics #where_clause {
                fn referencable_name() -> Option<&'static str> {
                    Some(#ref_name)
                }

                #metadata_fn

                #generate_schema_fn
            }
        };
    };
    configurable_impl.into()
}

fn build_metadata_fn(_context: &Ctxt, container: &ContainerAttributes) -> proc_macro2::TokenStream {
    let (clt, _) = get_configurable_lifetime();

    let meta_ident = Ident::new("metadata", Span::call_site());
    let container_metadata = generate_container_metadata(&meta_ident, container);

    quote! {
        fn metadata() -> ::vector_config::Metadata<#clt, Self> {
            #container_metadata
            #meta_ident
        }
    }
}

fn build_enum_generate_schema_fn(
    context: &Ctxt,
    container: &ContainerAttributes,
    variants: &[VariantAttributes],
) -> proc_macro2::TokenStream {
    let (clt, _) = get_configurable_lifetime();

    let mapped_variants = variants.into_iter()
        // Don't map this variant if it's marked to be skipped for both serialization and deserialization.
        .filter(|variant| variant.visible())
        // All variants must have their own description, even if it's a newtype variant (i.e.
        // `Variant(some_value)`) as it makes the code much simpler in terms of generating the schema.
        .filter(|variant| if variant.description.is_none() {
            context.error_spanned_by(&variant.variant, "enum variants must always have a valid description i.e. `/// Description of variant...` or `#[configurable(description = \"Description of variant...\")]`");
            false
        } else {
            true
        })
        // Struct variants cannot have more than a single unnamed field.  In other words, they can
        // only be used as a newtype wrapper.
        .filter(|variant| match &variant.variant.fields {
			Fields::Unnamed(fields) if fields.unnamed.len() > 1 => {
				context.error_spanned_by(fields, "enum variants with more than one unnamed field are not supported");
                false
			},
            _ => true,
        })
        .map(|variant| generate_enum_variant_schema(context, container, variant));

    quote! {
        fn generate_schema(schema_gen: &mut ::schemars::gen::SchemaGenerator, overrides: ::vector_config::Metadata<#clt, Self>) -> ::schemars::schema::SchemaObject {
            let mut subschemas = ::std::vec::Vec::new();

            let schema_metadata = Self::metadata().merge(overrides);
            #(#mapped_variants)*

            let mut schema = ::vector_config::schema::generate_composite_schema(schema_gen, &subschemas);
            ::vector_config::schema::finalize_schema(schema_gen, &mut schema, schema_metadata);

            schema
        }
    }
}

fn build_struct_generate_schema_fn(
    container: &ContainerAttributes,
    fields: &[FieldAttributes],
) -> proc_macro2::TokenStream {
    let (clt, _) = get_configurable_lifetime();

    let mapped_fields = fields.into_iter()
        // Don't map this field if it's marked to be skipped for both serialization and deserialization.
        .filter(|field| field.visible())
        .map(|field| {
            let field_name = field.field.ident.clone().expect("only structs with named fields can derive `Configurable`");
			let field_as_configurable = get_field_type_as_configurable(field);
            let field_already_contained = format!("schema properties already contained entry for `{}`, this should not occur", field_name);
            let field_key = field.serde.name().deserialize_name();

            let field_metadata_ref = Ident::new("field_metadata", Span::call_site());
			let field_metadata = generate_field_metadata(&field_metadata_ref, field);

            // If there is no default value specified for either the field itself, or the container the
            // field is a part of, then we consider it required unless the field type itself is inherently
            // optional, such as being `Option<T>`.
            let field_required = if container.serde.default().is_none() && field.serde.default().is_none() {
                quote! {
                    if !#field_as_configurable::is_optional() {
                        if !schema_required.insert(#field_key.to_string()) {
							panic!(#field_already_contained);
						}
                    }
                }
            } else {
                quote! {}
            };

            quote! {
				{
					#field_metadata
					let mut field_schema = #field_as_configurable::generate_schema(schema_gen, #field_metadata_ref.clone());
					::vector_config::schema::finalize_schema(schema_gen, &mut field_schema, #field_metadata_ref);

					if let Some(_) = schema_properties.insert(#field_key.to_string(), field_schema) {
						panic!(#field_already_contained);
					}

					#field_required
				}
            }
        });

    quote! {
        fn generate_schema(schema_gen: &mut ::schemars::gen::SchemaGenerator, overrides: ::vector_config::Metadata<#clt, Self>) -> ::schemars::schema::SchemaObject {
            let mut schema_properties = ::indexmap::IndexMap::new();
            let mut schema_required = ::std::collections::BTreeSet::new();

            let schema_metadata = Self::metadata().merge(overrides);
            #(#mapped_fields)*

            // TODO: We need to figure out if we actually use `#[serde(flatten)]` anywhere in order
            // to capture not-specifically-named fields i.e. collecting all remaining/unknown fields
            // in a hashmap.
            //
            // That usage would drive `additional_properties` but I can we can currently ignore it
            // until we hit our first struct that needs it.
            let schema_additional_properties = None;
            let mut schema = ::vector_config::schema::generate_struct_schema(
                schema_gen,
                schema_properties,
                schema_required,
                schema_additional_properties,
            );
            ::vector_config::schema::finalize_schema(schema_gen, &mut schema, schema_metadata);

            schema
        }
    }
}

fn generate_container_metadata(
    meta_ident: &Ident,
    container: &ContainerAttributes,
) -> proc_macro2::TokenStream {
    // Set a container description if we have one.
    let container_description = get_metadata_description(meta_ident, &container.description);

    // Set a container default value if we have one.
    let container_default_value = get_metadata_default_value(meta_ident, container.serde.default());

    // Set any container custom attributes if we have any.
    let container_custom_attributes = container.metadata.iter().map(|(key, value)| {
        quote! {
            #meta_ident.add_custom_attribute(#key, #value);
        }
    });

    quote! {
        let mut #meta_ident = ::vector_config::Metadata::default();
        #container_description
        #container_default_value
        #(#container_custom_attributes)*
    }
}

fn generate_field_metadata(
    meta_ident: &Ident,
    field: &FieldAttributes,
) -> proc_macro2::TokenStream {
    let field_as_configurable = get_field_type_as_configurable(field);

    // Set a field description if we have one.
    let field_description = get_metadata_description(meta_ident, &field.description);

    // Set a field default value if we have one.
    let field_default = get_metadata_default_value(meta_ident, field.serde.default());

    // Set field transparency if enabled.
    let field_transparent = get_metadata_transparent(meta_ident, field.transparent);

    quote! {
        let mut #meta_ident = #field_as_configurable::metadata();
        #field_description
        #field_default
        #field_transparent
    }
}

fn get_field_type_as_configurable(field: &FieldAttributes) -> proc_macro2::TokenStream {
    let (clt, _) = get_configurable_lifetime();
    let field_ty = &field.field.ty;
    quote! { <#field_ty as ::vector_config::Configurable<#clt>> }
}

fn get_metadata_description(
    meta_ident: &Ident,
    description: &Option<String>,
) -> proc_macro2::TokenStream {
    description
        .as_ref()
        .map(|description| {
            quote! {
                #meta_ident.set_description(#description);
            }
        })
        .unwrap_or_default()
}

fn get_metadata_default_value(
    meta_ident: &Ident,
    default_value: &serde_derive_internals::attr::Default,
) -> proc_macro2::TokenStream {
    match default_value {
        serde_derive_internals::attr::Default::None => quote! {},
        serde_derive_internals::attr::Default::Default => quote! {
            #meta_ident.set_default_value(::std::default::Default::default());
        },
        serde_derive_internals::attr::Default::Path(expr) => quote! {
            #meta_ident.set_default_value(#expr());
        },
    }
}

fn get_metadata_transparent(meta_ident: &Ident, transparent: bool) -> proc_macro2::TokenStream {
    transparent
        .then(|| {
            quote! {
                #meta_ident.set_transparent();
            }
        })
        .unwrap_or_default()
}

fn generate_enum_struct_named_variant_schema(
    context: &Ctxt,
    variant: &VariantAttributes,
    fields: &FieldsNamed,
    post_fields: Option<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let mapped_fields = fields.named.iter()
        .enumerate()
        .map(|(i, field)| FieldAttributes::from_variant(&context, &variant, field, i))
        .map(|field| {
            let field_name = field.field.ident.clone().expect("only structs with named fields can derive `Configurable`");
			let field_as_configurable = get_field_type_as_configurable(&field);
            let field_already_contained = format!("schema properties already contained entry for `{}`, this should not occur", field_name);
            let field_key = field_name.to_string();

            let field_metadata_ref = Ident::new("field_metadata", Span::call_site());
            let field_metadata = generate_field_metadata(&field_metadata_ref, &field);

            // If there is no default value specified for either the field itself, or the container the
            // field is a part of, then we consider it required unless the field type itself is inherently
            // optional, such as being `Option<T>`.
            let field_required = if field.serde.default().is_none() {
                quote! {
                    if !#field_as_configurable::is_optional() {
                        if !subschema_required.insert(#field_key.to_string()) {
							panic!(#field_already_contained);
						}
                    }
                }
            } else {
                quote! {}
            };

            quote! {
				{
					#field_metadata
					let mut field_schema = #field_as_configurable::generate_schema(schema_gen, #field_metadata_ref.clone());
					::vector_config::schema::finalize_schema(schema_gen, &mut field_schema, #field_metadata_ref);

					if let Some(_) = subschema_properties.insert(#field_key.to_string(), field_schema) {
						panic!(#field_already_contained);
					}

					#field_required
				}
            }
        });

    quote! {
        let mut subschema_properties = ::indexmap::IndexMap::new();
        let mut subschema_required = ::std::collections::BTreeSet::new();

        let subschema_metadata = Self::metadata();
        #(#mapped_fields)*

        #post_fields

        let mut subschema = ::vector_config::schema::generate_struct_schema(
            schema_gen,
            subschema_properties,
            subschema_required,
            None
        );
    }
}

fn generate_enum_newtype_struct_variant_schema(
    context: &Ctxt,
    variant: &VariantAttributes,
    fields: &FieldsUnnamed,
) -> proc_macro2::TokenStream {
    // When we only have a single unnamed field, we basically just treat it as a
    // passthrough, and we generate the schema for that field directly, without any
    // metadata or anything, since things like defaults can't travel from the enum
    // container to a specific variant anyways.
    let field = fields.unnamed.first().expect("must exist");
    let field = FieldAttributes::from_variant(&context, &variant, field, 0);
    let field_as_configurable = get_field_type_as_configurable(&field);
    let field_metadata_ref = Ident::new("field_metadata", Span::call_site());
    let field_metadata = generate_field_metadata(&field_metadata_ref, &field);

    quote! {
        #field_metadata
        let mut subschema = #field_as_configurable::generate_schema(schema_gen, #field_metadata_ref.clone());
        ::vector_config::schema::finalize_schema(schema_gen, &mut subschema, #field_metadata_ref);
    }
}

fn generate_enum_unit_variant_schema(variant: &VariantAttributes) -> proc_macro2::TokenStream {
    let variant_name = variant.serde.name().deserialize_name();

    quote! {
        let mut subschema = ::vector_config::schema::generate_const_string_schema(#variant_name.to_string());
    }
}

fn generate_enum_variant_schema(
    context: &Ctxt,
    container: &ContainerAttributes,
    variant: &VariantAttributes,
) -> proc_macro2::TokenStream {
    // For the sake of all examples below, we'll use JSON syntax to represent the following enum
    // variants:
    //
    // enum ExampleEnum {
    //   Struct { some_field: bool },
    //   Unnamed(bool),
    //   Unit,
    // }
    let variant_name = variant.serde.name().deserialize_name();
    let variant_description = variant
        .description
        .clone()
        .expect("variants must always have a description");

    match container.serde.tag() {
        // The variant is represented "externally" by wrapping the contents of the variant as an
        // object pointed to by a property whose name is the name of the variant.
        //
        // This is when the rendered output looks like the following:
        //
        // # Struct form.
        // { "field_using_enum": { "VariantName": { "some_field": false } } }
        //
        // # Struct form with unnamed field.
        // { "field_using_enum": { "VariantName": false } }
        //
        // # Unit form.
        // { "field_using_enum": "VariantName" }
        TagType::External => {
            let (wrapped, variant_schema) = match &variant.variant.fields {
                Fields::Named(fields) => (
                    true,
                    generate_enum_struct_named_variant_schema(context, variant, fields, None),
                ),
                Fields::Unnamed(fields) => (
                    true,
                    generate_enum_newtype_struct_variant_schema(context, variant, fields),
                ),
                Fields::Unit => (false, generate_enum_unit_variant_schema(variant)),
            };

            // In external mode, we don't wrap the schema for unit variants, because they're
            // interpreted directly as the value of the field using the enum.
            if wrapped {
                quote! {
                    {
                        #variant_schema

                        let mut wrapper_properties = ::indexmap::IndexMap::new();
                        let mut wrapper_required = ::std::collections::BTreeSet::new();

                        wrapper_properties.insert(#variant_name.to_string(), subschema);
                        wrapper_required.insert(#variant_name.to_string());

                        let mut subschema = ::vector_config::schema::generate_struct_schema(
                            schema_gen,
                            wrapper_properties,
                            wrapper_required,
                            None
                        );
                        ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                        subschemas.push(subschema);
                    }
                }
            } else {
                quote! {
                    {
                        #variant_schema
                        ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                        subschemas.push(subschema);
                    }
                }
            }
        }
        // The variant is represented "internally" by adding a new property to the contents of the
        // variant whose name is the value of `tag` and must match the name of the variant.
        //
        // This is when the rendered output looks like the following:
        //
        // # Struct form.
        // { "field_using_enum": { "<tag>": "VariantName", "some_field": false } }
        //
        // # Struct form with unnamed field is not valid here.  See comments below.
        //
        // # Unit form.
        // { "field_using_enum": { "<tag>": "VariantName" } }
        TagType::Internal { tag } => match &variant.variant.fields {
            Fields::Named(fields) => {
                let tag_already_contained = format!("enum tag `{}` already contained as a field in variant; tag cannot overlap with any fields in any variant", tag);

                // Just generate the tag field directly and pass it along to be included in the
                // struct schema.
                let tag_field = quote! {
                    {
                        let mut field_schema = ::vector_config::schema::generate_const_string_schema(#variant_name.to_string());
                        ::vector_config::schema::apply_metadata(&mut field_schema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                        if let Some(_) = subschema_properties.insert(#tag.to_string(), field_schema) {
                            panic!(#tag_already_contained);
                        }

                        if !subschema_required.insert(#tag.to_string()) {
                            panic!(#tag_already_contained);
                        }
                    }
                };
                let variant_schema = generate_enum_struct_named_variant_schema(
                    context,
                    variant,
                    fields,
                    Some(tag_field),
                );

                quote! {
                    {
                        #variant_schema
                        ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                        subschemas.push(subschema);
                    }
                }
            }
            // Newtype variants in "internal" tagging mode are a special case, which even
            // `serde` handles by deferring to runtime error checking, as there's not enough
            // information to know for sure whether the `T` can actually hold the tag field.
            //
            // Technically, _we_ could check that by examining the schema after we generate it for
            // the inner field, and then we'd be at parity with `serde`, but it would be ugly
            // because we'd have to defer finalizing the schema so that we could use it inline
            // instead of as a reference, and yeah, it's just ugly, and so we choose not to do
            // it for now.
            //
            // In the future, we may want to put in the work to handle this case at the level
            // that `serde` handles it, but right now, I don't think it's a necessity to
            // actually handle this use case any better.
            Fields::Unnamed(_) => {
                context.error_spanned_by(&variant.variant, "newtype variants (i.e. `enum SomeEnum { SomeVariant(T) }`) cannot be used with tag-only mode as the type inside may or may not support embedding the tag field");
                return quote! {};
            }
            Fields::Unit => {
                // Internally-tagged unit variants are basically just a play on externally-tagged
                // struct variants.
                let variant_schema = generate_enum_unit_variant_schema(variant);

                quote! {
                    {
                        #variant_schema

                        let mut wrapper_properties = ::indexmap::IndexMap::new();
                        let mut wrapper_required = ::std::collections::BTreeSet::new();

                        wrapper_properties.insert(#tag.to_string(), subschema);
                        wrapper_required.insert(#tag.to_string());

                        let mut subschema = ::vector_config::schema::generate_struct_schema(
                            schema_gen,
                            wrapper_properties,
                            wrapper_required,
                            None
                        );
                        ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                        subschemas.push(subschema);
                    }
                }
            }
        },
        // The variant is represented "adjacent" to the content, such that the variant name is in a
        // field whose name is the value of `tag` and the content of the variant is in a field whose
        // name is the value of `content`.
        //
        // This is when the rendered output looks like the following:
        //
        // # Struct form.
        // { "field_using_enum": { "<tag>": "VariantName", "<content>": { "some_field": false } } }
        //
        // # Struct form with unnamed field.
        // { "field_using_enum": { "<tag>": "VariantName", "<content>": false } }
        //
        // # Unit form.
        // { "field_using_enum": { "<tag>": "VariantName" } }
        TagType::Adjacent { tag, content } => {
            // For struct-type variants, just generate their schema as normal, and we'll wrap it up
            // in a new object.  For unit variants, adjacent tagging is identical to internal
            // tagging, so we handle that one by hand.
            let tag_schema = generate_enum_unit_variant_schema(variant);
            let maybe_variant_schema = match &variant.variant.fields {
                Fields::Named(fields) => Some(generate_enum_struct_named_variant_schema(
                    context, variant, fields, None,
                )),
                Fields::Unnamed(fields) => Some(generate_enum_newtype_struct_variant_schema(
                    context, variant, fields,
                )),
                Fields::Unit => None,
            }
            .map(|schema| {
                quote! {
                    #schema
                    wrapper_properties.insert(#content.to_string(), subschema);
                    wrapper_required.insert(#content.to_string());
                }
            });

            quote! {
                {
                    let mut wrapper_properties = ::indexmap::IndexMap::new();
                    let mut wrapper_required = ::std::collections::BTreeSet::new();

                    #tag_schema
                    wrapper_properties.insert(#tag.to_string(), subschema);
                    wrapper_required.insert(#tag.to_string());

                    #maybe_variant_schema

                    let mut subschema = ::vector_config::schema::generate_struct_schema(
                        schema_gen,
                        wrapper_properties,
                        wrapper_required,
                        None
                    );
                    ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                    subschemas.push(subschema);
                }
            }
        }
        TagType::None => {
            // This is simply when it's a free-for-all and `serde` tries to deserialize the data as
            // each variant until it finds one that can deserialize the data correctly. Essentially,
            // we encode the variant solely based on its contents, which for a unit variant, would
            // be nothing: a literal `null` in JSON.
            //
            // Accordingly, there is a higher-level check before we get here that yells at the user
            // that using `#[serde(untagged)]` with an enum where some variants that have
            // duplicate contents, compared to their siblings, is not allowed because doing so
            // provides unstable deserialization.
            //
            // This is when the rendered output looks like the following:
            //
            // # Struct form.
            // { "field_using_enum": { "some_field": false } }
            //
            // # Struct form with unnamed field.
            // { "field_using_enum": false }
            //
            // # Unit form.
            // { "field_using_enum": null }
            //
            // TODO: actually implement the aforementioned higher-level check

            let variant_schema = match &variant.variant.fields {
                Fields::Named(fields) => {
                    generate_enum_struct_named_variant_schema(context, variant, fields, None)
                }
                Fields::Unnamed(fields) => {
                    generate_enum_newtype_struct_variant_schema(context, variant, fields)
                }
                Fields::Unit => {
                    quote! { let mut subschema = ::vector_config::schema::generate_null_schema(); }
                }
            };

            quote! {
                {
                    #variant_schema
                    ::vector_config::schema::apply_metadata(&mut subschema, ::vector_config::Metadata::<'_, ()>::with_description(#variant_description));

                    subschemas.push(subschema);
                }
            }
        }
    }
}

fn get_configurable_lifetime() -> (Lifetime, LifetimeDef) {
    let lifetime = Lifetime::new("'configurable", Span::call_site());
    let lifetime_def = LifetimeDef::new(lifetime.clone());

    (lifetime, lifetime_def)
}
