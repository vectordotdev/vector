use darling::error::Accumulator;
use serde_derive_internals::{attr as serde_attr, Ctxt};
use syn::{spanned::Spanned, Attribute, ExprPath, Lit, Meta, MetaNameValue, NestedMeta};

const ERR_FIELD_MISSING_DESCRIPTION: &str = "field must have a description -- i.e. `/// This is a widget...` or `#[configurable(description = \"...\")] -- or derive it from the underlying type of the field by specifying `#[configurable(derived)]`";

pub fn try_extract_doc_title_description(
    attributes: &[Attribute],
) -> (Option<String>, Option<String>) {
    // Scrape all the attributes that have the `doc` path, which will be used for holding doc
    // comments that we're interested in utilizing, and extract their value.
    let doc_comments = attributes
        .iter()
        // We only care about `doc` attributes.
        .filter(|attribute| attribute.path.is_ident("doc"))
        // Extract the value of the attribute if it's in the form of `doc = "..."`.
        .filter_map(|attribute| match attribute.parse_meta() {
            Ok(Meta::NameValue(MetaNameValue {
                lit: Lit::Str(s), ..
            })) => Some(s.value()),
            _ => None,
        })
        // Trim any whitespace that is present at the beginning/end.
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>();

    // If there were no doc comments, then we have no title/description to try and extract.
    if doc_comments.is_empty() {
        return (None, None);
    }

    // We emulate what `cargo doc` does, which is that if you have a doc comment with a bunch of
    // text, then an empty line, and then more text, it considers the first chunk the title, and
    // the second chunk the description.
    //
    // If there's no empty line, then we just consider it all the description.
    let title_desc_break_index =
        doc_comments
            .iter()
            .enumerate()
            .find_map(|(index, l)| if l.trim() == "" { Some(index) } else { None });

    if let Some(index) = title_desc_break_index {
        let title_parts = doc_comments
            .iter()
            .take(index)
            .map(|s| s.as_str())
            .collect::<Vec<_>>();
        let title = title_parts.join(" ");

        let desc_parts = doc_comments
            .iter()
            .skip(index + 1)
            .map(|s| s.as_str())
            .collect::<Vec<_>>();
        let desc = desc_parts.join(" ");

        (none_if_empty(title), none_if_empty(desc))
    } else {
        let desc_parts = doc_comments.iter().map(|s| s.as_str()).collect::<Vec<_>>();
        let desc = desc_parts.join(" ");

        (None, none_if_empty(desc))
    }
}

fn none_if_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn get_default_exprpath() -> ExprPath {
    syn::parse_str("::std::default::Default::default")
        .expect("expression path for default should never be invalid")
}

pub fn err_field_missing_description<T: Spanned>(field: &T) -> darling::Error {
    darling::Error::custom(ERR_FIELD_MISSING_DESCRIPTION).with_span(field)
}

pub fn get_serde_default_value(default: &serde_attr::Default) -> Option<ExprPath> {
    match default {
        serde_attr::Default::None => None,
        serde_attr::Default::Default => Some(get_default_exprpath()),
        serde_attr::Default::Path(path) => Some(path.clone()),
    }
}

pub fn err_serde_failed(context: Ctxt) -> darling::Error {
    context
        .check()
        .map_err(|errs| darling::Error::multiple(errs.into_iter().map(Into::into).collect()))
        .expect_err("serde error context should not be empty")
}

pub trait DarlingResultIterator<I> {
    fn collect_darling_results(self, accumulator: &mut Accumulator) -> Vec<I>;
}

impl<I, T> DarlingResultIterator<I> for T
where
    T: Iterator<Item = Result<I, darling::Error>>,
{
    fn collect_darling_results(self, accumulator: &mut Accumulator) -> Vec<I> {
        self.filter_map(|result| accumulator.handle(result))
            .collect()
    }
}

/// Checks if the path matches `other`.
///
/// If a valid ident can be constructed from the path, and the ident's value matches `other`,
/// `true` is returned. Otherwise, `false` is returned.
fn path_matches<S: AsRef<str>>(path: &syn::Path, other: S) -> bool {
    path.get_ident().filter(|i| *i == &other).is_some()
}

/// Tries to find a specific attribute with a specific name/value pair.
///
/// Only works with derive macro helper attributes, and not raw name/value attributes such as
/// `#[path = "..."]`.
///
/// If an attribute with a path matching `attr_name`, and a meta name/value pair with a name
/// matching `name_key` is found, `Some(path)` is returned, representing the value of the name/value pair.
///
/// If no attribute matches, or if the given name/value pair is not found, `None` is returned.
fn find_name_value_attribute(
    attributes: &[syn::Attribute],
    attr_name: &str,
    name_key: &str,
) -> Option<Lit> {
    attributes
        .iter()
        // Only take attributes whose name matches `attr_name`.
        .filter(|attr| path_matches(&attr.path, attr_name))
        // Make sure the contents actually parse as a normal structured attribute.
        .filter_map(|attr| attr.parse_meta().ok())
        // Derive macro helper attributes will always be in the list form.
        .filter_map(|meta| match meta {
            Meta::List(ml) => Some(ml.nested.into_iter()),
            _ => None,
        })
        .flatten()
        // For each nested meta item in the list, find any that are name/value pairs where the
        // name matches `name_key`, and return their value.
        .find_map(|nm| match nm {
            NestedMeta::Meta(meta) => match meta {
                Meta::NameValue(nv) if path_matches(&nv.path, name_key) => Some(nv.lit),
                _ => None,
            },
            _ => None,
        })
}

/// Tries to find a delegated (de)serialization type from attributes.
///
/// In some cases, the `serde_with` crate, more specifically the `serde_as` attribute macro, may be
/// used to help (de)serialize a field/container with type A via a (de)implementation on type B, in order to
/// provide more ergonomic (de)serialization of values that can represent type A without needing to
/// explicitly match type A when (de)serialized. This is similar to `serde`'s existing support for
/// "remote" types but is taken further with a more generic and extensible approach.
///
/// This, however, presents an issue because while normally we can handle scenarios like
/// `#[serde(from = "...")]` and its siblings, `serde_as` depends on `#[serde(with = "...")]` and
/// the fact that it simply constructs a path to the (de)serialize methods, rather than always
/// needing to explicitly reference a type. This means that we cannot simply grab the value of the
/// `with` name/value pair blindly, and assume if there's a value that a delegated/remote type is in
/// play... it could be a module path, too.
///
/// This method looks for two indicators to understand when it should be able to extract the
/// delegated type:
///
/// - `#[serde(with = "...")]` is present
/// - `#[serde_as(as = "...")]` is present
///
/// When both of these are true, we can rely on the fact that the value of `with` will be a valid
/// type path, and usable like a virtual newtype, which is where we use the type specified for
/// `try_from`/`from`/`into` for the delegated (de)serialization type of a container itself.
///
/// If we find both of those attribute name/value pairs, and the value of `with` can be parsed
/// successfully as a type path, `Some(...)` is returned, contained the type. Otherwise, `None` is
/// returned.
pub fn find_delegated_serde_deser_ty(attributes: &[syn::Attribute]) -> Option<syn::Type> {
    // Make sure `#[serde_as(as = "...")]` is present.
    find_name_value_attribute(attributes, "serde_as", "as")
        // Make sure `#[serde(with = "...")]` is present, and grab its value.
        .and_then(|_| find_name_value_attribute(attributes, "serde", "with"))
        // Try and parse the value as a type path.
        .and_then(|with| match with {
            Lit::Str(s) => s.parse::<syn::Type>().ok(),
            _ => None,
        })
}
