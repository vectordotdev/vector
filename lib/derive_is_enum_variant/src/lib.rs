// Copyright (c) 2021 Vector Contributors <vector@timber.io>
// Copyright (c) 2015 The Rust Project Developers

// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:

// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! ### Add `#[derive(is_enum_variant)]` to your `enum` definitions:
//!
//! ```rust
//! #[macro_use]
//! extern crate derive_is_enum_variant;
//!
//! #[derive(is_enum_variant)]
//! pub enum Pet {
//!     Doggo,
//!     Kitteh,
//! }
//!
//! fn main() {
//!     let pet = Pet::Doggo;
//!
//!     assert!(pet.is_doggo());
//!     assert!(!pet.is_kitteh());
//! }
//! ```
//!
//! ### Customizing Predicate Names
//!
//! By default, the predicates are named `is_snake_case_of_variant_name`. You can
//! use any name you want instead with `#[is_enum_variant(name = "..")]`:
//!
//! ```rust
//! # #[macro_use]
//! # extern crate derive_is_enum_variant;
//!
//! #[derive(is_enum_variant)]
//! pub enum Pet {
//!     #[is_enum_variant(name = "is_real_good_boy")]
//!     Doggo,
//!     Kitteh,
//! }
//!
//! # fn main() {
//! let pet = Pet::Doggo;
//! assert!(pet.is_real_good_boy());
//! # }
//! ```
//!
//! ### Skipping Predicates for Certain Variants
//!
//! If you don't want to generate a predicate for a certain variant, you can use
//! `#[is_enum_variant(skip)]`:
//!
//! ```rust
//! # #[macro_use]
//! # extern crate derive_is_enum_variant;
//!
//! #[derive(is_enum_variant)]
//! pub enum Errors {
//!     Io(::std::io::Error),
//!
//!     #[doc(hidden)]
//!     #[is_enum_variant(skip)]
//!     __NonExhaustive,
//! }
//!
//! # fn main() {}
//! ```

#[macro_use]
extern crate quote;

use heck::SnakeCase;
use proc_macro::TokenStream;

#[proc_macro_derive(is_enum_variant, attributes(is_enum_variant))]
pub fn derive_is_enum_variant(tokens: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(tokens).expect("should parse input tokens into AST");

    let name = ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let data_enum = match ast.data {
        syn::Data::Enum(data_enum) => data_enum,
        _ => panic!("#[derive(is_enum_variant)] can only be used with enums"),
    };
    let predicates = data_enum.variants.into_iter().map(
        |syn::Variant {
             attrs,
             ident,
             fields,
             ..
         }| {
            let cfg = attrs.into();
            if let PredicateConfig::Skip = cfg {
                return quote! {};
            }

            let variant_name = ident.to_string();
            let doc = format!("Is this `{}` a `{}`?", name, variant_name);

            let predicate_name = if let PredicateConfig::Name(name) = cfg {
                name
            } else {
                format!("is_{}", variant_name.to_snake_case())
            };
            let fn_name = syn::Ident::new(&predicate_name, proc_macro2::Span::call_site());

            let data_tokens = match fields {
                syn::Fields::Named(..) => quote! { { .. } },
                syn::Fields::Unnamed(..) => quote! { (..) },
                syn::Fields::Unit => quote! {},
            };

            quote! {
                #[doc = #doc]
                #[inline]
                #[allow(unreachable_patterns)]
                #[allow(dead_code)]
                pub fn #fn_name(&self) -> bool {
                    matches!(*self, #name :: #ident #data_tokens)
                }
            }
        },
    );

    TokenStream::from(quote! {
        /// # `enum` Variant Predicates
        impl #impl_generics #name #ty_generics #where_clause {
            #(
                #predicates
            )*
        }
    })
}

enum PredicateConfig {
    None,
    Skip,
    Name(String),
}

impl PredicateConfig {
    fn join(self, meta: syn::Meta) -> Self {
        match meta {
            syn::Meta::Path(path) if path.is_ident("skip") => match self {
                PredicateConfig::None | PredicateConfig::Skip => PredicateConfig::Skip,
                PredicateConfig::Name(_) => panic!(
                    "Cannot both `#[is_enum_variant(skip)]` and \
                     `#[is_enum_variant(name = \"..\")]`"
                ),
            },
            syn::Meta::NameValue(syn::MetaNameValue {
                path,
                lit: syn::Lit::Str(s),
                ..
            }) if path.is_ident("name") => {
                let value = s.value();
                if !value
                    .chars()
                    .all(|c| matches!(c, '_' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
                {
                    panic!(
                        "#[is_enum_variant(name = \"..\")] must be provided \
                         a valid identifier"
                    )
                }
                match self {
                    PredicateConfig::None => PredicateConfig::Name(value),
                    PredicateConfig::Skip => panic!(
                        "Cannot both `#[is_enum_variant(skip)]` and \
                         `#[is_enum_variant(name = \"..\")]`"
                    ),
                    PredicateConfig::Name(_) => panic!(
                        "Cannot provide more than one \
                         `#[is_enum_variant(name = \"..\")]`"
                    ),
                }
            }
            otherwise => panic!(
                "Unknown item inside `#[is_enum_variant(..)]`: {:?}",
                otherwise
            ),
        }
    }
}

impl From<Vec<syn::Attribute>> for PredicateConfig {
    fn from(attrs: Vec<syn::Attribute>) -> Self {
        let our_attr = attrs
            .into_iter()
            .find(|attr| attr.path.is_ident("is_enum_variant"));
        our_attr.map_or(PredicateConfig::None, |attr| {
            match attr.parse_meta().expect("unable to parse Meta") {
                syn::Meta::List(list) => list
                    .nested
                    .into_iter()
                    .map(|meta| match meta {
                        syn::NestedMeta::Meta(meta) => meta,
                        syn::NestedMeta::Lit(_) => {
                            panic!("Invalid #[is_enum_variant] item")
                        }
                    })
                    .fold(PredicateConfig::None, PredicateConfig::join),
                _ => panic!(
                    "#[is_enum_variant] must be used with name/value pairs, like \
                    #[is_enum_variant(name = \"..\")]"
                ),
            }
        })
    }
}
