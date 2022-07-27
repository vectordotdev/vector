use darling::{util::Flag, FromAttributes};
use serde_derive_internals::ast as serde_ast;
use syn::{spanned::Spanned, ExprPath, Ident};
use vector_config_common::{attributes::CustomAttribute, validation::Validation};

use super::{
    util::{
        err_field_missing_description, get_serde_default_value, try_extract_doc_title_description,
    },
    Metadata,
};

pub struct Field<'a> {
    original: &'a syn::Field,
    name: String,
    default_value: Option<ExprPath>,
    attrs: Attributes,
}

impl<'a> Field<'a> {
    pub fn from_ast(
        serde: &serde_ast::Field<'a>,
        is_virtual_newtype: bool,
    ) -> darling::Result<Field<'a>> {
        let original = serde.original;

        let name = serde.attrs.name().deserialize_name();
        let default_value = get_serde_default_value(serde.attrs.default());

        Attributes::from_attributes(&original.attrs)
            .and_then(|attrs| attrs.finalize(serde, &original.attrs, is_virtual_newtype))
            .map(|attrs| Field {
                original,
                name,
                default_value,
                attrs,
            })
    }

    pub fn ident(&self) -> Option<&Ident> {
        self.original.ident.as_ref()
    }

    pub fn ty(&self) -> &syn::Type {
        &self.original.ty
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

    pub fn transparent(&self) -> bool {
        self.attrs.transparent.is_some()
    }

    pub fn deprecated(&self) -> bool {
        self.attrs.deprecated.is_some()
    }

    pub fn validation(&self) -> &[Validation] {
        &self.attrs.validation
    }

    pub fn visible(&self) -> bool {
        self.attrs.visible
    }

    pub fn flatten(&self) -> bool {
        self.attrs.flatten
    }

    pub fn metadata(&self) -> impl Iterator<Item = CustomAttribute> {
        self.attrs
            .metadata
            .clone()
            .into_iter()
            .flat_map(|metadata| metadata.attributes())
    }
}

impl<'a> Spanned for Field<'a> {
    fn span(&self) -> proc_macro2::Span {
        match self.original.ident.as_ref() {
            Some(ident) => ident.span(),
            None => self.original.ty.span(),
        }
    }
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(configurable))]
struct Attributes {
    title: Option<String>,
    description: Option<String>,
    derived: Flag,
    transparent: Flag,
    deprecated: Flag,
    #[darling(skip)]
    visible: bool,
    #[darling(skip)]
    flatten: bool,
    #[darling(multiple)]
    metadata: Vec<Metadata>,
    #[darling(multiple)]
    validation: Vec<Validation>,
}

impl Attributes {
    fn finalize(
        mut self,
        field: &serde_ast::Field<'_>,
        forwarded_attrs: &[syn::Attribute],
        is_virtual_newtype: bool,
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

        // If no description was provided for the field, it is typically an error. There are few situations when this is
        // fine/valid, though:
        //
        // - the field is derived (`#[configurable(derived)]`)
        // - the field is transparent (`#[configurable(transparent)]`)
        // - the field is not visible (`#[serde(skip)]`)
        // - the field is flattened (`#[serde(flatten)]`)
        // - the field is part of a virtual newtype
        //
        // If a field is derived, it means we're taking the description/title from the `Configurable` implementation of
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
        // Finally, if a field is part of a virtual newtype, this means the container has instructed `serde` to
        // (de)serialize it as some entirely different type. This means the original field will never show up in a
        // schema, because the schema of the thing being (de)esrialized is some `T`, not `ContainerType`. Simply put,
        // like a field that is flattened or not visible, it makes no sense to require a description or title for fields
        // in a virtual newtype.
        if self.description.is_none()
            && !self.derived.is_some()
            && !self.transparent.is_some()
            && self.visible
            && !self.flatten
            && !is_virtual_newtype
        {
            return Err(err_field_missing_description(&field.original));
        }

        Ok(self)
    }
}
