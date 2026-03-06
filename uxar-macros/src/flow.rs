use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ImplItemFn};

#[derive(Debug, Default, FromMeta)]
struct FlowArgs {
    /// Optional task name
    #[darling(default)]
    name: Option<String>,
}

/// Unified implementation for both free functions and methods
pub(crate) fn parse_flow(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = if attr.is_empty() {
        FlowArgs::default()
    } else {
        match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
            Ok(v) => match FlowArgs::from_list(&v) {
                Ok(args) => args,
                Err(e) => return e.write_errors().into(),
            },
            Err(e) => return e.into_compile_error().into(),
        }
    };

    let (original, fn_ident, is_method) = if let Ok(func) = syn::parse::<ItemFn>(item.clone()) {
        let ident = func.sig.ident.clone();
        (quote! { #func }, ident, false)
    } else if let Ok(method) = syn::parse::<ImplItemFn>(item.clone()) {
        let ident = method.sig.ident.clone();
        (quote! { #method }, ident, true)
    } else {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[flow] can only be applied to functions or methods"
        )
        .to_compile_error()
        .into();
    };

    let fn_name = fn_ident.to_string();
    let task_name = args.name.as_ref().unwrap_or(&fn_name);

    let bundle_part_fn_name = syn::Ident::new(
        &format!("__bundle_part_{}", fn_name),
        fn_ident.span()
    );

    let call_expr = if is_method {
        quote! { Self::#fn_ident }
    } else {
        quote! { #fn_ident }
    };

    let expanded = quote! {
        #original

        #[allow(non_snake_case)]
        fn #bundle_part_fn_name() -> ::uxar::bundles::BundlePart {
            ::uxar::bundles::BundlePart::from_flow_task(#task_name, #call_expr)
        }
    };

    expanded.into()
}
