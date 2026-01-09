use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, ImplItemFn, Pat, PatType, Type, TypePath, parse_macro_input};

// Reuse the same structures from routable.rs
#[derive(Debug, FromMeta)]
struct RouteArgs {
    /// Optional logical name; default: function ident
    #[darling(default)]
    name: Option<String>,

    /// Optional HTTP method or methods; default: "GET"
    #[darling(default, multiple, rename = "method")]
    methods: Vec<String>,

    /// Optional URL (relative); default: "/{name}"
    #[darling(default, rename = "url")]
    path: Option<String>,

    /// Optional summary; default: from first line of doc comments
    #[darling(default)]
    summary: Option<String>,

    /// Optional description; default: from remaining doc comments
    #[darling(default)]
    description: Option<String>,

    /// Optional tags for this specific route
    #[darling(default, multiple, rename = "tag")]
    tags: Vec<String>,

    /// Optional per-parameter overrides
    #[darling(default, multiple, rename = "param")]
    param: Vec<ParamArgs>,

    /// Optional response overrides
    #[darling(default, multiple, rename = "response")]
    response: Vec<ResponseSpec>,
}

#[derive(Debug, FromMeta)]
struct ParamArgs {
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    ty: Option<syn::Type>,
    #[darling(default)]
    source: Option<String>,
}

#[derive(Debug, FromMeta)]
struct ResponseSpec {
    #[darling(default)]
    status: Option<u16>,
    #[darling(default)]
    ty: Option<syn::Type>,
}

// Validate HTTP method
fn validate_http_method(method: &str, span: proc_macro2::Span) -> Result<(), syn::Error> {
    const VALID_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];
    
    let method_upper = method.to_uppercase();
    if !VALID_METHODS.contains(&method_upper.as_str()) {
        return Err(syn::Error::new(
            span,
            format!(
                "Unsupported HTTP method '{}'. Supported methods: {}",
                method,
                VALID_METHODS.join(", ")
            )
        ));
    }
    
    Ok(())
}

// Validate URL path format
fn validate_path(path: &str, span: proc_macro2::Span) -> Result<(), syn::Error> {
    if path.is_empty() {
        return Err(syn::Error::new(span, "Route path cannot be empty"));
    }
    
    if !path.starts_with('/') {
        return Err(syn::Error::new(
            span,
            format!("Route path must start with '/'. Found: '{}'", path)
        ));
    }
    
    if path.contains("//") {
        return Err(syn::Error::new(
            span,
            format!("Route path contains double slashes: '{}'", path)
        ));
    }
    
    Ok(())
}

/// Parse #[route] attribute on free functions
pub(crate) fn parse_route_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error().into(),
    };

    let route_args = match RouteArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };

    let input = parse_macro_input!(item as ItemFn);
    let fn_ident = &input.sig.ident;
    let fn_name = fn_ident.to_string();
    
    let meta_tokens = generate_view_meta(&route_args, &fn_name, &input.sig, &input.attrs);

    let meta_fn_name = syn::Ident::new(
        &format!("__route_meta_{}", fn_name),
        fn_ident.span()
    );

    let expanded = quote! {
        #input

        #[allow(non_snake_case)]
        fn #meta_fn_name() -> ::uxar::views::ViewMeta {
            #meta_tokens
        }
    };

    expanded.into()
}

/// Parse #[route] attribute on methods in impl blocks
pub(crate) fn parse_route_method(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match darling::ast::NestedMeta::parse_meta_list(attr.into()) {
        Ok(v) => v,
        Err(e) => return e.into_compile_error().into(),
    };

    let route_args = match RouteArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => return e.write_errors().into(),
    };

    let input = parse_macro_input!(item as ImplItemFn);
    let fn_ident = &input.sig.ident;
    let fn_name = fn_ident.to_string();
    
    let meta_tokens = generate_view_meta(&route_args, &fn_name, &input.sig, &input.attrs);

    let meta_fn_name = syn::Ident::new(
        &format!("__route_meta_{}", fn_name),
        fn_ident.span()
    );

    let expanded = quote! {
        #input

        #[allow(non_snake_case)]
        fn #meta_fn_name() -> ::uxar::views::ViewMeta {
            #meta_tokens
        }
    };

    expanded.into()
}

