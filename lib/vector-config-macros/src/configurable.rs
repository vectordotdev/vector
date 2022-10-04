use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, spanned::Spanned, token::Colon2, DeriveInput, ExprPath, Ident,
    PathArguments, Type,
};
use vector_config_common::{attributes::CustomAttribute, validation::Validation};

use crate::ast::{Container, Data, Field, Style, Tagging, Variant};

pub fn derive_configurable_impl(input: TokenStream) -> TokenStream {
    // Parse our input token stream as a derive input, and process the container, and the
    // container's children, that the macro is applied to.
    let input = parse_macro_input!(input as DeriveInput);
    let container = match Container::from_derive_input(&input) {
        Ok(container) => container,
        Err(e) => {
            // This should only occur when used on a union, as that's the only time `serde` will get
            // angry enough to not parse the derive AST at all, so we just return the context errors
            // we have, which will say as much, because also, if it gave us `None`, it should have
            // registered an error in the context as well.
            return e.write_errors().into();
        }
    };

    let mut generics = container.generics().clone();

    // We need to construct an updated where clause that properly constrains any generic types which are used as fields
    // on the container. We _only_ care about fields that are pure generic types, because anything that's a concrete
    // type -- Foo<T> -- will be checked when the schema is generated, but we want generic types to be able to be
    // resolved for compatibility at the point of usage, not the point of definition.
    let generic_field_types = container.generic_field_types();
    if !generic_field_types.is_empty() {
        let where_clause = generics.make_where_clause();
        for typ in generic_field_types {
            let ty = &typ.ident;
            let predicate =
                parse_quote! { #ty: ::vector_config::Configurable + ::serde::Serialize };

            where_clause.predicates.push(predicate);
        }
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Now we can go ahead and actually generate the method bodies for our `Configurable` impl,
    // which are varied based on whether we have a struct or enum container.
    let metadata_fn = build_metadata_fn(&container);
    let generate_schema_fn = match container.virtual_newtype() {
        Some(virtual_ty) => build_virtual_newtype_schema_fn(virtual_ty),
        None => match container.data() {
            Data::Struct(style, fields) => {
                build_struct_generate_schema_fn(&container, style, fields)
            }
            Data::Enum(variants) => build_enum_generate_schema_fn(variants),
        },
    };

    let name = container.ident();
    let ref_name = container.name();
    let configurable_impl = quote! {
        const _: () = {
            #[automatically_derived]
            #[allow(unused_qualifications)]
            impl #impl_generics ::vector_config::Configurable for #name #ty_generics #where_clause {
                fn referenceable_name() -> Option<&'static str> {
                    // If the type name we get back from `std::any::type_name` doesn't start with
                    // the module path, use a concatentated version.
                    //
                    // We do this because `std::any::type_name` states it may or may not return a
                    // fully-qualified type path, as that behavior is not stabilized, so we want to
                    // avoid using non-fully-qualified paths since we might encounter collisions
                    // with schema reference names otherwise.
                    //
                    // The reason we don't _only_ use the manually-concatentated version is because
                    // it's a little difficult to get it to emit a clean name, as we can't emit
                    // pretty-printed tokens directly -- i.e. just emit the tokens that represent
                    // `MyStructName<T, U, ...>` -- and would need to format the string to do so,
                    // which would mean we wouldn't be able to return `&'static str`.
                    //
                    // We'll likely relax that in the future, given the inconsequential nature of
                    // allocations during configuration schema generation... but this works well for
                    // now and at least will be consistent within the same Rust version.

                    let self_type_name = ::std::any::type_name::<Self>();
                    if !self_type_name.starts_with(std::module_path!()) {
                        Some(std::concat!(std::module_path!(), "::", #ref_name))
                    } else {
                        Some(self_type_name)
                    }
                }

                #metadata_fn

                #generate_schema_fn
            }
        };
    };

    configurable_impl.into()
}

fn build_metadata_fn(container: &Container<'_>) -> proc_macro2::TokenStream {
    let meta_ident = Ident::new("metadata", Span::call_site());
    let container_metadata = generate_container_metadata(&meta_ident, container);

    quote! {
        fn metadata() -> ::vector_config::Metadata<Self> {
            #container_metadata
            #meta_ident
        }
    }
}

fn build_virtual_newtype_schema_fn(virtual_ty: Type) -> proc_macro2::TokenStream {
    quote! {
        fn generate_schema(schema_gen: &mut ::vector_config::schemars::gen::SchemaGenerator) -> std::result::Result<::vector_config::schemars::schema::SchemaObject, ::vector_config::GenerateError> {
            // Virtual newtypes always shuttle their schema's metadata/overridden metadata when generating the schema
            // for the wrapped type, otherwise we wouldn't be able to effectively document them. This does mean we end
            // up dropping any default value for _this_ schema's metadata, including overridden metadata, so the wrapped
            // type must have a default value for itself if having a default value is required.
            let metadata = <Self as ::vector_config::Configurable>::metadata().convert();

            ::vector_config::schema::get_or_generate_schema::<#virtual_ty>(schema_gen, metadata)
        }
    }
}

fn build_enum_generate_schema_fn(variants: &[Variant<'_>]) -> proc_macro2::TokenStream {
    let mapped_variants = variants
        .iter()
        // Don't map this variant if it's marked to be skipped for both serialization and deserialization.
        .filter(|variant| variant.visible())
        .map(generate_enum_variant_schema);

    quote! {
        fn generate_schema(schema_gen: &mut ::vector_config::schemars::gen::SchemaGenerator) -> std::result::Result<::vector_config::schemars::schema::SchemaObject, ::vector_config::GenerateError> {
            let mut subschemas = ::std::vec::Vec::new();

            let schema_metadata = <Self as ::vector_config::Configurable>::metadata();
            #(#mapped_variants)*

            let mut schema = ::vector_config::schema::generate_one_of_schema(&subschemas);
            ::vector_config::schema::apply_metadata(&mut schema, schema_metadata);

            Ok(schema)
        }
    }
}

fn build_struct_generate_schema_fn(
    container: &Container<'_>,
    style: &Style,
    fields: &[Field<'_>],
) -> proc_macro2::TokenStream {
    match style {
        Style::Struct => build_named_struct_generate_schema_fn(container, fields),
        Style::Tuple => build_tuple_struct_generate_schema_fn(fields),
        Style::Newtype => build_newtype_struct_generate_schema_fn(fields),
        Style::Unit => panic!("unit structs should be rejected during AST parsing"),
    }
}

fn generate_struct_field(field: &Field<'_>) -> proc_macro2::TokenStream {
    let field_metadata_ref = Ident::new("field_metadata", Span::call_site());
    let field_metadata = generate_field_metadata(&field_metadata_ref, field);

    let spanned_generate_schema = quote_spanned! {field.span()=>
        ::vector_config::schema::get_or_generate_schema(schema_gen, #field_metadata_ref.as_subschema())?
    };

    quote! {
        #field_metadata

        let mut subschema = #spanned_generate_schema;
        ::vector_config::schema::apply_metadata(&mut subschema, #field_metadata_ref);
    }
}

fn generate_named_struct_field(
    container: &Container<'_>,
    field: &Field<'_>,
) -> proc_macro2::TokenStream {
    let field_name = field
        .ident()
        .expect("named struct fields must always have an ident");
    let field_schema_ty = get_field_schema_ty(field);
    let field_already_contained = format!(
        "schema properties already contained entry for `{}`, this should not occur",
        field_name
    );
    let field_key = field.name();

    let field_schema = generate_struct_field(field);

    // If the field is flattened, we store it into a different list of flattened subschemas vs adding it directly as a
    // field via `properties`/`required`.
    //
    // If any flattened subschemas are present when we generate the struct schema overall, we do the merging of those at
    // the end.
    let integrate_field = if field.flatten() {
        quote! {
            flattened_subschemas.push(subschema);
        }
    } else {
        // If there is no default value specified for either the field itself, or the container the
        // field is a part of, then we consider it required unless the field type itself is inherently
        // optional, such as being `Option<T>`.
        let spanned_is_optional = quote_spanned! {field.span()=>
            <#field_schema_ty as ::vector_config::Configurable>::is_optional()
        };
        let maybe_field_required =
            if container.default_value().is_none() && field.default_value().is_none() {
                Some(quote! {
                    if !#spanned_is_optional {
                        assert!(required.insert(#field_key.to_string()), #field_already_contained);
                    }
                })
            } else {
                None
            };

        quote! {
            if let Some(_) = properties.insert(#field_key.to_string(), subschema) {
                panic!(#field_already_contained);
            }

            #maybe_field_required
        }
    };

    quote! {
        {
            #field_schema
            #integrate_field
        }
    }
}

fn generate_tuple_struct_field(field: &Field<'_>) -> proc_macro2::TokenStream {
    let field_schema = generate_struct_field(field);

    quote! {
        {
            #field_schema
            subschemas.push(subschema);
        }
    }
}

fn build_named_struct_generate_schema_fn(
    container: &Container<'_>,
    fields: &[Field<'_>],
) -> proc_macro2::TokenStream {
    let mapped_fields = fields
        .iter()
        // Don't map this field if it's marked to be skipped for both serialization and deserialization.
        .filter(|field| field.visible())
        .map(|field| generate_named_struct_field(container, field));

    quote! {
        fn generate_schema(schema_gen: &mut ::vector_config::schemars::gen::SchemaGenerator) -> std::result::Result<::vector_config::schemars::schema::SchemaObject, ::vector_config::GenerateError> {
            let mut properties = ::vector_config::indexmap::IndexMap::new();
            let mut required = ::std::collections::BTreeSet::new();
            let mut flattened_subschemas = ::std::vec::Vec::new();

            let metadata = <Self as ::vector_config::Configurable>::metadata();
            #(#mapped_fields)*

            let additional_properties = None;
            let mut schema = ::vector_config::schema::generate_struct_schema(
                properties,
                required,
                additional_properties,
            );

            // If we have any flattened subschemas, deal with them now.
            if !flattened_subschemas.is_empty() {
                ::vector_config::schema::convert_to_flattened_schema(&mut schema, flattened_subschemas);
            }

            ::vector_config::schema::apply_metadata(&mut schema, metadata);

            Ok(schema)
        }
    }
}

fn build_tuple_struct_generate_schema_fn(fields: &[Field<'_>]) -> proc_macro2::TokenStream {
    let mapped_fields = fields
        .iter()
        // Don't map this field if it's marked to be skipped for both serialization and deserialization.
        .filter(|field| field.visible())
        .map(generate_tuple_struct_field);

    quote! {
        fn generate_schema(schema_gen: &mut ::vector_config::schemars::gen::SchemaGenerator) -> std::result::Result<::vector_config::schemars::schema::SchemaObject, ::vector_config::GenerateError> {
            let mut subschemas = ::std::collections::Vec::new();

            let metadata = <Self as ::vector_config::Configurable>::metadata();
            #(#mapped_fields)*

            let mut schema = ::vector_config::schema::generate_tuple_schema(&subschemas);
            ::vector_config::schema::apply_metadata(&mut schema, metadata);

            Ok(schema)
        }
    }
}

fn build_newtype_struct_generate_schema_fn(fields: &[Field<'_>]) -> proc_macro2::TokenStream {
    // Map the fields normally, but we should end up with a single field at the end.
    let mut mapped_fields = fields
        .iter()
        // Don't map this field if it's marked to be skipped for both serialization and deserialization.
        .filter(|field| field.visible())
        .map(generate_struct_field)
        .collect::<Vec<_>>();

    if mapped_fields.len() != 1 {
        panic!("newtype structs should never have more than one field");
    }

    let field_schema = mapped_fields.remove(0);

    quote! {
        fn generate_schema(schema_gen: &mut ::vector_config::schemars::gen::SchemaGenerator) -> std::result::Result<::vector_config::schemars::schema::SchemaObject, ::vector_config::GenerateError> {
            let metadata = <Self as ::vector_config::Configurable>::metadata();

            #field_schema
            ::vector_config::schema::apply_metadata(&mut subschema, metadata);

            Ok(subschema)
        }
    }
}

fn generate_container_metadata(
    meta_ident: &Ident,
    container: &Container<'_>,
) -> proc_macro2::TokenStream {
    let maybe_title = get_metadata_title(meta_ident, container.title());
    let maybe_description = get_metadata_description(meta_ident, container.description());
    let maybe_default_value = get_metadata_default_value(meta_ident, container.default_value());
    let maybe_deprecated = get_metadata_deprecated(meta_ident, container.deprecated());
    let maybe_custom_attributes = get_metadata_custom_attributes(meta_ident, container.metadata());

    quote! {
        let mut #meta_ident = ::vector_config::Metadata::default();
        #maybe_title
        #maybe_description
        #maybe_default_value
        #maybe_deprecated
        #maybe_custom_attributes
    }
}

fn generate_field_metadata(meta_ident: &Ident, field: &Field<'_>) -> proc_macro2::TokenStream {
    let field_ty = field.ty();
    let field_schema_ty = get_field_schema_ty(field);

    let spanned_metadata = quote_spanned! {field.span()=>
        <#field_schema_ty as ::vector_config::Configurable>::metadata()
    };

    let maybe_title = get_metadata_title(meta_ident, field.title());
    let maybe_description = get_metadata_description(meta_ident, field.description());
    let maybe_default_value = if field_ty != field_schema_ty {
        get_metadata_default_value_delegated(meta_ident, field_schema_ty, field.default_value())
    } else {
        get_metadata_default_value(meta_ident, field.default_value())
    };
    let maybe_deprecated = get_metadata_deprecated(meta_ident, field.deprecated());
    let maybe_transparent = get_metadata_transparent(meta_ident, field.transparent());
    let maybe_validation = get_metadata_validation(meta_ident, field.validation());
    let maybe_custom_attributes = get_metadata_custom_attributes(meta_ident, field.metadata());

    quote! {
        let mut #meta_ident = #spanned_metadata;
        #maybe_title
        #maybe_description
        #maybe_default_value
        #maybe_deprecated
        #maybe_transparent
        #maybe_validation
        #maybe_custom_attributes
    }
}

fn generate_variant_metadata(
    meta_ident: &Ident,
    variant: &Variant<'_>,
) -> proc_macro2::TokenStream {
    let maybe_title = get_metadata_title(meta_ident, variant.title());
    let maybe_description = get_metadata_description(meta_ident, variant.description());
    let maybe_deprecated = get_metadata_deprecated(meta_ident, variant.deprecated());

    // We have to mark variants as transparent, so that if we're dealing with an untagged enum, we
    // don't panic if their description is intentionally left out.
    let maybe_transparent =
        get_metadata_transparent(meta_ident, variant.tagging() == &Tagging::None);
    let maybe_custom_attributes = get_metadata_custom_attributes(meta_ident, variant.metadata());

    // We add a special metadata key (`logical_name`) that informs consumers of the schema what the
    // variant name is for this variant's subschema. While the doc comments being coerced into title
    // and/or description are typically good enough, sometimes we need a more mechanical mapping of
    // the variant's name since shoving it into the title would mean doc comments with redundant
    // information.
    //
    // You can think of this as an enum-specific additional title.
    let logical_name_attrs = vec![CustomAttribute::KeyValue {
        key: "logical_name".to_string(),
        value: variant.ident().to_string(),
    }];
    let variant_logical_name =
        get_metadata_custom_attributes(meta_ident, logical_name_attrs.into_iter());

    // We specifically use `()` as the type here because we need to generate the metadata for this
    // variant, but there's no unique concrete type for a variant, only the type of the enum
    // container it exists within. We also don't want to use the metadata of the enum container, as
    // it might have values that would conflict with the metadata of this specific variant.
    quote! {
        let mut #meta_ident = ::vector_config::Metadata::<()>::default();
        #maybe_title
        #maybe_description
        #maybe_deprecated
        #maybe_transparent
        #maybe_custom_attributes
        #variant_logical_name
    }
}

fn get_metadata_title(
    meta_ident: &Ident,
    title: Option<&String>,
) -> Option<proc_macro2::TokenStream> {
    title.map(|title| {
        quote! {
            #meta_ident.set_title(#title);
        }
    })
}

fn get_metadata_description(
    meta_ident: &Ident,
    description: Option<&String>,
) -> Option<proc_macro2::TokenStream> {
    description.map(|description| {
        quote! {
            #meta_ident.set_description(#description);
        }
    })
}

fn get_metadata_default_value(
    meta_ident: &Ident,
    default_value: Option<ExprPath>,
) -> Option<proc_macro2::TokenStream> {
    default_value.map(|value| {
        quote! {
            #meta_ident.set_default_value(#value());
        }
    })
}

fn get_metadata_default_value_delegated(
    meta_ident: &Ident,
    default_ty: &syn::Type,
    default_value: Option<ExprPath>,
) -> Option<proc_macro2::TokenStream> {
    default_value.map(|value| {
        let default_ty = get_ty_for_expr_pos(default_ty);

        quote! {
            #meta_ident.set_default_value(#default_ty::from(#value()));
        }
    })
}

fn get_metadata_deprecated(
    meta_ident: &Ident,
    deprecated: bool,
) -> Option<proc_macro2::TokenStream> {
    deprecated.then(|| {
        quote! {
            #meta_ident.set_deprecated();
        }
    })
}

fn get_metadata_transparent(
    meta_ident: &Ident,
    transparent: bool,
) -> Option<proc_macro2::TokenStream> {
    transparent.then(|| {
        quote! {
            #meta_ident.set_transparent();
        }
    })
}

fn get_metadata_validation(
    meta_ident: &Ident,
    validation: &[Validation],
) -> proc_macro2::TokenStream {
    let mapped_validation = validation
        .iter()
        .map(|v| quote! { #meta_ident.add_validation(#v); });

    quote! {
        #(#mapped_validation)*
    }
}

fn get_metadata_custom_attributes(
    meta_ident: &Ident,
    custom_attributes: impl Iterator<Item = CustomAttribute>,
) -> proc_macro2::TokenStream {
    let mapped_custom_attributes = custom_attributes
        .map(|attr| match attr {
            CustomAttribute::Flag(key) => quote! {
                #meta_ident.add_custom_attribute(::vector_config_common::attributes::CustomAttribute::Flag(#key.to_string()));
            },
            CustomAttribute::KeyValue { key, value } => quote! {
                #meta_ident.add_custom_attribute(::vector_config_common::attributes::CustomAttribute::KeyValue {
                    key: #key.to_string(),
                    value: #value.to_string(),
                });
            },
        });

    quote! {
        #(#mapped_custom_attributes)*
    }
}

fn get_field_schema_ty<'a>(field: &'a Field<'a>) -> &'a syn::Type {
    // If there's a delegated type being used for field (de)serialization, that's ultimately the type
    // we use to declare the schema, because we have to generate the schema for whatever type is
    // actually being (de)serialized, not the final type that the intermediate value ends up getting
    // converted to.
    //
    // Otherwise, we just use the actual field type.
    field.delegated_ty().unwrap_or_else(|| field.ty())
}

fn generate_named_enum_field(field: &Field<'_>) -> proc_macro2::TokenStream {
    let field_name = field.ident().expect("field should be named");
    let field_ty = field.ty();
    let field_already_contained = format!(
        "schema properties already contained entry for `{}`, this should not occur",
        field_name
    );
    let field_key = field.name().to_string();

    let field_schema = generate_struct_field(field);

    // Fields that have no default value are inherently required.  Unlike fields on a normal
    // struct, we can't derive a default value for an individual field because `serde`
    // doesn't allow even specifying a default value for an enum overall, only structs.
    let spanned_is_optional = quote_spanned! {field.span()=>
        <#field_ty as ::vector_config::Configurable>::is_optional()
    };
    let maybe_field_required = if field.default_value().is_none() {
        Some(quote! {
        if !#spanned_is_optional {
                if !required.insert(#field_key.to_string()) {
                    panic!(#field_already_contained);
                }
            }
        })
    } else {
        None
    };

    quote! {
        {
            #field_schema

            if let Some(_) = properties.insert(#field_key.to_string(), subschema) {
                panic!(#field_already_contained);
            }

            #maybe_field_required
        }
    }
}

fn generate_enum_struct_named_variant_schema(
    variant: &Variant<'_>,
    post_fields: Option<proc_macro2::TokenStream>,
) -> proc_macro2::TokenStream {
    let mapped_fields = variant.fields().iter().map(generate_named_enum_field);

    quote! {
        let mut properties = ::vector_config::indexmap::IndexMap::new();
        let mut required = ::std::collections::BTreeSet::new();

        #(#mapped_fields)*

        #post_fields

        let mut subschema = ::vector_config::schema::generate_struct_schema(
            properties,
            required,
            None
        );
    }
}

fn generate_enum_newtype_struct_variant_schema(variant: &Variant<'_>) -> proc_macro2::TokenStream {
    // When we only have a single unnamed field, we basically just treat it as a
    // passthrough, and we generate the schema for that field directly, without any
    // metadata or anything, since things like defaults can't travel from the enum
    // container to a specific variant anyways.
    let field = variant.fields().first().expect("must exist");
    generate_struct_field(field)
}

fn generate_enum_unit_variant_schema(variant: &Variant<'_>) -> proc_macro2::TokenStream {
    let variant_name = variant.name();

    quote! {
        let mut subschema = ::vector_config::schema::generate_const_string_schema(#variant_name.to_string());
    }
}

fn generate_enum_variant_schema(variant: &Variant<'_>) -> proc_macro2::TokenStream {
    // For the sake of all examples below, we'll use JSON syntax to represent the following enum
    // variants:
    //
    // enum ExampleEnum {
    //   Struct { some_field: bool },
    //   Unnamed(bool),
    //   Unit,
    // }
    let variant_name = variant.name();
    let apply_variant_metadata = generate_enum_variant_apply_metadata(variant);

    match variant.tagging() {
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
        Tagging::External => {
            let (wrapped, variant_schema) = match variant.style() {
                Style::Struct => (
                    true,
                    generate_enum_struct_named_variant_schema(variant, None),
                ),
                Style::Tuple => panic!("tuple variants should be rejected during AST parsing"),
                Style::Newtype => (true, generate_enum_newtype_struct_variant_schema(variant)),
                Style::Unit => (false, generate_enum_unit_variant_schema(variant)),
            };

            // In external mode, we don't wrap the schema for unit variants, because they're
            // interpreted directly as the value of the field using the enum.
            //
            // TODO: we can maybe reuse the existing struct schema gen stuff here, but we'd need
            // a way to force being required + customized metadata
            let variant_schema = if wrapped {
                quote! {
                    #variant_schema

                    let mut wrapper_properties = ::vector_config::indexmap::IndexMap::new();
                    let mut wrapper_required = ::std::collections::BTreeSet::new();

                    wrapper_properties.insert(#variant_name.to_string(), subschema);
                    wrapper_required.insert(#variant_name.to_string());

                    let mut subschema = ::vector_config::schema::generate_struct_schema(
                        wrapper_properties,
                        wrapper_required,
                        None
                    );
                }
            } else {
                variant_schema
            };

            generate_enum_variant_subschema(variant, variant_schema)
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
        Tagging::Internal { tag } => match variant.style() {
            Style::Struct => {
                let tag_already_contained = format!("enum tag `{}` already contained as a field in variant; tag cannot overlap with any fields in any variant", tag);

                // Just generate the tag field directly and pass it along to be included in the
                // struct schema.
                let tag_field = quote! {
                    {
                        let mut subschema = ::vector_config::schema::generate_const_string_schema(#variant_name.to_string());
                        #apply_variant_metadata

                        if let Some(_) = properties.insert(#tag.to_string(), subschema) {
                            panic!(#tag_already_contained);
                        }

                        if !required.insert(#tag.to_string()) {
                            panic!(#tag_already_contained);
                        }
                    }
                };
                let variant_schema =
                    generate_enum_struct_named_variant_schema(variant, Some(tag_field));

                generate_enum_variant_subschema(variant, variant_schema)
            }
            Style::Tuple => panic!("tuple variants should be rejected during AST parsing"),
            Style::Newtype => {
                // We have to delegate viability to `serde`, essentially, because using internal tagging for a newtype
                // variant is only possible when the inner field is a struct or map, and we can't access that type of
                // information here, which is why `serde` does it at compile-time.

                // As such, we generate the schema for the single field, like we would normally do for a newtype
                // variant, and then we follow the struct flattening logic where we layer on our tag field schema on the
                // schema of the wrapped field... and since it has to be a struct or map to be valid for `serde`, that
                // means it will also be an object schema in both cases, which means our flatteneing logic will be
                // correct if the caller is doing The Right Thing (tm).
                let wrapped_variant_schema = generate_enum_newtype_struct_variant_schema(variant);

                let variant_schema = quote! {
                    let mut subschema = {
                        let tag_schema = ::vector_config::schema::generate_internal_tagged_variant_schema(#tag.to_string(), #variant_name.to_string());
                        let mut flattened_subschemas = ::std::vec::Vec::new();
                        flattened_subschemas.push(tag_schema);

                        #wrapped_variant_schema

                        ::vector_config::schema::convert_to_flattened_schema(&mut subschema, flattened_subschemas);

                        subschema
                    };
                };

                generate_enum_variant_subschema(variant, variant_schema)
            }
            Style::Unit => {
                // Internally-tagged unit variants are basically just a play on externally-tagged
                // struct variants.
                let variant_schema = generate_enum_unit_variant_schema(variant);
                let variant_schema = quote! {
                    #variant_schema

                    let mut wrapper_properties = ::vector_config::indexmap::IndexMap::new();
                    let mut wrapper_required = ::std::collections::BTreeSet::new();

                    wrapper_properties.insert(#tag.to_string(), subschema);
                    wrapper_required.insert(#tag.to_string());

                    let mut subschema = ::vector_config::schema::generate_struct_schema(
                        wrapper_properties,
                        wrapper_required,
                        None
                    );
                };

                generate_enum_variant_subschema(variant, variant_schema)
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
        Tagging::Adjacent { tag, content } => {
            // For struct-type variants, just generate their schema as normal, and we'll wrap it up
            // in a new object.  For unit variants, adjacent tagging is identical to internal
            // tagging, so we handle that one by hand.
            let tag_schema = generate_enum_unit_variant_schema(variant);
            let maybe_variant_schema = match variant.style() {
                Style::Struct => Some(generate_enum_struct_named_variant_schema(variant, None)),
                Style::Tuple => panic!("tuple variants should be rejected during AST parsing"),
                Style::Newtype => Some(generate_enum_newtype_struct_variant_schema(variant)),
                Style::Unit => None,
            }
            .map(|schema| {
                quote! {
                    #schema
                    wrapper_properties.insert(#content.to_string(), subschema);
                    wrapper_required.insert(#content.to_string());
                }
            });

            let apply_variant_metadata = generate_enum_variant_apply_metadata(variant);

            quote! {
                {
                    let mut wrapper_properties = ::vector_config::indexmap::IndexMap::new();
                    let mut wrapper_required = ::std::collections::BTreeSet::new();

                    #tag_schema
                    wrapper_properties.insert(#tag.to_string(), subschema);
                    wrapper_required.insert(#tag.to_string());

                    #maybe_variant_schema

                    let mut subschema = ::vector_config::schema::generate_struct_schema(
                        wrapper_properties,
                        wrapper_required,
                        None
                    );
                    #apply_variant_metadata

                    subschemas.push(subschema);
                }
            }
        }
        Tagging::None => {
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

            let variant_schema = match variant.style() {
                Style::Struct => generate_enum_struct_named_variant_schema(variant, None),
                Style::Tuple => panic!("tuple variants should be rejected during AST parsing"),
                Style::Newtype => generate_enum_newtype_struct_variant_schema(variant),
                Style::Unit => {
                    quote! { let mut subschema = ::vector_config::schema::generate_null_schema(); }
                }
            };

            generate_enum_variant_subschema(variant, variant_schema)
        }
    }
}

fn generate_enum_variant_apply_metadata(variant: &Variant<'_>) -> proc_macro2::TokenStream {
    let variant_metadata_ref = Ident::new("variant_metadata", Span::call_site());
    let variant_metadata = generate_variant_metadata(&variant_metadata_ref, variant);

    quote! {
        #variant_metadata
        ::vector_config::schema::apply_metadata(&mut subschema, #variant_metadata_ref);
    }
}

fn generate_enum_variant_subschema(
    variant: &Variant<'_>,
    variant_schema: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let apply_variant_metadata = generate_enum_variant_apply_metadata(variant);

    quote! {
        {
            #variant_schema
            #apply_variant_metadata

            subschemas.push(subschema);
        }
    }
}

/// Gets a type token suitable for use in expression position.
///
/// Normally, we refer to types with generic type parameters using their condensed form: `T<...>`.
/// Sometimes, however, we must refer to them with their disambiguated form: `T::<...>`. This is due
/// to a limitation in syntax parsing between types in statement versus expression position.
///
/// Statement position would be somehwere like declaring a field on a struct, where using angle
/// brackets has no ambiguous meaning, as you can't compare two items as part of declaring a struct
/// field. Conversely, expression position implies anywhere we could normally provide an expression,
/// and expressions can certainly contain comparisons. As such, we need to use the disambiguated
/// form in expression position.
///
/// While most commonly used for passing generic type parameters to functions/methods themselves,
/// this is also known as the "turbofish" syntax.
fn get_ty_for_expr_pos(ty: &syn::Type) -> syn::Type {
    match ty {
        syn::Type::Path(tp) => {
            let mut new_tp = tp.clone();
            for segment in new_tp.path.segments.iter_mut() {
                if let PathArguments::AngleBracketed(ab) = &mut segment.arguments {
                    ab.colon2_token = Some(Colon2::default());
                }
            }

            syn::Type::Path(new_tp)
        }
        _ => ty.clone(),
    }
}
