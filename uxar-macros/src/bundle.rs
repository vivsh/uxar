use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, Path, Token};
use syn::spanned::Spanned;

/// Parse bundle! macro
/// 
/// Usage:
/// ```
/// bundle! {
///     tags = ["users", "api"],
///     hello,
///     create_user,
/// }
/// ```
pub(crate) fn parse_bundle(input: TokenStream) -> TokenStream {
    let input_parsed = parse_macro_input!(input as BundleInput);
    
    let handlers = input_parsed.handlers;
    let tags = input_parsed.tags;
    
    if handlers.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "bundle! requires at least one handler"
        )
        .to_compile_error()
        .into();
    }
    
    // Generate code for each handler
    let route_setup: Vec<_> = handlers.iter().map(|handler| {
        // Extract the final identifier from the path for the metadata function name
        let fn_name = handler.segments.last()
            .expect("Path must have at least one segment")
            .ident.to_string();
        let meta_fn = Ident::new(&format!("__route_meta_{}", fn_name), handler.span());
        
        quote! {
            bundle = bundle.route(#handler, #meta_fn());
        }
    }).collect();
    
    let expanded = if tags.is_empty() {
        quote! {
            {
                let mut bundle = ::uxar::bundles::Bundle::new();
                
                #(#route_setup)*
                
                bundle
            }
        }
    } else {
        quote! {
            {
                let mut bundle = ::uxar::bundles::Bundle::new();
                
                #(#route_setup)*
                
                bundle.with_tags([#(#tags),*])
            }
        }
    };
    
    expanded.into()
}

/// Attribute macro version for impl blocks
/// 
/// Usage:
/// ```
/// #[bundle(tags = ["api", "v1"])]
/// impl MyApi {
///     #[route(...)]
///     async fn handler1() { }
/// }
/// ```
pub(crate) fn parse_bundle_attr(attr: TokenStream, item: TokenStream) -> TokenStream {
    use syn::{ItemImpl, ImplItem};
    
    let item_impl = parse_macro_input!(item as ItemImpl);
    let self_ty = &item_impl.self_ty;
    
    // Parse tags from attribute
    let tags = if !attr.is_empty() {
        match parse_tags_attr(attr) {
            Ok(t) => t,
            Err(e) => return e.to_compile_error().into(),
        }
    } else {
        Vec::new()
    };
    
    // Collect all methods with #[route] attribute
    let mut methods = Vec::new();
    for impl_item in &item_impl.items {
        if let ImplItem::Fn(method) = impl_item {
            // Check if method has #[route] attribute
            if method.attrs.iter().any(|attr| {
                attr.path().is_ident("route")
            }) {
                methods.push(&method.sig.ident);
            }
        }
    }
    
    if methods.is_empty() {
        return syn::Error::new_spanned(
            &item_impl.self_ty,
            "No methods with #[route] attribute found in impl block"
        )
        .to_compile_error()
        .into();
    }
    
    // Generate metadata collection code
    let route_setup: Vec<_> = methods.iter().map(|method_name| {
        let meta_fn = Ident::new(&format!("__route_meta_{}", method_name), method_name.span());
        
        quote! {
            bundle = bundle.route(Self::#method_name, Self::#meta_fn());
        }
    }).collect::<Vec<_>>();
    
    let bundle_result = if tags.is_empty() {
        quote! { bundle }
    } else {
        quote! { bundle.with_tags([#(#tags),*]) }
    };
    
    let expanded = quote! {
        #item_impl
        
        impl ::uxar::bundles::IntoBundle for #self_ty {
            fn into_bundle(self) -> ::uxar::bundles::Bundle {
                let mut bundle = ::uxar::bundles::Bundle::new();
                
                #(#route_setup)*
                
                #bundle_result
            }
        }
    };
    
    expanded.into()
}

fn parse_tags_attr(attr: TokenStream) -> syn::Result<Vec<String>> {
    use syn::parse::{Parse, ParseStream};
    
    struct TagsAttr {
        tags: Vec<String>,
    }
    
    impl Parse for TagsAttr {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            let ident: Ident = input.parse()?;
            if ident != "tags" {
                return Err(syn::Error::new(ident.span(), "expected 'tags'"));
            }
            
            input.parse::<Token![=]>()?;
            
            let content;
            syn::bracketed!(content in input);
            
            let mut tags = Vec::new();
            while !content.is_empty() {
                let tag: syn::LitStr = content.parse()?;
                tags.push(tag.value());
                
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
            
            Ok(Self { tags })
        }
    }
    
    let parsed = syn::parse::<TagsAttr>(attr)?;
    Ok(parsed.tags)
}

/// Input structure for parsing bundle! macro
struct BundleInput {
    tags: Vec<String>,
    handlers: Vec<Path>,
}

impl syn::parse::Parse for BundleInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut tags = Vec::new();
        let mut handlers = Vec::new();
        
        // Try to parse optional tags first
        if input.peek(syn::Ident) && input.peek2(Token![=]) {
            let ident: Ident = input.parse()?;
            if ident == "tags" {
                input.parse::<Token![=]>()?;
                
                // Parse array literal [...]
                let content;
                syn::bracketed!(content in input);
                
                while !content.is_empty() {
                    let tag: syn::LitStr = content.parse()?;
                    tags.push(tag.value());
                    
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
                
                // Expect comma after tags array
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
            } else {
                return Err(syn::Error::new(ident.span(), "expected 'tags' or handler identifier"));
            }
        }
        
        // Parse handlers
        while !input.is_empty() {
            handlers.push(input.parse::<Path>()?);
            
            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        
        Ok(Self { tags, handlers })
    }
}