fn generate_view_meta(
    args: &RouteArgs,
    fn_name: &str,
    sig: &syn::Signature,
    attrs: &[syn::Attribute],
) -> proc_macro2::TokenStream {
    let name_str = args.name.clone().unwrap_or_else(|| fn_name.to_string());
    let path_str = args.path.clone().unwrap_or_else(|| format!("/{}", name_str));
    
    // Validate path
    if let Err(e) = validate_path(&path_str, sig.ident.span()) {
        return e.to_compile_error();
    }
    
    // Normalize methods
    let http_methods: Vec<String> = if args.methods.is_empty() {
        vec!["GET".to_string()]
    } else {
        args.methods.iter().map(|m| m.to_uppercase()).collect()
    };
    
    // Validate methods
    for method in &http_methods {
        if let Err(e) = validate_http_method(method, sig.ident.span()) {
            return e.to_compile_error();
        }
    }
    
    let method_tokens: Vec<_> = http_methods.iter().map(|m| method_to_token(m)).collect();
    let method_filter_tokens = build_method_filter(&http_methods);
    let summary_tokens = build_summary_tokens(args.summary.as_ref(), attrs);
    let description_tokens = build_description_tokens(args.description.as_ref(), attrs);
    let tags_tokens = build_tags_tokens(&args.tags);
    let params_tokens = build_params_tokens(sig, args);
    let responses_tokens = build_responses_tokens(sig, args);

    quote! {
        ::uxar::views::ViewMeta {
            name: ::std::borrow::Cow::Borrowed(#name_str),
            method_filter: #method_filter_tokens,
            methods: ::std::vec![ #(#method_tokens),* ],
            path: ::std::borrow::Cow::Borrowed(#path_str),
            summary: #summary_tokens,
            description: #description_tokens,
            tags: #tags_tokens,
            params: #params_tokens,
            responses: #responses_tokens,
        }
    }
}

fn method_to_token(method: &str) -> proc_macro2::TokenStream {
    match method {
        "GET" => quote! { ::axum::http::Method::GET },
        "POST" => quote! { ::axum::http::Method::POST },
        "PUT" => quote! { ::axum::http::Method::PUT },
        "DELETE" => quote! { ::axum::http::Method::DELETE },
        "PATCH" => quote! { ::axum::http::Method::PATCH },
        "OPTIONS" => quote! { ::axum::http::Method::OPTIONS },
        "HEAD" => quote! { ::axum::http::Method::HEAD },
        _ => panic!("Invalid HTTP method passed validation: {}", method),
    }
}

fn build_method_filter(methods: &[String]) -> proc_macro2::TokenStream {
    if methods.is_empty() {
        return quote! { ::axum::routing::MethodFilter::GET };
    }
    
    let filters: Vec<_> = methods.iter().map(|m| {
        match m.as_str() {
            "GET" => quote! { ::axum::routing::MethodFilter::GET },
            "POST" => quote! { ::axum::routing::MethodFilter::POST },
            "PUT" => quote! { ::axum::routing::MethodFilter::PUT },
            "DELETE" => quote! { ::axum::routing::MethodFilter::DELETE },
            "PATCH" => quote! { ::axum::routing::MethodFilter::PATCH },
            "OPTIONS" => quote! { ::axum::routing::MethodFilter::OPTIONS },
            "HEAD" => quote! { ::axum::routing::MethodFilter::HEAD },
            _ => panic!("Invalid HTTP method: {}", m),
        }
    }).collect();
    
    if filters.len() == 1 {
        filters[0].clone()
    } else {
        let first = &filters[0];
        let rest = &filters[1..];
        quote! {
            #first #(.or(#rest))*
        }
    }
}

/// Extract doc comments and split into summary and description
fn doc_from_attrs(attrs: &[syn::Attribute]) -> (Option<String>, Option<String>) {
    let docs: Vec<_> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value());
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        return (None, None);
    }

    let full_doc = docs.join("\n");
    let parts: Vec<&str> = full_doc.split("\n\n").collect();
    
    if parts.is_empty() {
        return (None, None);
    }
    
    let first_para = parts[0].trim();
    let (summary, first_para_remainder) = if first_para.is_empty() {
        (None, None)
    } else {
        let mut lines = first_para.lines();
        let first_line = lines.next().map(|s| s.trim().to_string());
        let remaining: Vec<_> = lines.map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        let remainder = if remaining.is_empty() {
            None
        } else {
            Some(remaining.join("\n"))
        };
        (first_line, remainder)
    };
    
    let mut desc_parts = Vec::new();
    if let Some(remainder) = first_para_remainder {
        desc_parts.push(remainder);
    }
    if parts.len() > 1 {
        for part in &parts[1..] {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                desc_parts.push(trimmed.to_string());
            }
        }
    }
    
    let description = if desc_parts.is_empty() {
        None
    } else {
        Some(desc_parts.join("\n\n"))
    };
    
    (summary, description)
}

