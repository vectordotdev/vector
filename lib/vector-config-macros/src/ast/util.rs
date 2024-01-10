use darling::{ast::NestedMeta, error::Accumulator};
use quote::{quote, ToTokens};
use serde_derive_internals::{attr as serde_attr, Ctxt};
use syn::{
    punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Expr, ExprLit, ExprPath,
    Lit, Meta, MetaNameValue,
};

const ERR_FIELD_MISSING_DESCRIPTION: &str = "field must have a description -- i.e. `/// This is a widget...` or `#[configurable(description = \"...\")] -- or derive it from the underlying type of the field by specifying `#[configurable(derived)]`";
const ERR_FIELD_IMPLICIT_TRANSPARENT: &str =
    "field in a newtype wrapper should not be manually marked as `derived`/`transparent`";

pub fn try_extract_doc_title_description(
    attributes: &[Attribute],
) -> (Option<String>, Option<String>) {
    // Scrape all the attributes that have the `doc` path, which will be used for holding doc
    // comments that we're interested in utilizing, and extract their value.
    let doc_comments = attributes
        .iter()
        // We only care about `doc` attributes.
        .filter(|attribute| attribute.path().is_ident("doc"))
        // Extract the value of the attribute if it's in the form of `doc = "..."`.
        .filter_map(|attribute| match &attribute.meta {
            Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(s), ..
                    }),
                ..
            }) => Some(s.value()),
            _ => None,
        })
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
    //
    // The grouping logic of `group_doc_lines` lets us determine which scenario we're dealing with
    // based on the number of grouped lines.
    let mut grouped_lines = group_doc_lines(&doc_comments);
    match grouped_lines.len() {
        // No title or description.
        0 => (None, None),
        // Just a single grouped line/paragraph, so we emit that as the description.
        1 => (None, none_if_empty(grouped_lines.remove(0))),
        // Two or more grouped lines/paragraphs, so the first one is the title, and the rest are the
        // description, which we concatenate together with newlines, since the description at least
        // needs to be a single string.
        _ => {
            let title = grouped_lines.remove(0);
            let description = grouped_lines.join("\n\n");

            (none_if_empty(title), none_if_empty(description))
        }
    }
}

fn group_doc_lines(ungrouped: &[String]) -> Vec<String> {
    // When we write a doc comment in Rust, it typically ends up looking something like this:
    //
    // /// A helper for XYZ.
    // ///
    // /// This helper works in the following way, and so on and so forth.
    // ///
    // /// This separate paragraph explains a different, but related, aspect
    // /// of the helper.
    //
    // To humans, this format is natural and we see it and read it as three paragraphs. Once those
    // doc comments are processed and we get them in a procedural macro, they look like this,
    // though:
    //
    // #[doc = " A helper for XYZ."]
    // #[doc = ""]
    // #[doc = " This helper works in the following way, and so on and so forth."]
    // #[doc = ""]
    // #[doc = " This separate paragraph explains a different, but related, aspect"]
    // #[doc = " of the helper."]
    //
    // What we want to do is actually parse this as three paragraphs, with the individual lines of
    // each paragraph merged together as a single string, and extraneous whitespace removed, such
    // that we should end up with a vector of strings that looks like:
    //
    // - "A helper for XYZ."
    // - "This helper works in the following way, and so on and so forth."
    // - "This separate paragraph explains a different, but related, aspect\n of the helper."

    // TODO: Markdown link reference definitions (LFDs) -- e.g. `[foo]: https://zombohtml5.com` --
    // have to be on their own line, which is a little annoying because ideally we want to remove
    // the newlines between lines that simply get line wrapped, such that in the above example.
    // While that extra newline towards the end of the third line/paragraph is extraneous, because
    // it represents a forced line break which is imposing some measure of stylistic license, we
    // _do_ need line breaks to stay in place so that LFDs stay on their own line, otherwise it
    // seems like Markdown parsers will treat them as free-form text.
    //
    // I'm not sure if we'll want to go as far as trying to parse each line specifically as an LFD,
    // for the purpose of controlling how we add/remove linebreaks... but it's something we'll
    // likely want/need to eventually figure out.

    let mut buffer = String::new();
    let mut grouped = ungrouped.iter().fold(Vec::new(), |mut grouped, line| {
        match line.as_str() {
            // Full line breaks -- i.e. `#[doc = ""]` -- will be empty strings, which is our
            // signal to consume our buffer and emit it as a grouped line/paragraph.
            "" => {
                if !buffer.is_empty() {
                    let trimmed = buffer.trim().to_string();
                    grouped.push(trimmed);

                    buffer.clear();
                }
            }
            // The line actually has some content, so just append it to our string buffer after
            // dropping the leading space, if one exists.
            s => {
                buffer.push_str(s.strip_prefix(' ').unwrap_or(s));
                buffer.push('\n');
            }
        };

        grouped
    });

    // If we have anything left in the buffer, consume it as a grouped line/paragraph.
    if !buffer.is_empty() {
        let trimmed = buffer.trim().to_string();
        grouped.push(trimmed);
    }

    grouped
}

fn none_if_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub fn err_field_missing_description<T: Spanned>(field: &T) -> darling::Error {
    darling::Error::custom(ERR_FIELD_MISSING_DESCRIPTION).with_span(field)
}

pub fn err_field_implicit_transparent<T: Spanned>(field: &T) -> darling::Error {
    darling::Error::custom(ERR_FIELD_IMPLICIT_TRANSPARENT).with_span(field)
}

pub fn get_serde_default_value<S: ToTokens>(
    source: &S,
    default: &serde_attr::Default,
) -> Option<ExprPath> {
    match default {
        serde_attr::Default::None => None,
        serde_attr::Default::Default => {
            let qualified_path = syn::parse2(quote! {
                <#source as ::std::default::Default>::default
            })
            .expect("should not fail to parse qualified default path");
            Some(qualified_path)
        }
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
        .filter(|attr| path_matches(attr.path(), attr_name))
        // Derive macro helper attributes will always be in the list form.
        .filter_map(|attr| match &attr.meta {
            Meta::List(ml) => ml
                .parse_args_with(Punctuated::<NestedMeta, Comma>::parse_terminated)
                .map(|nested| nested.into_iter())
                .ok(),
            _ => None,
        })
        .flatten()
        // For each nested meta item in the list, find any that are name/value pairs where the
        // name matches `name_key`, and return their value.
        .find_map(|nm| match nm {
            NestedMeta::Meta(meta) => match meta {
                Meta::NameValue(nv) if path_matches(&nv.path, name_key) => match nv.value {
                    Expr::Lit(ExprLit { lit, .. }) => Some(lit),
                    _ => None,
                },
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
    find_name_value_attribute(attributes, "serde_as", "r#as")
        // Make sure `#[serde(with = "...")]` is present, and grab its value.
        .and_then(|_| find_name_value_attribute(attributes, "serde", "with"))
        // Try and parse the value as a type path.
        .and_then(|with| match with {
            Lit::Str(s) => s.parse::<syn::Type>().ok(),
            _ => None,
        })
}
