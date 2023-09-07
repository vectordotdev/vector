use darling::{error::Accumulator, util::Flag, FromAttributes};
use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use serde_derive_internals::ast as serde_ast;

use super::{
    util::{try_extract_doc_title_description, DarlingResultIterator},
    Field, LazyCustomAttribute, Metadata, Style, Tagging,
};

/// A variant in an enum.
pub struct Variant<'a> {
    original: &'a syn::Variant,
    name: String,
    attrs: Attributes,
    fields: Vec<Field<'a>>,
    style: Style,
    tagging: Tagging,
}

impl<'a> Variant<'a> {
    /// Creates a new `Variant<'a>` from the `serde`-derived information about the given variant.
    pub fn from_ast(
        serde: &serde_ast::Variant<'a>,
        tagging: Tagging,
        is_virtual_newtype: bool,
    ) -> darling::Result<Variant<'a>> {
        let original = serde.original;
        let name = serde.attrs.name().deserialize_name().to_string();
        let style = serde.style.into();
        let is_newtype_wrapper_field = style == Style::Newtype;

        let attrs = Attributes::from_attributes(&original.attrs)
            .and_then(|attrs| attrs.finalize(serde, &original.attrs))?;

        let mut accumulator = Accumulator::default();
        let fields = serde
            .fields
            .iter()
            .map(|field| Field::from_ast(field, is_virtual_newtype, is_newtype_wrapper_field))
            .collect_darling_results(&mut accumulator);

        let variant = Variant {
            original,
            name,
            attrs,
            fields,
            style,
            tagging,
        };
        accumulator.finish_with(variant)
    }

    /// Ident of the variant.
    pub fn ident(&self) -> &Ident {
        &self.original.ident
    }

    /// Style of the variant.
    ///
    /// This comes directly from `serde`, but effectively represents common terminology used outside
    /// of `serde` when describing the shape of a data container, such as if a struct is a "tuple
    /// struct" or a "newtype wrapper", and so on.
    pub fn style(&self) -> Style {
        self.style
    }

    /// Tagging configuration of the variant.
    ///
    /// This comes directly from `serde`. For more information on tagging, see [Enum representations][serde_tagging_docs].
    ///
    /// [serde_tagging_docs]: https://serde.rs/enum-representations.html
    pub fn tagging(&self) -> &Tagging {
        &self.tagging
    }

    /// Fields of the variant, if any.
    pub fn fields(&self) -> &[Field<'_>] {
        &self.fields
    }

    /// Name of the variant when deserializing.
    ///
    /// This may be different than the name of the variant itself depending on whether it has been
    /// altered with `serde` helper attributes i.e. `#[serde(rename = "...")]`.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Title of the variant, if any.
    ///
    /// The title specifically refers to the headline portion of a doc comment. For example, if a
    /// variant has the following doc comment:
    ///
    /// ```text
    /// /// My special variant.
    /// ///
    /// /// Here's why it's special:
    /// /// ...
    /// SomeVariant(...),
    /// ```
    ///
    /// then the title would be `My special variant`. If the doc comment only contained `My special
    /// variant.`, then we would consider the title _empty_. See `description` for more details on
    /// detecting titles vs descriptions.
    pub fn title(&self) -> Option<&String> {
        self.attrs.title.as_ref()
    }

    /// Description of the variant, if any.
    ///
    /// The description specifically refers to the body portion of a doc comment, or the headline if
    /// only a headline exists.. For example, if a variant has the following doc comment:
    ///
    /// ```text
    /// /// My special variant.
    /// ///
    /// /// Here's why it's special:
    /// /// ...
    /// SomeVariant(...),
    /// ```
    ///
    /// then the title would be everything that comes after `My special variant`. If the doc comment
    /// only contained `My special variant.`, then the description would be `My special variant.`,
    /// and the title would be empty.  In this way, the description will always be some port of a
    /// doc comment, depending on the formatting applied.
    ///
    /// This logic was chosen to mimic how Rust's own `rustdoc` tool works, where it will use the
    /// "title" portion as a high-level description for an item, only showing the title and
    /// description together when drilling down to the documentation for that specific item. JSON
    /// Schema supports both title and description for a schema, and so we expose both.
    pub fn description(&self) -> Option<&String> {
        self.attrs.description.as_ref()
    }

    /// Whether or not the variant is deprecated.
    ///
    /// Applying the `#[configurable(deprecated)]` helper attribute will mark this variant as
    /// deprecated from the perspective of the resulting schema. It does not interact with Rust's
    /// standard `#[deprecated]` attribute, neither automatically applying it nor deriving the
    /// deprecation status of a variant when it is present.
    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_present()
    }

    /// Whether or not this variant is visible during either serialization or deserialization.
    ///
    /// This is derived from whether any of the `serde` visibility attributes are applied: `skip`,
    /// `skip_serializing`, and `skip_deserializing`. Unless the variant is skipped entirely, it will
    /// be considered visible and part of the schema.
    pub fn visible(&self) -> bool {
        self.attrs.visible
    }

    /// Metadata (custom attributes) for the variant, if any.
    ///
    /// Attributes can take the shape of flags (`#[configurable(metadata(im_a_teapot))]`) or
    /// key/value pairs (`#[configurable(metadata(status = "beta"))]`) to allow rich, semantic
    /// metadata to be attached directly to variants.
    pub fn metadata(&self) -> impl Iterator<Item = LazyCustomAttribute> {
        self.attrs
            .metadata
            .clone()
            .into_iter()
            .flat_map(|metadata| metadata.attributes())
    }
}

impl<'a> ToTokens for Variant<'a> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.original.to_tokens(tokens)
    }
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(configurable))]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    deprecated: Flag,
    #[darling(skip)]
    visible: bool,
    #[darling(multiple)]
    metadata: Vec<Metadata>,
}

impl Attributes {
    fn finalize(
        mut self,
        variant: &serde_ast::Variant<'_>,
        forwarded_attrs: &[syn::Attribute],
    ) -> darling::Result<Self> {
        // Derive any of the necessary fields from the `serde` side of things.
        self.visible = !variant.attrs.skip_deserializing() || !variant.attrs.skip_serializing();

        // We additionally attempt to extract a title/description from the forwarded doc attributes, if they exist.
        // Whether we extract both a title and description, or just description, is documented in more detail in
        // `try_extract_doc_title_description` itself.
        let (doc_title, doc_description) = try_extract_doc_title_description(forwarded_attrs);
        self.title = self.title.or(doc_title);
        self.description = self.description.or(doc_description);

        Ok(self)
    }
}

impl<'a> PartialEq for Variant<'a> {
    fn eq(&self, other: &Self) -> bool {
        // Equality checking between variants is only used to drive conformance checks around making
        // sure no duplicate variants exist when in untagged mode, so all we care about is what
        // distinguishes a variant when it's in its serialized form, which is the shape -- struct vs
        // tuple vs unit -- and the fields therein.

        // It's suboptimal to be allocating strings for the field names here but we need the
        // deserialized name as `serde` observes it, and this only runs at compile-time.
        let self_fields = self
            .fields
            .iter()
            .map(|field| (field.name(), field.ty()))
            .collect::<Vec<_>>();
        let other_fields = other
            .fields
            .iter()
            .map(|field| (field.name(), field.ty()))
            .collect::<Vec<_>>();

        self.style() == other.style()
            && self.tagging == other.tagging
            && self_fields == other_fields
    }
}
