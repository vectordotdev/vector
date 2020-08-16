// The MIT License (MIT)
//
// Copyright (c) 2019 Tokio Contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use proc_macro::TokenStream;
use quote::quote;
use std::num::NonZeroUsize;

#[derive(Clone, Copy, PartialEq)]
enum Scheduler {
    Basic,
    Threaded,
}

#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    parse(args, input).unwrap_or_else(|e| e.to_compile_error().into())
}

fn parse(args: syn::AttributeArgs, mut input: syn::ItemFn) -> Result<TokenStream, syn::Error> {
    let sig = &mut input.sig;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    for attr in attrs {
        if attr.path.is_ident("test") {
            let msg = "second test attribute is supplied";
            return Err(syn::Error::new_spanned(&attr, msg));
        }
    }

    if !sig.inputs.is_empty() {
        let msg = "the test function cannot accept arguments";
        return Err(syn::Error::new_spanned(&sig.inputs, msg));
    }

    if sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(sig.fn_token, msg));
    }

    sig.asyncness = None;

    let mut scheduler = None;
    let mut core_threads = None;

    for arg in args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "core_threads" => match &namevalue.lit {
                        syn::Lit::Int(expr) => {
                            let num = expr.base10_parse::<NonZeroUsize>().unwrap();
                            core_threads = Some(num);
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "core_threads argument must be an int",
                            ))
                        }
                    },
                    name => {
                        let msg = format!("Unknown attribute pair {} is specified; expected one of: `core_threads`", name);
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            syn::NestedMeta::Meta(syn::Meta::Path(path)) => {
                let ident = path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(path, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "threaded_scheduler" => scheduler = Some(Scheduler::Threaded),
                    "basic_scheduler" => scheduler = Some(Scheduler::Basic),
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                        return Err(syn::Error::new_spanned(path, msg));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unknown attribute inside the macro",
                ));
            }
        }
    }

    let mut rt = quote! { tokio::runtime::Builder::new().threaded_scheduler() };
    if scheduler == Some(Scheduler::Basic) {
        rt = quote! { #rt.basic_scheduler() };
    }
    if let Some(v) = core_threads.map(|v| v.get()) {
        rt = quote! { #rt.core_threads(#v) };
    }

    let result = quote! {
        #[::core::prelude::v1::test]
        #(#attrs)*
        #vis #sig {
            #rt
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    crate::test_util::trace_init();
                    #body
                })
        }
    };

    Ok(result.into())
}
