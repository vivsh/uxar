use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, LitStr, Meta, Pat, PatType, parse_macro_input};

/// Identity macro; all logic happens in `#[viewable]`.
pub(crate) fn parse_action(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[derive(Debug, FromMeta)]
struct ActionArgs {
    /// Optional logical name; default: method ident
    #[darling(default)]
    name: Option<String>,

    /// Optional HTTP method; default: "GET"
    #[darling(default)]
    method: Option<String>,

    /// Optional URL (relative); default: "/{name}"
    #[darling(default, rename = "url")]
    path: Option<String>,

    /// Optional summary; default: from doc comments
    #[darling(default)]
    summary: Option<String>,
}

#[derive(Debug, FromMeta)]
struct ViewableArgs {
    /// Optional group name (unused for now)
    #[darling(default)]
    name: Option<String>,

    /// Optional base path; e.g. "/api"
    #[darling(default)]
    base_path: Option<String>,
}


pub fn parse_viewable(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse #[viewable(...)] arguments with darling
    let args = match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error().into(),
    };

    let input = parse_macro_input!(item as ItemImpl);

    let viewable_args = match ViewableArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };

    let self_ty = *input.self_ty.clone();

    let methods = match collect_action_methods(&input) {
        Ok(m) => m,
        Err(ts) => return ts,
    };

    let (meta_entries, route_chain) = build_meta_and_routes(&methods, &viewable_args);

    let viewable_impl = quote! {
        impl ::uxar::views::Viewable for #self_ty {
            fn describe_routes() -> ::std::vec::Vec<::uxar::views::ViewMeta> {
                ::std::vec![ #(#meta_entries),* ]
            }

            fn register_routes(mut router: ::uxar::views::RouterType) -> ::uxar::views::RouterType {
                #(#route_chain)*
                router
            }
        }
    };

    let expanded = quote! {
        #input
        #viewable_impl
    };

    expanded.into()
}

// ---------------------------------------------------------
// Helpers
// ---------------------------------------------------------

fn collect_action_methods(input: &ItemImpl) -> Result<Vec<(ImplItemFn, ActionArgs)>, TokenStream> {
    let mut res = Vec::new();

    for item in &input.items {
        let fn_item = match item {
            ImplItem::Fn(f) => f,
            _ => continue,
        };

        let mut found: Option<ActionArgs> = None;

        for attr in &fn_item.attrs {
            if !attr.path().is_ident("action") {
                continue;
            }

            let nested = match &attr.meta {
                Meta::List(list) => {
                    darling::ast::NestedMeta::parse_meta_list(list.tokens.clone().into())
                }
                Meta::Path(_) => Ok(Vec::new()),
                Meta::NameValue(_) => {
                    let err = syn::Error::new(attr.span(), "expected #[action] or #[action(...)]");
                    return Err(err.to_compile_error().into());
                }
            };

            let nested = match nested {
                Ok(v) => v,
                Err(e) => return Err(e.into_compile_error().into()),
            };

            let parsed = match ActionArgs::from_list(&nested) {
                Ok(a) => a,
                Err(e) => return Err(e.write_errors().into()),
            };

            found = Some(parsed);
            break;
        }

        if let Some(args) = found {
            res.push((fn_item.clone(), args));
        }
    }

    Ok(res)
}

