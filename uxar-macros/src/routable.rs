use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{
    FnArg, GenericArgument, ImplItem, ImplItemFn, ItemImpl, LitStr, Meta, Pat, PatType,
    PathArguments, Type, TypePath, parse_macro_input,
};

/// Identity macro; all logic happens in `#[viewable]`.
pub(crate) fn parse_action(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

// Validate HTTP method at compile time
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
    
    // Check for double slashes
    if path.contains("//") {
        return Err(syn::Error::new(
            span,
            format!("Route path contains double slashes: '{}'", path)
        ));
    }
    
    // Check for trailing slash (except root)
    if path.len() > 1 && path.ends_with('/') {
        return Err(syn::Error::new(
            span,
            format!("Route path should not end with '/': '{}'. Use '{}' instead.", path, path.trim_end_matches('/'))
        ));
    }
    
    Ok(())
}

#[derive(Debug, FromMeta)]
struct ActionArgs {
    /// Optional logical name; default: method ident
    #[darling(default)]
    name: Option<String>,

    /// Optional HTTP method or methods; default: "GET"
    /// Can be a single method: method = "POST"
    /// Or multiple methods: method = ["GET", "POST"]
    #[darling(default, multiple, rename = "method")]
    methods: Vec<String>,

    /// Optional URL (relative); default: "/{name}"
    #[darling(default, rename = "url")]
    path: Option<String>,

    /// Optional summary; default: from doc comments
    #[darling(default)]
    summary: Option<String>,

    /// Optional per-parameter overrides. These are ordered and match function
    /// parameter positions (excluding `&self`). Use `name = "_"` to hide a
    /// parameter from docs. Use `ty = _` to explicitly mark no schema.
    #[darling(default, multiple, rename = "param")]
    param: Vec<ParamArgs>,

    /// Optional response overrides. Multiple allowed.
    #[darling(default, multiple, rename = "response")]
    response: Vec<ResponseSpec>,
}


#[derive(Debug, FromMeta)]
struct ParamArgs {
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    ty: Option<syn::Type>,
    /// Optional source/location: e.g. "path", "query", "body"
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

#[derive(Debug, FromMeta)]
struct ViewableArgs {
    /// Optional group name (unused for now)
    #[darling(default)]
    #[allow(dead_code)]
    name: Option<String>,

    /// Optional base path; e.g. "/api"
    #[darling(default)]
    base_path: Option<String>,
}


pub fn parse_routable(attr: TokenStream, item: TokenStream) -> TokenStream {
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
        impl ::uxar::views::StaticRoutable for #self_ty {

            fn as_routable() -> (::axum::Router<::uxar::Site>, ::std::vec::Vec<::uxar::views::ViewMeta>) {
                let mut router = ::uxar::views::AxumRouter::new();
                #(#route_chain)*;

                let metas = ::std::vec![ #(#meta_entries),* ];
                (router, metas)
            }
        }

        impl #self_ty {
            fn as_routable() -> (::axum::Router<::uxar::Site>, ::std::vec::Vec<::uxar::views::ViewMeta>) {
                <Self as ::uxar::views::StaticRoutable>::as_routable()
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
    input
        .items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(fn_item) => parse_action_attr(fn_item),
            _ => None,
        })
        .collect()
}

fn parse_action_attr(fn_item: &ImplItemFn) -> Option<Result<(ImplItemFn, ActionArgs), TokenStream>> {
    fn_item
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("route"))
        .map(|attr| parse_action_args(attr, fn_item))
}

fn parse_action_args(
    attr: &syn::Attribute,
    fn_item: &ImplItemFn,
) -> Result<(ImplItemFn, ActionArgs), TokenStream> {
    let nested = match &attr.meta {
        Meta::List(list) => darling::ast::NestedMeta::parse_meta_list(list.tokens.clone()),
        Meta::Path(_) => Ok(Vec::new()),
        Meta::NameValue(_) => {
            let err = syn::Error::new(attr.span(), "expected #[route] or #[route(...)]");
            return Err(err.to_compile_error().into());
        }
    };

    let nested = nested.map_err(|e| e.into_compile_error())?;
    let args = ActionArgs::from_list(&nested).map_err(|e| e.write_errors())?;

    Ok((fn_item.clone(), args))
}

fn build_meta_and_routes(
    methods: &[(ImplItemFn, ActionArgs)],
    viewable_args: &ViewableArgs,
) -> (Vec<proc_macro2::TokenStream>, Vec<proc_macro2::TokenStream>) {
    methods
        .iter()
        .flat_map(|(fn_item, action)| {
            build_action_routes(fn_item, action, viewable_args)
        })
        .unzip()
}