fn build_summary_tokens(
    summary_attr: Option<&String>,
    attrs: &[syn::Attribute],
) -> proc_macro2::TokenStream {
    if let Some(s) = summary_attr {
        quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#s)) }
    } else {
        let (summary, _) = doc_from_attrs(attrs);
        if let Some(doc) = summary {
            quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#doc)) }
        } else {
            quote! { ::std::option::Option::None }
        }
    }
}

fn build_description_tokens(
    description_attr: Option<&String>,
    attrs: &[syn::Attribute],
) -> proc_macro2::TokenStream {
    if let Some(d) = description_attr {
        quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#d)) }
    } else {
        let (_, description) = doc_from_attrs(attrs);
        if let Some(doc) = description {
            quote! { ::std::option::Option::Some(::std::borrow::Cow::Borrowed(#doc)) }
        } else {
            quote! { ::std::option::Option::None }
        }
    }
}

fn build_tags_tokens(tags: &[String]) -> proc_macro2::TokenStream {
    if tags.is_empty() {
        quote! { ::std::vec![] }
    } else {
        quote! {
            ::std::vec![ #(::std::borrow::Cow::Borrowed(#tags)),* ]
        }
    }
}

fn build_params_tokens(sig: &syn::Signature, args: &RouteArgs) -> proc_macro2::TokenStream {
    if args.param.iter().any(|p| p.name.is_none()) {
        return quote! {
            compile_error!("All #[route(param(...))] entries must provide a `name = \"...\"`");
        };
    }

    let mut overrides: std::collections::HashMap<String, &ParamArgs> = std::collections::HashMap::new();
    for spec in &args.param {
        if let Some(n) = &spec.name {
            overrides.insert(n.clone(), spec);
        }
    }

    let param_names: Vec<String> = sig.inputs.iter()
        .filter_map(|input| match input {
            FnArg::Typed(pat_type) => Some(extract_param_name(&pat_type.pat)),
            _ => None,
        })
        .collect();

    let unknowns: Vec<String> = overrides
        .keys()
        .filter(|k| !param_names.contains(k))
        .cloned()
        .collect();

    if !unknowns.is_empty() {
        let msg = format!(
            "Unknown #[route(param(...))] override(s): {}. Handler parameters: {}",
            unknowns.join(", "),
            param_names.join(", ")
        );
        return quote! { compile_error!(#msg); };
    }

    let entries: Vec<_> = sig.inputs.iter()
        .filter_map(|input| match input {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => {
                let pname = extract_param_name(&pat_type.pat);
                let meta = build_param_meta(pat_type, overrides.get(&pname).copied());
                if meta.is_empty() { None } else { Some(meta) }
            }
        })
        .collect();

    quote! { ::std::vec![ #(#entries),* ] }
}

fn build_param_meta(pat_type: &PatType, override_spec: Option<&ParamArgs>) -> proc_macro2::TokenStream {
    if let Some(spec) = override_spec {
        if let Some(n) = &spec.name {
            if n == "_" {
                return quote! {};
            }
        }
    }

    let name = override_spec
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| extract_param_name(&pat_type.pat));

    let schema_tokens = match override_spec.and_then(|s| s.ty.clone()) {
        Some(syn::Type::Infer(_)) => quote! { ::std::vec![] },
        Some(ty) => quote! { <#ty as ::uxar::schemables::IntoApiParts>::api_parts() },
        None => {
            let ty = &pat_type.ty;
            quote! { <#ty as ::uxar::schemables::IntoApiParts>::api_parts() }
        }
    };

    quote! {
        ::uxar::views::ParamMeta {
            name: ::std::borrow::Cow::Borrowed(#name),
            fragments: #schema_tokens,
        }
    }
}

fn extract_param_name(pat: &Pat) -> String {
    match pat {
        Pat::Ident(ident) => ident.ident.to_string(),
        Pat::TupleStruct(ts) => {
            ts.elems.first().map_or("_".to_string(), extract_param_name)
        }
        Pat::Tuple(tuple) => tuple.elems.first().map_or("_".to_string(), extract_param_name),
        Pat::Paren(paren) => extract_param_name(&paren.pat),
        Pat::Type(pat_type) => extract_param_name(&pat_type.pat),
        Pat::Reference(pat_ref) => extract_param_name(&pat_ref.pat),
        Pat::Wild(_) => "_".to_string(),
        _ => "_".to_string(),
    }
}

fn build_responses_tokens(sig: &syn::Signature, args: &RouteArgs) -> proc_macro2::TokenStream {
    let func_ty = match &sig.output {
        syn::ReturnType::Default => syn::parse_quote! { () },
        syn::ReturnType::Type(_, ty) => (**ty).clone(),
    };

    let default_specs: Vec<_> = args.response.iter().filter(|s| s.status.is_none()).collect();
    if default_specs.len() > 1 {
        return quote! { compile_error!("Multiple #[route(response(...))] entries without `status` not allowed"); };
    }
    let default_spec = default_specs.first().copied();

    let mut status_entries: Vec<(u16, proc_macro2::TokenStream)> = Vec::new();
    let mut seen_statuses: std::collections::HashSet<u16> = std::collections::HashSet::new();

    for spec in &args.response {
        if let Some(status) = spec.status {
            if !seen_statuses.insert(status) {
                let msg = format!("Multiple #[route(response(status = {}))] entries not allowed", status);
                return quote! { compile_error!(#msg); };
            }

            let ty = spec.ty.clone()
                .or_else(|| default_spec.and_then(|d| d.ty.clone()))
                .unwrap_or_else(|| func_ty.clone());

            let entry = match ty {
                syn::Type::Infer(_) => quote! {
                    ::uxar::views::ReturnMeta { 
                        status: ::std::option::Option::Some(#status), 
                        fragments: ::std::vec![]
                    }
                },
                _ => quote! {
                    ::uxar::views::ReturnMeta { 
                        status: ::std::option::Option::Some(#status), 
                        fragments: <#ty as ::uxar::schemables::IntoApiParts>::api_parts() 
                    }
                },
            };
            status_entries.push((status, entry));
        }
    }

    let default_ty = default_spec
        .and_then(|d| d.ty.clone())
        .unwrap_or(func_ty);

    let default_entry = match default_ty {
        syn::Type::Infer(_) => quote! {
            ::uxar::views::ReturnMeta { 
                status: ::std::option::Option::None, 
                fragments: ::std::vec![]
            }
        },
        _ => quote! {
            ::uxar::views::ReturnMeta { 
                status: ::std::option::Option::None, 
                fragments: <#default_ty as ::uxar::schemables::IntoApiParts>::api_parts() 
            }
        },
    };

    status_entries.sort_by_key(|(s, _)| *s);
    let mut entries: Vec<_> = status_entries.into_iter().map(|(_, v)| v).collect();
    entries.push(default_entry);

    quote! { ::std::vec![ #(#entries),* ] }
}
