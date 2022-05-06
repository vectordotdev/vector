use serde_derive_internals::{ast as serde_ast, attr as serde_attr};

mod container;
mod field;
pub(self) mod util;
mod variant;

pub use container::Container;
pub use field::Field;
pub use variant::Variant;

/// The style of a data container, applying to both enum variants and structs.
///
/// This mirrors the type by the same name in `serde_derive_internal`.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
