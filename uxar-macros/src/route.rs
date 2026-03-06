//! Route macro implementation using bundlepart infrastructure.
//!
//! Provides #[route] attribute macro for HTTP route handlers.
//! Delegates to bundlepart.rs for consistent code generation.

use darling::FromMeta;
use proc_macro::TokenStream;
use quote::quote;

use crate::bundlepart::{self, FnSpec};

/// Route configuration metadata.
///
/// Maps to uxar::routes::RouteConf runtime structure.
#[derive(Debug, FromMeta, Default)]
struct RouteConfMeta {
    name: Option<String>,

    /// HTTP method(s); default: GET
    #[darling(default, multiple, rename = "method")]
    methods: Vec<String>,

    /// URL path; default: "/{name}"
    #[darling(rename = "path")]
    path: String,
}

/// Entry point for #[route] macro.
///
/// Handles both free functions and methods in impl blocks.
pub(crate) fn parse_route(attr: TokenStream, item: TokenStream) -> TokenStream {
    bundlepart::generate_bundle_part::<RouteConfMeta>(
        attr,
        item,
        "route",
        build_route_conf,
    )
}

/// Build RouteConf from parsed metadata and function spec.
///
/// Validates and normalizes configuration, generates RouteConf construction.
fn build_route_conf(
    conf: &RouteConfMeta,
    spec: &FnSpec,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let path = &conf.path;
    validate_path(path)?;
    
    let methods = normalize_methods(&conf.methods);
    for method in &methods {
        validate_method(method)?;
    }
    
    let method_filter = build_method_filter(&methods);
    let name = &conf.name.as_deref().unwrap_or(&spec.name);

    Ok(quote! {
        ::uxar::routes::RouteConf {
            name: ::std::borrow::Cow::Borrowed(#name),
            methods: #method_filter,
            path: ::std::borrow::Cow::Borrowed(#path),
        }
    })
}

/// Normalize method list (uppercase, default to GET).
fn normalize_methods(methods: &[String]) -> Vec<String> {
    if methods.is_empty() {
        vec!["GET".to_string()]
    } else {
        methods.iter().map(|m| m.to_uppercase()).collect()
    }
}

/// Validate URL path format.
fn validate_path(path: &str) -> Result<(), syn::Error> {
    if path.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Route path cannot be empty"
        ));
    }
    
    if !path.starts_with('/') {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("Route path must start with '/'. Found: '{}'", path)
        ));
    }
    
    if path.contains("//") {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("Route path contains double slashes: '{}'", path)
        ));
    }
    
    Ok(())
}

/// Validate HTTP method.
fn validate_method(method: &str) -> Result<(), syn::Error> {
    const VALID: &[&str] = &[
        "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"
    ];
    
    if !VALID.contains(&method) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!(
                "Invalid HTTP method '{}'. Supported: {}",
                method,
                VALID.join(", ")
            )
        ));
    }
    
    Ok(())
}

/// Build Methods filter expression from method list.
fn build_method_filter(methods: &[String]) -> proc_macro2::TokenStream {
    let filters: Vec<_> = methods.iter()
        .map(|m| method_to_const(m.as_str()))
        .collect();
    
    if filters.len() == 1 {
        filters[0].clone()
    } else {
        let first = &filters[0];
        let rest = &filters[1..];
        quote! { #first #(| #rest)* }
    }
}

/// Convert method string to Methods constant.
fn method_to_const(method: &str) -> proc_macro2::TokenStream {
    match method {
        "GET" => quote! { ::uxar::routes::Methods::GET },
        "POST" => quote! { ::uxar::routes::Methods::POST },
        "PUT" => quote! { ::uxar::routes::Methods::PUT },
        "DELETE" => quote! { ::uxar::routes::Methods::DELETE },
        "PATCH" => quote! { ::uxar::routes::Methods::PATCH },
        "OPTIONS" => quote! { ::uxar::routes::Methods::OPTIONS },
        "HEAD" => quote! { ::uxar::routes::Methods::HEAD },
        _ => panic!(
            "Invalid method '{}' - should have been caught by validation",
            method
        ),
    }
}
