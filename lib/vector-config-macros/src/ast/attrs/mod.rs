use serde_derive_internals::Ctxt;
use syn::{Attribute, Lit, LitStr, Meta, MetaList, MetaNameValue, NestedMeta, Path, PathArguments};

mod container;
mod field;
mod validation;
mod variant;

pub(super) use container::Container;
pub(super) use field::FieldAttributes;
pub(super) use variant::VariantAttributes;

fn try_get_attribute_meta_list(
    attribute: &Attribute,
    context: &Ctxt,
) -> Result<Vec<NestedMeta>, ()> {
    // We only care about attributes matching the given type i.e. `configurable` or `serde`.
    if !attribute.path.is_ident("configurable") {
        return Ok(Vec::new());
    }

    // We always expect to see attributes in the form of `#[foo(...)]` which are considered a
    // "list".  All others -- `#[foo]`, `#[foo = ...]` -- are invalid for our purposes.
    match attribute.parse_meta() {
        Ok(Meta::List(meta)) => Ok(meta.nested.into_iter().collect()),
        Ok(other) => {
            context.error_spanned_by(other, "expected #[configurable(...)]");
            Err(())
        }
        Err(err) => {
            context.error_spanned_by(attribute, err);
            Err(())
        }
    }
}

fn get_lit_str<'a>(
    context: &Ctxt,
    meta_item_name: &'static str,
    lit: &'a Lit,
) -> Result<&'a LitStr, ()> {
    if let Lit::Str(lit) = lit {
        Ok(lit)
    } else {
        context.error_spanned_by(
            lit,
            format!(
                "expected configurable `{}` attribute to be a string: `{} = \"...\"`",
                meta_item_name, meta_item_name
            ),
        );
        Err(())
    }
}

fn get_back_to_back_lit_strs<'a>(
    context: &Ctxt,
    meta_item_name: &'static str,
    meta_list: &'a MetaList,
) -> Result<(&'a LitStr, &'a LitStr), ()> {
    let mut lit_strs = meta_list
        .nested
        .iter()
        .filter_map(|nm| match nm {
            NestedMeta::Lit(Lit::Str(lit)) => Some(lit),
            _ => None,
        })
        .collect::<Vec<_>>();

    if lit_strs.len() == 2 {
        Ok((lit_strs.remove(0), lit_strs.remove(0)))
    } else {
        context.error_spanned_by(
            meta_list,
            format!(
                "expected configurable `{}` attribute to be back-to-back literals: `{}(\"...\", \"...\")`",
                meta_item_name, meta_item_name
            ),
        );
        Err(())
    }
}

fn try_extract_doc_title_description(attributes: &[Attribute]) -> (Option<String>, Option<String>) {
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

fn duplicate_attribute(context: &Ctxt, item: &MetaNameValue) {
    let msg = format!(
        "duplicate configurable attribute `{}`",
        item.path.get_ident().unwrap()
    );
    context.error_spanned_by(item, msg)
}

fn path_to_string(path: &Path) -> String {
    // If the path already has an ident available, just use that.
    if let Some(ident) = path.get_ident() {
        return ident.to_string();
    }

    // We didn't have a valid ident, so we need tgo reconstruct the path from the path segments and
    // generate something that's decently human-readable.
    path.segments
        .iter()
        .map(|segment| match segment.arguments {
            PathArguments::None => segment.ident.to_string(),
            // We don't bother fully expanding the arguments in angle brackets/parentheses, because
            // there's no fallthrough `Display` impl for all possible enums/variants/fields/etc and
            // this function should be used very infrequently, so its output should be more than
            // adequate in those cases as-is.
            PathArguments::AngleBracketed(_) => format!("{}<...>", segment.ident),
            PathArguments::Parenthesized(_) => format!("{}(...)", segment.ident),
        })
        .reduce(|mut s, seg| {
            s.push_str("::");
            s.push_str(seg.as_str());
            s
        })
        .expect("paths should never be empty")
}