fn build_action_routes(
    fn_item: &ImplItemFn,
    action: &ActionArgs,
    viewable_args: &ViewableArgs,
) -> Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let sig = &fn_item.sig;
    let fn_ident = &sig.ident;
    let name_str = action.name.clone().unwrap_or_else(|| fn_ident.to_string());
    let path_str = build_full_path(&name_str, action.path.as_deref(), viewable_args.base_path.as_deref());
    
    // Validate path format
    if let Err(e) = validate_path(&path_str, fn_ident.span()) {
        return vec![(e.to_compile_error(), quote! {})];
    }
    
    let path_lit = path_str.clone();
    
    let http_methods = normalize_methods(&action.methods);
    
    // Validate all HTTP methods
    for method in &http_methods {
        if let Err(e) = validate_http_method(method, fn_ident.span()) {
            return vec![(e.to_compile_error(), quote! {})];
        }
    }
    
    // Validate HEAD handlers: they must not accept body extractors
    if http_methods.iter().any(|m| m == "HEAD")
        && signature_accepts_body(sig) {
            let err = syn::Error::new(
                fn_ident.span(),
                "HEAD request handlers must not accept body extractors (Json, Form, Bytes, String, Multipart). HEAD requests do not have a body."
            );
            return vec![(err.to_compile_error(), quote! {})];
        }
    
    let summary_tokens = build_summary_tokens(action.summary.as_ref(), fn_item);
    let params_tokens = build_params_tokens(sig, action);
    let responses_tokens = build_responses_tokens(sig, action);

    let method_routes: Vec<_> = http_methods
        .iter()
        .map(|method_str| build_method_route(method_str, fn_ident))
        .collect();

    vec![build_final_route(&name_str, &path_lit, &summary_tokens, &params_tokens, &responses_tokens, method_routes)]
}

fn normalize_methods(methods: &[String]) -> Vec<String> {
    if methods.is_empty() {
        vec!["GET".to_string()]
    } else {
        methods.iter().map(|m| m.to_uppercase()).collect()
    }
}

fn build_method_route(
    method_str: &str,
    fn_ident: &syn::Ident,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let (method_token, route_fn) = map_method_to_tokens(method_str, fn_ident);
    (method_token, route_fn)
}

