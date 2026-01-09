use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::schemable::ParsedStruct;
use crate::bindable::derive_bindable_impl;
use crate::scannable::derive_scannable_impl;

/// Derives Model trait, which combines Schemable, Validate, Bindable, and Scannable.
/// 
/// This is a convenience macro that generates all four trait implementations
/// using the shared ParsedStruct parser from schemable.rs.
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input_clone = input.clone();
    let input_ast = syn::parse_macro_input!(input as DeriveInput);
    
    // Generate each trait implementation using existing internal functions
    let schemable_impl = crate::schemable::derive_schemable_impl(input_clone.clone());
    let validate_impl = crate::validate::derive_validate_impl(input_clone);
    let bindable_impl = crate::bindable::derive_bindable_impl(&input_ast);
    let scannable_impl = crate::scannable::derive_scannable_impl(&input_ast);
    let model_impl = gen_model_impl(&input_ast);
    
    // Parse the token streams back to proc_macro2::TokenStream for combination
    let schemable_tokens: proc_macro2::TokenStream = schemable_impl.into();
    let validate_tokens: proc_macro2::TokenStream = validate_impl.into();
    
    // Combine all implementations
    let combined = quote! {
        #schemable_tokens
        #validate_tokens
        #bindable_impl
        #scannable_impl
        #model_impl
    };
    
    combined.into()
}

/// Generate Model trait implementation with OnceLock for static schema.
fn gen_model_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    
    quote! {
        impl #impl_generics ::uxar::db::Model for #ident #ty_generics #where_clause {
            fn model_schema() -> &'static ::uxar::schemables::StructSchema {
                static SCHEMA: ::std::sync::OnceLock<::uxar::schemables::StructSchema> = ::std::sync::OnceLock::new();
                SCHEMA.get_or_init(|| {
                    match <Self as ::uxar::schemables::Schemable>::schema_type() {
                        ::uxar::schemables::SchemaType::Struct(schema) => schema,
                        _ => panic!("Model can only be derived for structs"),
                    }
                })
            }
        }
    }
}
