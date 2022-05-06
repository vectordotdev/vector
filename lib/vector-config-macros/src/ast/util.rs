use darling::error::Accumulator;
use serde_derive_internals::{attr as serde_attr, Ctxt};
use syn::{Attribute, ExprPath, Lit, Meta, MetaNameValue};

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

pub fn err_field_missing_description(field: &syn::Field) -> darling::Error {
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
