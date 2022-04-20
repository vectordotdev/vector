use serde_derive_internals::ast as serde_ast;

use crate::attrs;

pub struct Container<'a> {
	serde: serde_ast::Container<'a>,
	attrs: attrs::Container,
	original: &'a syn::DeriveInput,
}