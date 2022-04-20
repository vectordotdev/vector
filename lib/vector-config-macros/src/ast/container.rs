use serde_derive_internals::{ast as serde_ast, Ctxt, Derive};
use syn::{DeriveInput, Ident, Generics};

use crate::attrs;

pub struct Container<'a> {
	serde: serde_ast::Container<'a>,
	attrs: attrs::Container,
	original: &'a syn::DeriveInput,
}

impl<'a> Container<'a> {
	pub fn from_derive_input(context: &Ctxt, input: &'a DeriveInput) -> Option<Container<'a>> {
		// We can't do anything unless `serde` can also handle this container. We specifically only
		// care about deserialization here, because the schema tells us what we can _give_ to Vector.
		serde_ast::Container::from_ast(context, input, Derive::Deserialize)
			// Once we have the `serde` side of things, we need to collect our own specific
			// attributes for the container and map things to our own `Container`.
			.map(|serde| {
				let attrs = attrs::Container::from_ast(context, input);

				Container { serde, attrs, original: input }
			})
	}

	pub fn original(&self) -> &DeriveInput {
		self.original
	}

	pub fn ident(&self) -> Ident {
		self.serde.ident
	}

	pub fn generics(&self) -> &Generics {
		self.serde.generics
	}

	pub fn referencable_name(&self) -> String {
		self.serde.attrs.name().deserialize_name()
	}

	pub fn title(&self) -> Option<String> {
		self.attrs.title.clone()
	}

	pub fn description(&self) -> Option<String> {
		self.attrs.title.clone()
	}

	pub fn deprecated(&self) -> bool {
		self.attrs.deprecated
	}

	pub fn metadata(&self) -> Vec<(String, String)> {
		self.attrs.metadata.clone()
	}
}