fn build_final_route(
    name_str: &str,
    path_lit: &str,
    summary_tokens: &proc_macro2::TokenStream,
    params_tokens: &proc_macro2::TokenStream,
    responses_tokens: &proc_macro2::TokenStream,
    method_routes: Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)>,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    // Split into method tokens and route functions
    let (method_tokens, routes): (Vec<_>, Vec<_>) = method_routes.into_iter().unzip();

    let route_chain = if routes.len() == 1 {
        let route = &routes[0];
        quote! { router = router.route(#path_lit, #route); }
    } else {
        let first = &routes[0];
        let rest = &routes[1..];
        quote! {
            router = router.route(#path_lit, {
                let r = #first;
                #( let r = r.merge(#rest); )*
                r
            });
        }
    };

    // Single meta with methods vec
    let methods_vec = quote! { ::std::vec![ #(#method_tokens),* ] };

    let meta = quote! {
        ::uxar::views::ViewMeta {
            name: ::std::borrow::Cow::Owned(String::from(#name_str)),
            methods: #methods_vec,
            path: ::std::borrow::Cow::Owned(String::from(#path_lit)),
            summary: #summary_tokens,
            params: #params_tokens,
            responses: #responses_tokens,
        }
    };

    (meta, route_chain)
}

fn map_method_to_tokens(
    method_upper: &str,
    fn_ident: &syn::Ident,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    // Note: validation happens before this is called, so we should only see valid methods
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
            quote! { ::axum::routing::head(Self::#fn_ident) },
        ),
        _ => {
            // Should never reach here due to validation
            panic!("Invalid HTTP method '{}' passed validation", method_upper)
        }
    }
}

fn signature_accepts_body(sig: &syn::Signature) -> bool {
    const BODY_EXTRACTORS: &[&str] = &["Json", "Form", "Bytes", "String", "Multipart", "BodyStream"];
    
    for input in sig.inputs.iter() {
        if let FnArg::Typed(pat_type) = input {
            let ty = &*pat_type.ty;
            if let Some(name) = extractor_name_of_type(ty)
                && BODY_EXTRACTORS.contains(&name.as_str()) {
                    return true;
                }
        }
    }
    false
}

fn build_full_path(name: &str, explicit_path: Option<&str>, base_path: Option<&str>) -> String {
    let default_path = format!("/{}", name);
    let relative_path = explicit_path.unwrap_or(&default_path);
    
    match base_path {
        None | Some("") => relative_path.to_string(),
        Some(base) => {
            let base_clean = base.trim_end_matches('/');
            let rel_clean = relative_path.trim_start_matches('/');
            format!("{}/{}", base_clean, rel_clean)
        }
    }
}

fn doc_from_fn(fn_item: &ImplItemFn) -> Option<String> {
    let docs: Vec<_> = fn_item
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| attr.parse_args::<LitStr>().ok())
        .map(|lit| lit.value().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

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
        quote! { Some(::std::borrow::Cow::Owned(String::from(#s))) }
    } else if let Some(doc) = doc_from_fn(fn_item) {
        quote! { Some(::std::borrow::Cow::Owned(String::from(#doc))) }
    } else {
        quote! { None }
    }
}

fn build_params_tokens(sig: &syn::Signature, action: &ActionArgs) -> proc_macro2::TokenStream {
    // Enforce named parameter overrides. Positional overrides removed.
    if action.param.iter().any(|p| p.name.is_none()) {
        return quote! {
            compile_error!("All #[route(param(...))] entries must provide a `name = \"...\"`; positional parameter overrides are no longer supported.");
        };
    }

    let mut overrides: std::collections::HashMap<String, &ParamArgs> = std::collections::HashMap::new();
    for spec in &action.param {
        if let Some(n) = &spec.name {
            overrides.insert(n.clone(), spec);
        }
    }

    // Collect actual parameter names from the function signature (excluding receiver)
    let mut param_names: Vec<String> = Vec::new();
    for input in sig.inputs.iter() {
        if let FnArg::Typed(pat_type) = input {
            let pname = extract_param_name(&pat_type.pat);
            param_names.push(pname);
        }
    }

    // Ensure overrides do not introduce new parameter names
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
        let lit = LitStr::new(&msg, proc_macro2::Span::call_site());
        return quote! { compile_error!(#lit); };
    }

    let mut entries: Vec<proc_macro2::TokenStream> = Vec::new();
    for input in sig.inputs.iter() {
        match input {
            FnArg::Receiver(_) => continue,
            FnArg::Typed(pat_type) => {
                let pname = extract_param_name(&pat_type.pat);
                let override_spec = overrides.get(&pname).copied();
                let meta = build_param_meta(pat_type, override_spec);
                if !meta.is_empty() {
                    entries.push(meta);
                }
            }
        }
    }

    quote! { vec![ #(#entries),* ] }
}

fn extractor_name_of_type(ty: &Type) -> Option<String> {
    let Type::Path(TypePath { path, .. }) = ty else { return None; };
    path.segments.last().map(|s| s.ident.to_string())
}

fn build_param_meta(pat_type: &PatType, override_spec: Option<&ParamArgs>) -> proc_macro2::TokenStream {
    // Determine if override hides the param
    if let Some(spec) = override_spec
        && let Some(n) = &spec.name
            && n == "_" {
                return quote! {};
            }

    let name = override_spec
        .and_then(|s| s.name.clone())
        .unwrap_or_else(|| extract_param_name(&pat_type.pat));

    // If override type provided and is `Type::Infer` (i.e. `_`), treat as explicit "no schema".
    let override_ty = override_spec.and_then(|s| s.ty.clone());

    // Determine source: explicit override takes precedence; otherwise infer from extractor.
    let source_tokens: Option<proc_macro2::TokenStream> = if let Some(spec) = override_spec {
        spec.source.as_ref().map(|src| quote! { Some(::std::borrow::Cow::Owned(String::from(#src))) })
    } else {
        None
    };

    let inferred_source = if source_tokens.is_none() {
        // try to infer from the parameter type (Path, Query, Json, etc.)
        let ty = &*pat_type.ty;
        
        extractor_name_of_type(ty).map(|s| match s.as_str() {
            "Path" => "path",
            "Query" => "query",
            "Json" => "body",
            _ => "",
        }.to_string())
    } else {
        None
    };

    let source_expr = if let Some(_) = source_tokens {
        source_tokens.unwrap()
    } else if let Some(src) = inferred_source {
        if src.is_empty() {
            quote! { None }
        } else {
            quote! { Some(::std::borrow::Cow::Owned(String::from(#src))) }
        }
    } else {
        quote! { None }
    };

    // Determine actual type for type_name and schema: prefer override when present.
    let (type_name_tokens, schema_tokens) = if let Some(ov_ty) = override_ty {
        // If override is `Type::Infer`, generate no schema expression but still show type_name as `_`.
        match ov_ty {
            syn::Type::Infer(_) => (
                quote! { ::std::borrow::Cow::Owned(String::from(stringify!(#ov_ty))) },
                quote! { None },
            ),
            _ => (
                quote! { ::std::borrow::Cow::Owned(String::from(stringify!(#ov_ty))) },
                quote! { <#ov_ty as ::uxar::views::IntoSchemaPart>::into_schema_part() },
            ),
        }
    } else {
        // No override: infer from extractor
        let ty = &pat_type.ty;
        let (inner_ty_opt, should_document) = extract_inner_type(ty);
        if !should_document {
            return quote! {};
        }

        let schema = generate_schema_call(&inner_ty_opt);
        let type_name = quote! { ::std::borrow::Cow::Owned(String::from(stringify!(#ty))) };
        (type_name, schema)
    };

    quote! {
        ::uxar::views::ParamMeta {
            name: ::std::borrow::Cow::Owned(String::from(#name)),
            type_name: #type_name_tokens,
            schema: #schema_tokens,
            source: #source_expr,
        }
    }
}

fn extract_param_name(pat: &Pat) -> String {
    match pat {
        Pat::Ident(ident) => ident.ident.to_string(),
        Pat::TupleStruct(ts) => {
            // e.g. `Path(name)` -> grab first inner binding
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

/// Extract inner type from Axum extractors
/// Returns (inner_type, should_document)
fn extract_inner_type(ty: &Type) -> (Option<Type>, bool) {
    let Type::Path(TypePath { path, .. }) = ty else {
        return (None, true);
    };

    let Some(segment) = path.segments.last() else {
        return (None, true);
    };

    let extractor_name = segment.ident.to_string();
    let inner = extract_generic_arg(&segment.arguments);

    // Determine if this should be documented
    let should_document = !matches!(
        extractor_name.as_str(),
        "State" | "Extension" | "ConnectInfo"
    );

    (inner, should_document)
}

fn extract_generic_arg(args: &PathArguments) -> Option<Type> {
    match args {
        PathArguments::AngleBracketed(angle_args) => {
            angle_args.args.first().and_then(|arg| match arg {
                GenericArgument::Type(ty) => Some(ty.clone()),
                _ => None,
            })
        }
        _ => None,
    }
}

fn generate_schema_call(inner_ty: &Option<Type>) -> proc_macro2::TokenStream {
    match inner_ty {
        Some(ty) => quote! {
            <#ty as ::uxar::views::IntoSchemaPart>::into_schema_part()
        },
        None => quote! { None },
    }
}

// No separate `return_meta` generation: responses now hold status + type/schema.

fn build_responses_tokens(sig: &syn::Signature, action: &ActionArgs) -> proc_macro2::TokenStream {
    // Compute function return type fallback
    let (func_ty, func_inner) = match &sig.output {
        syn::ReturnType::Default => {
            let unit_ty: Type = syn::parse_quote! { () };
            (unit_ty.clone(), Some(unit_ty))
        }
        syn::ReturnType::Type(_, ty) => {
            let inner = extract_inner_return_type(ty);
            ((**ty).clone(), inner)
        }
    };

    // Ensure at most one default (no-status) response
    let default_specs: Vec<_> = action.response.iter().filter(|s| s.status.is_none()).collect();
    if default_specs.len() > 1 {
        return quote! { compile_error!("Multiple #[route(response(...))] entries without `status` are not allowed (ambiguous default response)"); };
    }
    let default_spec = default_specs.first().copied();

    // Build statused entries while disallowing duplicate numeric statuses
    let mut status_entries: Vec<(u16, proc_macro2::TokenStream)> = Vec::new();
    let mut seen_statuses: std::collections::HashSet<u16> = std::collections::HashSet::new();

    for spec in &action.response {
        if let Some(status) = spec.status {
            if seen_statuses.contains(&status) {
                let msg = format!("Multiple #[route(response(status = {}))] entries for the same status are not allowed", status);
                let lit = LitStr::new(&msg, proc_macro2::Span::call_site());
                return quote! { compile_error!(#lit); };
            }
            seen_statuses.insert(status);

            // Determine type choice for this status: explicit on spec -> default_spec.ty -> func_inner -> func_ty
            let ty_choice: Option<Type> = if let Some(spec_ty) = &spec.ty {
                Some(spec_ty.clone())
            } else if let Some(def) = default_spec {
                if let Some(def_ty) = &def.ty {
                    Some(def_ty.clone())
                } else if let Some(inner) = &func_inner {
                    Some(inner.clone())
                } else {
                    Some(func_ty.clone())
                }
            } else if let Some(inner) = &func_inner {
                Some(inner.clone())
            } else {
                Some(func_ty.clone())
            };

            if let Some(ty) = ty_choice {
                let entry = match ty {
                    syn::Type::Infer(_) => quote! {
                        ::uxar::views::ReturnMeta { status: ::std::option::Option::Some(#status), type_name: ::std::borrow::Cow::Owned(String::from(stringify!(#ty))), schema: None }
                    },
                    _ => quote! {
                        ::uxar::views::ReturnMeta { status: ::std::option::Option::Some(#status), type_name: ::std::borrow::Cow::Owned(String::from(stringify!(#ty))), schema: <#ty as ::uxar::views::IntoSchemaPart>::into_schema_part() }
                    },
                };
                status_entries.push((status, entry));
            }
        }
    }

    // Build default (no-status) entry if applicable. This will use status: None
    let mut default_entry: Option<proc_macro2::TokenStream> = None;
    // Determine default type: default_spec.ty -> func_inner -> func_ty
    let default_ty_choice: Option<Type> = if let Some(def) = default_spec {
        if let Some(def_ty) = &def.ty { Some(def_ty.clone()) }
        else if let Some(inner) = &func_inner { Some(inner.clone()) }
        else { Some(func_ty.clone()) }
    } else if let Some(inner) = &func_inner {
        Some(inner.clone())
    } else {
        Some(func_ty.clone())
    };

    if let Some(ty) = default_ty_choice {
        let def_tok = match ty {
            syn::Type::Infer(_) => quote! {
                ::uxar::views::ReturnMeta { status: ::std::option::Option::None, type_name: ::std::borrow::Cow::Owned(String::from(stringify!(#ty))), schema: None }
            },
            _ => quote! {
                ::uxar::views::ReturnMeta { status: ::std::option::Option::None, type_name: ::std::borrow::Cow::Owned(String::from(stringify!(#ty))), schema: <#ty as ::uxar::views::IntoSchemaPart>::into_schema_part() }
            },
        };
        default_entry = Some(def_tok);
    }

    // Sort numeric statuses and collect tokens
    status_entries.sort_by_key(|(s, _)| *s);
    let mut entries: Vec<proc_macro2::TokenStream> = status_entries.into_iter().map(|(_, v)| v).collect();
    if let Some(d) = default_entry {
        entries.push(d);
    }

    quote! { ::std::vec![ #(#entries),* ] }
}

/// Extract inner type from response wrappers like Result<Json<T>, E> -> T
fn extract_inner_return_type(ty: &Type) -> Option<Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };

    let Some(segment) = path.segments.last() else {
        return None;
    };

    let type_name = segment.ident.to_string();

    // Handle Result<T, E> -> extract T
    if type_name == "Result"
        && let PathArguments::AngleBracketed(args) = &segment.arguments
            && let Some(GenericArgument::Type(ok_ty)) = args.args.first() {
                // Recursively extract from the Ok type
                return extract_inner_return_type(ok_ty).or(Some(ok_ty.clone()));
            }

    // Handle response wrappers: Json<T>, Html, Response, etc.
    extract_generic_arg(&segment.arguments)
}


#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    #[test]
    fn duplicate_status_is_compile_error() {
        // Build a fake signature `fn foo() -> i32`
        let sig: syn::Signature = parse_str("fn foo() -> i32").expect("parse sig");

        // Build an action with duplicate status entries
        let spec1 = ResponseSpec { status: Some(200), ty: None };
        let spec2 = ResponseSpec { status: Some(200), ty: None };
        let action = ActionArgs {
            name: None,
            methods: vec![],
            path: None,
            summary: None,
            param: vec![],
            response: vec![spec1, spec2],
        };

        let tokens = build_responses_tokens(&sig, &action);
        let s = tokens.to_string();
        assert!(s.contains("compile_error"), "expected compile_error for duplicate statuses; got: {}", s);
    }

    #[test]
    fn default_response_emits_none_status() {
        // signature with return type i32
        let sig: syn::Signature = parse_str("fn foo() -> i32").expect("parse sig");

        // default response (no status)
        let spec = ResponseSpec { status: None, ty: None };
        let action = ActionArgs {
            name: None,
            methods: vec![],
            path: None,
            summary: None,
            param: vec![],
            response: vec![spec],
        };

        let tokens = build_responses_tokens(&sig, &action);
        let s = tokens.to_string();
        // should indicate an Option::None for status
        assert!(s.contains("status") && s.contains("Option") && s.contains("None"), "expected default None status in tokens: {}", s);
    }
}
