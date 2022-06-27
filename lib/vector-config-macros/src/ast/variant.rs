use darling::{error::Accumulator, util::Flag, FromAttributes};
use serde_derive_internals::ast as serde_ast;
use syn::spanned::Spanned;

use super::{
    util::{try_extract_doc_title_description, DarlingResultIterator},
    Field, Style, Tagging,
};

pub struct Variant<'a> {
    original: &'a syn::Variant,
    name: String,
    attrs: Attributes,
    fields: Vec<Field<'a>>,
    style: Style,
    tagging: Tagging,
}

impl<'a> Variant<'a> {
    pub fn from_ast(
        serde: &serde_ast::Variant<'a>,
        tagging: Tagging,
    ) -> darling::Result<Variant<'a>> {
        let original = serde.original;
        let name = serde.attrs.name().deserialize_name();
        let style = serde.style.into();

        let attrs = Attributes::from_attributes(&original.attrs)
            .and_then(|attrs| attrs.finalize(serde, &original.attrs))?;

        let mut accumulator = Accumulator::default();
        let fields = serde
            .fields
            .iter()
            .map(Field::from_ast)
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

    pub fn style(&self) -> Style {
        self.style
    }

    pub fn tagging(&self) -> &Tagging {
        &self.tagging
    }

    pub fn fields(&self) -> &[Field<'_>] {
        &self.fields
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

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_some()
    }

    pub fn visible(&self) -> bool {
        self.attrs.visible
    }
}

impl<'a> Spanned for Variant<'a> {
    fn span(&self) -> proc_macro2::Span {
        self.original.span()
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
