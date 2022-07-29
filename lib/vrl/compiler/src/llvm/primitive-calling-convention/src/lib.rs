use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{
    parse_macro_input, parse_quote, spanned::Spanned, FnArg, Ident, ItemFn, Pat, PatType,
    ReturnType, Stmt, Token, Type, TypePtr,
};

#[proc_macro_attribute]
pub fn primitive_calling_convention(_attributes: TokenStream, input: TokenStream) -> TokenStream {
    let mut function = parse_macro_input!(input as ItemFn);
    let mut errors = Vec::new();

    let primitive_types = [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "char", "bool", "()",
    ];

    let mut read_arguments = Vec::new();
    for input in &mut function.sig.inputs {
        match input {
            FnArg::Receiver(_) => {
                errors.push(quote_spanned! {
                    input.span() => compile_error!("Receiver argument not allowed");
                });
            }
            FnArg::Typed(PatType { pat, ty, .. }) => match **ty {
                Type::Ptr(_) | Type::Reference(_) => {}
                _ if primitive_types.contains(&quote!(#ty).to_string().as_str()) => {}
                _ => {
                    match **pat {
                        Pat::Ident(ref ident) => {
                            let elem = ty.clone();
                            let ident = ident.clone();
                            read_arguments.push((ident.mutability, ident.ident));
                            *ty = Box::new(Type::Ptr(TypePtr {
                                star_token: <Token![*]>::default(),
                                const_token: None,
                                mutability: Some(<Token![mut]>::default()),
                                elem,
                            }));
                        },
                        _ => errors.push(quote_spanned! {
                            input.span() => compile_error!("Only function arguments with regular identifiers are supported");
                        }),
                    };
                }
            },
        }
    }

    if !read_arguments.is_empty() {
        let unsafety = function.sig.unsafety;
        function.sig.unsafety = Some(unsafety.unwrap_or_default());

        if unsafety.is_none() {
            function
                .attrs
                .push(parse_quote! { #[warn(unsafe_op_in_unsafe_fn)] });
            function.attrs.push(parse_quote! { #[doc = ""] });
            function.attrs.push(parse_quote! { #[doc = " # Safety"] });
        }

        function.attrs.push(parse_quote! { #[doc = ""] });
        let safety_description = format!(
            " Ownership of the contents of argument{} {} is transferred to this function.",
            if read_arguments.len() > 1 { "s" } else { "" },
            match read_arguments.len() {
                1 => format!("`{}`", read_arguments[0].1),
                n => format!(
                    "{} and `{}`",
                    read_arguments[0..n - 1]
                        .iter()
                        .map(|(_, identifier)| format!(r#"`{}`"#, identifier))
                        .collect::<Vec<_>>()
                        .join(", "),
                    read_arguments[n - 1].1
                ),
            }
        );
        function
            .attrs
            .push(parse_quote! { #[doc = #safety_description] });
    }

    read_arguments.reverse();
    for (mutability, ident) in read_arguments {
        let statement: Stmt = parse_quote! {
            let #mutability #ident = unsafe { #ident.read() };
        };
        function.block.stmts.insert(0, statement);
    }

    if let ReturnType::Type(_, ref ty) = function.sig.output {
        match **ty {
            Type::Ptr(_) | Type::Reference(_) => {}
            _ if primitive_types.contains(&quote!(#ty).to_string().as_str()) => {}
            _ => {
                let ident_return = Ident::new_raw("return", function.sig.ident.span());
                let argument = parse_quote! {
                    #ident_return: &mut #ty
                };
                function.sig.inputs.push(argument);
                function.sig.output = ReturnType::Default;
                let block = function.block.clone();
                function.block = Box::new(parse_quote!(
                    {
                        *#ident_return = (||{ #block })();
                    }
                ));
            }
        }
    }

    let expanded = quote! {
        #function

        #(#errors)*
    };

    TokenStream::from(expanded)
}
