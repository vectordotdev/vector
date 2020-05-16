// Can not be used on `stable` channel.
// #![feature(proc_macro_diagnostic)]

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{parse_macro_input, AttributeArgs, ImplItem, Item, Type};

#[derive(Debug, FromMeta)]
struct ImplSinkConfigArgs {
    name: String,
    input_type: String,
    component: String,
}

#[proc_macro_attribute]
pub fn impl_sink_config(metadata: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(metadata as AttributeArgs);
    let args = match ImplSinkConfigArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let (name, input_type, component) = (args.name, args.input_type, args.component);
    let input_type = syn::parse_str::<syn::Path>(&input_type)
        .expect("Failed to parse input_type into `syn::Path`");
    let component = syn::parse_str::<syn::Path>(&component)
        .expect("Failed to parse component into `syn::Path`");

    let mut item: Item = syn::parse(input).expect("Failed to parse input into `syn::Item`");
    let self_ty = match item {
        Item::Impl(ref mut item) => {
            item.items.push(ImplItem::Verbatim(quote! {
                fn input_type(&self) -> DataType {
                    #input_type
                }
            }));

            item.items.push(ImplItem::Verbatim(quote! {
                fn sink_type(&self) -> &'static str {
                    #name
                }
            }));

            match *item.self_ty {
                Type::Path(ref item) => Some(item.path.clone()),
                _ => {
                    // item.self_ty.span().unstable().error("This is not a `syn::Type::Path`").emit();
                    // None
                    panic!(
                        "This is not a `syn::Type::Path`.\n{:?}",
                        item.self_ty.span()
                    );
                }
            }
        }
        _ => {
            // item.span().unstable().error("This is not a `syn::Item::Impl`").emit();
            // None
            panic!("This is not a `syn::Item::Impl`.\n{:?}", item.span());
        }
    };

    let output = quote! {
        #[typetag::serde(name = #name)]
        #item

        inventory::submit! {
            #component::<#self_ty>(#name)
        }
    };
    output.into()
}
