use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Derives Model which combines Schemable, Scannable, Bindable, and Validatable
/// 
/// Usage:
/// ```ignore
/// #[derive(Model)]
/// #[model(crate = "uxar::db")]
/// struct User {
///     #[column(db_column = "user_email")]
///     #[validate(email)]
///     email: String,
/// }
/// ```
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    // Transform model attribute to schemable attribute for Schemable implementation
    let mut input_for_schemable = input.clone();
    transform_model_to_schemable_attr(&mut input_for_schemable);
    
    // Generate implementations for all four traits by calling their respective derive functions
    let schemable_impl = crate::schemable::derive_schemable_impl(&input_for_schemable);
    let scannable_impl = crate::scannable::derive_scannable_impl(&input);
    let bindable_impl = crate::bindable::derive_bindable_impl(&input);
    let validatable_impl = crate::validatable::derive_validatable_impl(&input);
    
    let expanded = quote! {
        #schemable_impl
        #scannable_impl
        #bindable_impl
        #validatable_impl
    };
    
    TokenStream::from(expanded)
}

/// Transform `#[model(...)]` attributes to `#[schemable(...)]` for the Schemable macro
fn transform_model_to_schemable_attr(input: &mut DeriveInput) {
    for attr in &mut input.attrs {
        if attr.path().is_ident("model") {
            // Parse and reconstruct the attribute with schemable path
            if let Ok(meta) = attr.meta.require_list() {
                let tokens = &meta.tokens;
                *attr = syn::parse_quote! {
                    #[schemable(#tokens)]
                };
            }
        }
    }
}
