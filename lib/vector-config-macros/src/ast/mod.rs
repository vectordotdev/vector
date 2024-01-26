use darling::{ast::NestedMeta, error::Accumulator, util::path_to_string, FromMeta};
use quote::ToTokens;
use serde_derive_internals::{ast as serde_ast, attr as serde_attr};

mod container;
mod field;
mod util;
mod variant;

pub use container::Container;
pub use field::Field;
use syn::Expr;
pub use variant::Variant;
use vector_config_common::constants;

const INVALID_VALUE_EXPR: &str =
    "got function call-style literal value but could not parse as expression";

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
    pub fn as_enum_metadata(&self) -> Vec<LazyCustomAttribute> {
        match self {
            Self::External => vec![LazyCustomAttribute::kv(
                constants::DOCS_META_ENUM_TAGGING,
                "external",
            )],
            Self::Internal { tag } => vec![
                LazyCustomAttribute::kv(constants::DOCS_META_ENUM_TAGGING, "internal"),
                LazyCustomAttribute::kv(constants::DOCS_META_ENUM_TAG_FIELD, tag),
            ],
            Self::Adjacent { tag, content } => vec![
                LazyCustomAttribute::kv(constants::DOCS_META_ENUM_TAGGING, "adjacent"),
                LazyCustomAttribute::kv(constants::DOCS_META_ENUM_TAG_FIELD, tag),
                LazyCustomAttribute::kv(constants::DOCS_META_ENUM_CONTENT_FIELD, content),
            ],
            Self::None => vec![LazyCustomAttribute::kv(
                constants::DOCS_META_ENUM_TAGGING,
                "untagged",
            )],
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

/// A lazy version of `CustomAttribute`.
///
/// This is used to capture the value at the macro callsite without having to evaluate it, which
/// lets us generate code where, for example, the value of a metadata key/value pair can be
/// evaluated by an expression given in the attribute.
///
/// This is similar to how `serde` takes an expression for things like `#[serde(default =
/// "exprhere")]`, and so on.
#[derive(Clone, Debug)]
pub enum LazyCustomAttribute {
    /// A standalone flag.
    Flag(String),

    /// A key/value pair.
    KeyValue {
        key: String,
        value: proc_macro2::TokenStream,
    },
}

impl LazyCustomAttribute {
    pub fn kv<K, V>(key: K, value: V) -> Self
    where
        K: std::fmt::Display,
        V: ToTokens,
    {
        Self::KeyValue {
            key: key.to_string(),
            value: value.to_token_stream(),
        }
    }
}

/// Metadata items defined on containers, variants, or fields.
#[derive(Clone, Debug)]
pub struct Metadata {
    items: Vec<LazyCustomAttribute>,
}

impl Metadata {
    pub fn attributes(&self) -> impl Iterator<Item = LazyCustomAttribute> {
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
                    syn::Meta::Path(path) => Some(LazyCustomAttribute::Flag(path_to_string(path))),
                    syn::Meta::List(_) => {
                        errors.push(darling::Error::unexpected_type("list").with_span(nmeta));
                        None
                    }
                    syn::Meta::NameValue(nv) => match &nv.value {
                        Expr::Lit(expr) => {
                            match &expr.lit {
                                // When dealing with a string literal, we check if it ends in `()`. If so,
                                // we emit that as-is, leading to doing a function call and using the return
                                // value of that function as the value for this key/value pair.
                                //
                                // Otherwise, we just treat the string literal normally.
                                syn::Lit::Str(s) => {
                                    if s.value().ends_with("()") {
                                        if let Ok(expr) = s.parse::<Expr>() {
                                            Some(LazyCustomAttribute::KeyValue {
                                                key: path_to_string(&nv.path),
                                                value: expr.to_token_stream(),
                                            })
                                        } else {
                                            errors.push(
                                                darling::Error::custom(INVALID_VALUE_EXPR)
                                                    .with_span(nmeta),
                                            );
                                            None
                                        }
                                    } else {
                                        Some(LazyCustomAttribute::KeyValue {
                                            key: path_to_string(&nv.path),
                                            value: s.value().to_token_stream(),
                                        })
                                    }
                                }
                                lit => Some(LazyCustomAttribute::KeyValue {
                                    key: path_to_string(&nv.path),
                                    value: lit.to_token_stream(),
                                }),
                            }
                        }
                        expr => {
                            errors
                                .push(darling::Error::unexpected_expr_type(expr).with_span(nmeta));
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
