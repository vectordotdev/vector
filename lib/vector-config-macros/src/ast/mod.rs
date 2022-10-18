use darling::{error::Accumulator, util::path_to_string, FromMeta};
use serde_derive_internals::{ast as serde_ast, attr as serde_attr};

mod container;
mod field;
pub(self) mod util;
mod variant;

pub use container::Container;
pub use field::Field;
use syn::NestedMeta;
pub use variant::Variant;
use vector_config_common::attributes::CustomAttribute;

/// The style of a data container, applying to both enum variants and structs.
///
/// This mirrors the type by the same name in `serde_derive_internal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Named fields.
    Struct,

    /// Multiple unnamed fields.
    Tuple,

    /// Single unnamed field.
    Newtype,

    /// No fields.
    Unit,
}

impl From<serde_ast::Style> for Style {
    fn from(style: serde_ast::Style) -> Self {
        match style {
            serde_ast::Style::Struct => Style::Struct,
            serde_ast::Style::Tuple => Style::Tuple,
            serde_ast::Style::Newtype => Style::Newtype,
            serde_ast::Style::Unit => Style::Unit,
        }
    }
}

/// The tagging configuration for an enum.
///
/// This mirrors the type by the nearly-same name (`TagType`) in `serde_derive_internal`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tagging {
    /// The default.
    ///
    /// ```json
    /// {"variant1": {"key1": "value1", "key2": "value2"}}
    /// ```
    External,

    /// `#[serde(tag = "type")]`
    ///
    /// ```json
    /// {"type": "variant1", "key1": "value1", "key2": "value2"}
    /// ```
    Internal { tag: String },

    /// `#[serde(tag = "t", content = "c")]`
    ///
    /// ```json
    /// {"t": "variant1", "c": {"key1": "value1", "key2": "value2"}}
    /// ```
    Adjacent { tag: String, content: String },

    /// `#[serde(untagged)]`
    ///
    /// ```json
    /// {"key1": "value1", "key2": "value2"}
    /// ```
    None,
}

impl Tagging {
    /// Generates custom attributes that describe the tagging mode.
    ///
    /// This is typically added to the metadata for an enum's overall schema to better describe how
    /// the various subschemas relate to each other and how they're used on the Rust side, for the
    /// purpose of generating usable documentation from the schema.
    pub fn as_enum_metadata(&self) -> Vec<CustomAttribute> {
        match self {
            Self::External => vec![CustomAttribute::kv("enum_tagging", "external")],
            Self::Internal { tag } => vec![
                CustomAttribute::kv("enum_tagging", "internal"),
                CustomAttribute::kv("enum_tag_field", tag),
            ],
            Self::Adjacent { tag, content } => vec![
                CustomAttribute::kv("enum_tagging", "adjacent"),
                CustomAttribute::kv("enum_tag_field", tag),
                CustomAttribute::kv("enum_content_field", content),
            ],
            Self::None => vec![CustomAttribute::kv("enum_tagging", "untagged")],
        }
    }
}

impl From<&serde_attr::TagType> for Tagging {
    fn from(tag: &serde_attr::TagType) -> Self {
        match tag {
            serde_attr::TagType::External => Tagging::External,
            serde_attr::TagType::Internal { tag } => Tagging::Internal { tag: tag.clone() },
            serde_attr::TagType::Adjacent { tag, content } => Tagging::Adjacent {
                tag: tag.clone(),
                content: content.clone(),
            },
            serde_attr::TagType::None => Tagging::None,
        }
    }
}

/// The fields of the data container.
///
/// This mirrors the type by the same name in `serde_derive_internal`.
pub enum Data<'a> {
    Enum(Vec<Variant<'a>>),
    Struct(Style, Vec<Field<'a>>),
}

/// Metadata items defined on containers, variants, or fields.
#[derive(Clone, Debug)]
pub struct Metadata {
    items: Vec<CustomAttribute>,
}

impl Metadata {
    pub fn attributes(&self) -> impl Iterator<Item = CustomAttribute> {
        self.items.clone().into_iter()
    }
}

impl FromMeta for Metadata {
    fn from_list(items: &[NestedMeta]) -> darling::Result<Self> {
        let mut errors = Accumulator::default();

        // Can't be empty.
        if items.is_empty() {
            errors.push(darling::Error::too_few_items(1));
        }

        errors = errors.checkpoint()?;

        // Can't either be name/value pairs or single items i.e. flags.
        let meta_items = items
            .iter()
            .filter_map(|nmeta| match nmeta {
                NestedMeta::Meta(meta) => match meta {
                    syn::Meta::Path(path) => match path.get_ident() {
                        Some(ident) => Some(CustomAttribute::Flag(ident.to_string())),
                        None => {
                            errors.push(darling::Error::unknown_value("flag attributes must be simple strings i.e. `flag` or `my_flag`").with_span(nmeta));
                            None
                        },
                    }
                    syn::Meta::List(_) => {
                        errors.push(darling::Error::unexpected_type("list").with_span(nmeta));
                        None
                    }
                    syn::Meta::NameValue(nv) => match &nv.lit {
                        syn::Lit::Str(s) => Some(CustomAttribute::KeyValue {
                            key: path_to_string(&nv.path),
                            value: s.value(),
                        }),
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
            .collect::<Vec<_>>();

        errors.finish_with(Metadata { items: meta_items })
    }
}
