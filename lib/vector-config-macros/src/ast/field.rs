use darling::{
    util::{Flag, Override, SpannedValue},
    FromAttributes,
};
use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use serde_derive_internals::ast as serde_ast;
use syn::{parse_quote, ExprPath, Ident};
use vector_config_common::{configurable_package_name_hack, validation::Validation};

use super::{
    util::{
        err_field_implicit_transparent, err_field_missing_description,
        find_delegated_serde_deser_ty, get_serde_default_value, try_extract_doc_title_description,
    },
    LazyCustomAttribute, Metadata,
};

/// A field of a container.
pub struct Field<'a> {
    original: &'a syn::Field,
    name: String,
    default_value: Option<ExprPath>,
    attrs: Attributes,
}

impl<'a> Field<'a> {
    /// Creates a new `Field<'a>` from the `serde`-derived information about the given field.
    pub fn from_ast(
        serde: &serde_ast::Field<'a>,
        is_virtual_newtype: bool,
        is_newtype_wrapper_field: bool,
    ) -> darling::Result<Field<'a>> {
        let original = serde.original;

        let name = serde.attrs.name().deserialize_name().to_string();
        let default_value = get_serde_default_value(&serde.ty, serde.attrs.default());

        Attributes::from_attributes(&original.attrs)
            .and_then(|attrs| {
                attrs.finalize(
                    serde,
                    &original.attrs,
                    is_virtual_newtype,
                    is_newtype_wrapper_field,
                )
            })
            .map(|attrs| Field {
                original,
                name,
                default_value,
                attrs,
            })
    }

    /// Name of the field, if any.
    ///
    /// Fields of tuple structs have no names.
    pub fn ident(&self) -> Option<&Ident> {
        self.original.ident.as_ref()
    }

    /// Type of the field.
    ///
    /// This is the as-defined type, and may not necessarily match the type we use for generating
    /// the schema: see `delegated_ty` for more information.
    pub fn ty(&self) -> &syn::Type {
        &self.original.ty
    }

    /// Delegated type of the field, if any.
    ///
    /// In some cases, helper types may be used to provide (de)serialization of types that cannot
    /// have `Deserialize`/`Serialize`, such as types in the standard library, or may be used to
    /// provide customized (de)serialization, such (de)serializing to and from a more human-readable
    /// version of a type, like time strings that let you specify `1s` or `1 hour`, and don't just
    /// force you to always specify the total number of seconds and nanoseconds, and so on.
    ///
    /// When these helper types are in use, we need to be able to understand what _they_ look like
    /// when serialized so that our generated schema accurately reflects what we expect to get
    /// during deserialization. Even though we may end up with a `T` in our configuration type, if
    /// we're (de)serializing it like a `U`, then we care about `U` when generating the schema, not `T`.
    ///
    /// We currently scope this type to helper types defined with `serde_with`: the reason is
    /// slightly verbose to explain (see `find_delegated_serde_deser_ty` for the details), but
    /// unless `serde_with` is being used, specifically the `#[serde_as(as = "...")]` helper
    /// attribute, then this will generally return `None`.
    ///
    /// If `#[serde_as(as = "...")]` _is_ being used, then `Some` is returned containing a reference
    /// to the delegated (de)serialization type. Again, see `find_delegated_serde_deser_ty` for more
    /// details about exactly what we look for to figure out if delegation is occurring, and the
    /// caveats around our approach.
    pub fn delegated_ty(&self) -> Option<&syn::Type> {
        self.attrs.delegated_ty.as_ref()
    }

    /// Name of the field when deserializing.
    ///
    /// This may be different than the name of the field itself depending on whether it has been
    /// altered with `serde` helper attributes i.e. `#[serde(rename = "...")]`.
    ///
    /// Additionally, for unnamed fields (tuple structs/variants), this will be the integer index of
    /// the field, formatted as a string.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Title of the field, if any.
    ///
    /// The title specifically refers to the headline portion of a doc comment. For example, if a
    /// field has the following doc comment:
    ///
    /// ```text
    /// /// My special field.
    /// ///
    /// /// Here's why it's special:
    /// /// ...
    /// field: bool,
    /// ```
    ///
    /// then the title would be `My special field`. If the doc comment only contained `My special
    /// field.`, then we would consider the title _empty_. See `description` for more details on
    /// detecting titles vs descriptions.
    pub fn title(&self) -> Option<&String> {
        self.attrs.title.as_ref()
    }

    /// Description of the field, if any.
    ///
    /// The description specifically refers to the body portion of a doc comment, or the headline if
    /// only a headline exists.. For example, if a field has the following doc comment:
    ///
    /// ```text
    /// /// My special field.
    /// ///
    /// /// Here's why it's special:
    /// /// ...
    /// field: bool,
    /// ```
    ///
    /// then the title would be everything that comes after `My special field`. If the doc comment
    /// only contained `My special field.`, then the description would be `My special field.`, and
    /// the title would be empty. In this way, the description will always be some port of a doc
    /// comment, depending on the formatting applied.
    ///
    /// This logic was chosen to mimic how Rust's own `rustdoc` tool works, where it will use the
    /// "title" portion as a high-level description for an item, only showing the title and
    /// description together when drilling down to the documentation for that specific item. JSON
    /// Schema supports both title and description for a schema, and so we expose both.
    pub fn description(&self) -> Option<&String> {
        self.attrs.description.as_ref()
    }

    /// Path to a function to call to generate a default value for the field, if any.
    ///
    /// This will boil down to something like `std::default::Default::default` or
    /// `name_of_in_scope_method_to_call`, where we generate code to actually call that path as a
    /// function to generate the default value we include in the schema for this field.
    pub fn default_value(&self) -> Option<ExprPath> {
        self.default_value.clone()
    }

    /// Whether or not the field is transparent.
    ///
    /// In some cases, namely scenarios involving newtype structs or enum tuple variants, it may be
    /// counter-intuitive to specify a title/description for a field. For example, having a newtype
    /// struct for defining systemd file descriptors requires a single internal integer field. The
    /// title/description of the newtype struct itself are sufficient from a documentation
    /// standpoint, but the procedural macro doesn't know that, and wants to enforce that we give
    /// the "field" -- the unnamed single integer field -- a title/description to ensure our
    /// resulting schema is fully specified.
    ///
    /// Applying the `#[configurable(transparent)]` helper attribute to a field will disable the
    /// title/description enforcement logic, allowing these types of newtype structs, or enum tuple
    /// variants, to simply document themselves at the container/variant level and avoid needing to
    /// document that inner field which itself needs no further title/description.
    pub fn transparent(&self) -> bool {
        self.attrs.transparent.is_present()
    }

    /// Whether or not the field is deprecated.
    ///
    /// Applying the `#[configurable(deprecated)]` helper attribute will mark this field as
    /// deprecated from the perspective of the resulting schema. It does not interact with Rust's
    /// standard `#[deprecated]` attribute, neither automatically applying it nor deriving the
    /// deprecation status of a field when it is present.
    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_some()
    }

    /// The deprecated message, if one has been set.
    pub fn deprecated_message(&self) -> Option<&String> {
        self.attrs
            .deprecated
            .as_ref()
            .and_then(|message| match message {
                Override::Inherit => None,
                Override::Explicit(message) => Some(message),
            })
    }

    /// Validation rules specific to the field, if any.
    ///
    /// Validation rules are applied to the resulting schema for this field on top of any default
    /// validation rules defined on the field type/delegated field type itself.
    pub fn validation(&self) -> &[Validation] {
        &self.attrs.validation
    }

    /// Whether or not this field is visible during either serialization or deserialization.
    ///
    /// This is derived from whether any of the `serde` visibility attributes are applied: `skip`,
    /// `skip_serializing, and `skip_deserializing`. Unless the field is skipped entirely, it will
    /// be considered visible and part of the schema.
    pub fn visible(&self) -> bool {
        self.attrs.visible
    }

    /// Whether or not to flatten the schema of this field into its container.
    ///
    /// This is derived from whether the `#[serde(flatten)]` helper attribute is present. When
    /// enabled, this will cause the field's schema to be flatten into the container's schema,
    /// mirroring how `serde` will lift the fields of the flattened field's type into the container
    /// type when (de)serializing.
    pub fn flatten(&self) -> bool {
        self.attrs.flatten
    }

    /// Metadata (custom attributes) for the field, if any.
    ///
    /// Attributes can take the shape of flags (`#[configurable(metadata(im_a_teapot))]`) or
    /// key/value pairs (`#[configurable(metadata(status = "beta"))]`) to allow rich, semantic
    /// metadata to be attached directly to fields.
    pub fn metadata(&self) -> impl Iterator<Item = LazyCustomAttribute> {
        self.attrs
            .metadata
            .clone()
            .into_iter()
            .flat_map(|metadata| metadata.attributes())
    }
}

