use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, Path, Token};
use syn::spanned::Spanned;


pub(crate) fn parse_bundle(input: TokenStream) -> TokenStream {
    let input_parsed = parse_macro_input!(input as BundleInput);
    
    let handlers = input_parsed.handlers;
    
    let bundle_part_setup: Vec<_> = handlers.iter().map(|handler| {

        let fn_name = handler.segments.last()
            .expect("Path must have at least one segment")
            .ident.to_string();

        let bundle_part_fn = Ident::new(
            &format!("__bundle_part_{}", fn_name), 
            handler.span()
        );
        
        quote! {
            #bundle_part_fn(),
        }

    }).collect();
    
    let expanded = quote! {
        {
            ::uxar::bundles::bundle([
                #(#bundle_part_setup)*
            ])
        }
    };
    
    expanded.into()
}


struct BundleInput {
    handlers: Vec<Path>,
}

impl syn::parse::Parse for BundleInput {

    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut handlers = Vec::new();
         
        // Parse handlers
        while !input.is_empty() {
            handlers.push(input.parse::<Path>()?);
            
            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        
        Ok(Self { handlers })
    }

}