fn build_meta_and_routes(
    methods: &[(ImplItemFn, ActionArgs)],
    viewable_args: &ViewableArgs,
) -> (Vec<proc_macro2::TokenStream>, Vec<proc_macro2::TokenStream>) {
    let mut meta_entries = Vec::new();
    let mut route_chain = Vec::new();

    for (fn_item, action) in methods {
        let sig = &fn_item.sig;
        let fn_ident = &sig.ident;

        // ---- name ----
        let name_str = action.name.clone().unwrap_or_else(|| fn_ident.to_string());
        let name_lit = name_str.clone();

        // ---- method ----
        let method_upper = action.method.as_deref().unwrap_or("GET").to_uppercase();

        let (method_token, route_fn) = map_method_to_tokens(&method_upper, fn_ident);

        // ---- path ----
        let path_str = build_full_path(
            &name_str,
            action.path.as_deref(),
            viewable_args.base_path.as_deref(),
        );
        let path_lit = path_str.clone();

        // ---- summary ----
        let summary_tokens = build_summary_tokens(action.summary.as_ref(), fn_item);

        // ---- params + return type ----
        let params_tokens = build_params_tokens(sig);
        let ret_ty_tokens = build_return_type_tokens(sig);

        meta_entries.push(quote! {
            ::uxar::views::ViewMeta {
                name: #name_lit,
                method: #method_token,
                path: #path_lit,
                summary: #summary_tokens,
                params: #params_tokens,
                return_type: #ret_ty_tokens,
            }
        });

        route_chain.push(quote! {
            router = router.route(#path_lit, #route_fn);
        });
    }

    (meta_entries, route_chain)
}

fn map_method_to_tokens(
    method_upper: &str,
    fn_ident: &syn::Ident,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    match method_upper {
        "GET" => (
            quote! { ::axum::http::Method::GET },
            quote! { ::axum::routing::get(Self::#fn_ident) },
        ),
        "POST" => (
            quote! { ::axum::http::Method::POST },
            quote! { ::axum::routing::post(Self::#fn_ident) },
        ),
        "PUT" => (
            quote! { ::axum::http::Method::PUT },
            quote! { ::axum::routing::put(Self::#fn_ident) },
        ),
        "DELETE" => (
            quote! { ::axum::http::Method::DELETE },
            quote! { ::axum::routing::delete(Self::#fn_ident) },
        ),
        "PATCH" => (
            quote! { ::axum::http::Method::PATCH },
            quote! { ::axum::routing::patch(Self::#fn_ident) },
        ),
        "OPTIONS" => (
            quote! { ::axum::http::Method::OPTIONS },
            quote! { ::axum::routing::options(Self::#fn_ident) },
        ),
        "HEAD" => (
            quote! { ::axum::http::Method::HEAD },
            quote! { ::axum::routing::any(Self::#fn_ident) },
        ),
        other => {
            let msg = format!("Unsupported HTTP method {:?} in #[action]", other);
            (
                quote! { ::axum::http::Method::GET },
                quote! { ::axum::routing::any(|| async { compile_error!(#msg); }) },
            )
        }
    }
}

fn build_full_path(name: &str, explicit_path: Option<&str>, base_path: Option<&str>) -> String {
    let rel = explicit_path
        .map(|p| p.to_string())
        .unwrap_or_else(|| format!("/{}", name));

    if let Some(base) = base_path {
        if base.is_empty() {
            rel
        } else if base.ends_with('/') {
            format!("{}{}", base, rel.trim_start_matches('/'))
        } else {
            format!("{}/{}", base, rel.trim_start_matches('/'))
        }
    } else {
        rel
    }
}

fn doc_from_fn(fn_item: &ImplItemFn) -> Option<String> {
    let mut docs = Vec::new();

    for attr in &fn_item.attrs {
        if attr.path().is_ident("doc") {
            // In syn 2, for `#[doc = "…"]` the tokens are just `"…"`
            if let Ok(s) = attr.parse_args::<LitStr>() {
                docs.push(s.value());
            }
        }
    }

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

fn build_summary_tokens(
    summary_attr: Option<&String>,
    fn_item: &ImplItemFn,
) -> proc_macro2::TokenStream {
    if let Some(s) = summary_attr {
        quote! { Some(#s) }
    } else if let Some(doc) = doc_from_fn(fn_item) {
        quote! { Some(#doc) }
    } else {
        quote! { None }
    }
}

fn build_params_tokens(sig: &syn::Signature) -> proc_macro2::TokenStream {
    let mut entries = Vec::new();

    for input in &sig.inputs {
        match input {
            FnArg::Receiver(_) => {}
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let name = match &**pat {
                    Pat::Ident(ident) => ident.ident.to_string(),
                    _ => "_".to_string(),
                };

                entries.push(quote! {
                    ::uxar::views::ParamMeta {
                        name: #name,
                        ty: stringify!(#ty),
                    }
                });
            }
        }
    }

    quote! { &[ #(#entries),* ] }
}

fn build_return_type_tokens(sig: &syn::Signature) -> proc_macro2::TokenStream {
    match &sig.output {
        syn::ReturnType::Default => quote! { "()" },
        syn::ReturnType::Type(_, ty) => quote! { stringify!(#ty) },
    }
}