impl<'a> ToTokens for Field<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.original.to_tokens(tokens)
    }
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(configurable))]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    derived: SpannedValue<Flag>,
    transparent: SpannedValue<Flag>,
    deprecated: Option<Override<String>>,
    #[darling(skip)]
    visible: bool,
    #[darling(skip)]
    flatten: bool,
    #[darling(multiple)]
    metadata: Vec<Metadata>,
    #[darling(multiple)]
    validation: Vec<Validation>,
    #[darling(skip)]
    delegated_ty: Option<syn::Type>,
}

impl Attributes {
    fn finalize(
        mut self,
        field: &serde_ast::Field<'_>,
        forwarded_attrs: &[syn::Attribute],
        is_virtual_newtype: bool,
        is_newtype_wrapper_field: bool,
    ) -> darling::Result<Self> {
        // Derive any of the necessary fields from the `serde` side of things.
        self.visible = !field.attrs.skip_deserializing() || !field.attrs.skip_serializing();
        self.flatten = field.attrs.flatten();

        // We additionally attempt to extract a title/description from the forwarded doc attributes, if they exist.
        // Whether we extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(forwarded_attrs);
        self.title = self.title.or(doc_title);
        self.description = self.description.or(doc_description);

        // If the field is part of a newtype wrapper -- it has the be the only field, and unnamed,
        // like `struct Foo(usize)` -- then we simply mark it as transparent.
        //
        // We do this because the container -- struct or enum variant -- will itself be required to
        // have a description. We never show the description of unnamed fields, anyways, as we defer
        // to using the description of the container. Simply marking this field as transparent will
        // keep the schema generation happy and avoid having to constantly specify `derived` or
        // `transparent` all over the place.
        if is_newtype_wrapper_field {
            // We additionally check here to see if transparent/derived as already set, as we want
            // to throw an error if they are. As we're going to forcefully mark the field as
            // transparent, there's no reason to allow setting derived/transparent manually, as it
            // only leads to boilerplate and potential confusion.
            if self.transparent.is_present() {
                return Err(err_field_implicit_transparent(&self.transparent.span()));
            }

            if self.derived.is_present() {
                return Err(err_field_implicit_transparent(&self.derived.span()));
            }

            self.transparent = SpannedValue::new(Flag::present(), Span::call_site());
        }

        // If no description was provided for the field, it is typically an error. There are few situations when this is
        // fine/valid, though:
        //
        // - the field is derived (`#[configurable(derived)]`)
        // - the field is transparent (`#[configurable(transparent)]`)
        // - the field is not visible (`#[serde(skip)]`, or `skip_serializing` plus `skip_deserializing`)
        // - the field is flattened (`#[serde(flatten)]`)
        // - the field is part of a virtual newtype
        // - the field is part of a newtype wrapper (struct/enum variant with a single unnamed field)
        //
        // If the field is derived, it means we're taking the description/title from the `Configurable` implementation of
        // the field type, which we can only do at runtime so we ignore it here. Similarly, if a field is transparent,
        // we're explicitly saying that our container is meant to essentially take on the schema of the field, rather
        // than the container being defined by the fields, if that makes sense. Derived and transparent fields are most
        // common in newtype structs and newtype variants in enums, where they're a `(T)`, and so the container acts
        // much like `T` itself.
        //
        // If the field is not visible, well, then, we're not inserting it in the schema and so requiring a description
        // or title makes no sense. Similarly, if a field is flattened, that field also won't exist in the schema as
        // we're lifting up all the fields from the type of the field itself, so again, requiring a description or title
        // makes no sense.
        //
        // If the field is part of a virtual newtype, this means the container has instructed `serde` to
        // (de)serialize it as some entirely different type. This means the original field will never show up in a
        // schema, because the schema of the thing being (de)serialized is some `T`, not `ContainerType`. Simply put,
        // like a field that is flattened or not visible, it makes no sense to require a description or title for fields
        // in a virtual newtype.
        if self.description.is_none()
            && !self.derived.is_present()
            && !self.transparent.is_present()
            && self.visible
            && !self.flatten
            && !is_virtual_newtype
        {
            return Err(err_field_missing_description(&field.original));
        }

        // Try and find the delegated (de)serialization type for this field, if it exists.
        self.delegated_ty = find_delegated_serde_deser_ty(forwarded_attrs).map(|virtual_ty| {
            // If there's a virtual type in use, we immediately transform it into our delegated
            // serialize wrapper, since we know we'll have to do that in a few different places
            // during codegen, so it's cleaner to do it here.
            let field_ty = field.ty;
            let vector_config = configurable_package_name_hack();
            parse_quote! { #vector_config::ser::Delegated<#field_ty, #virtual_ty> }
        });

        Ok(self)
    }
}
