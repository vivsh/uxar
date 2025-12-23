
mod schemable;
mod scannable;
mod validatable;
mod bindable;
mod filterable;
mod routable;
mod model;

use proc_macro::TokenStream;
extern crate proc_macro;


/// Derives database schema traits for a type.
/// 
/// Implements: SchemaInfo, Scannable, Bindable, and Validatable traits.
/// 
/// Use `#[derive(Validatable)]` if you only need validation without database operations.
#[proc_macro_derive(Schemable, attributes(column, validate, schemable))]
pub fn derive_schemable(input: TokenStream) -> TokenStream {
    model::derive_model(input)
}


#[proc_macro_derive(Filterable, attributes(filterable, filter))]
pub fn derive_filterable(input: TokenStream) -> TokenStream {
    filterable::derive_filterable(input)
}


/// Derives validation trait for a type.
/// 
/// Use this for types that need validation but don't interact with the database.
/// Database models should use `#[derive(Schemable)]` which includes validation.
#[proc_macro_derive(Validatable, attributes(validate))]
pub fn derive_validatable(input: TokenStream) -> TokenStream {
    validatable::derive_validatable(input)
}


#[proc_macro_attribute]
pub fn route(attr: TokenStream, item: TokenStream) -> TokenStream {
    routable::parse_action(attr, item)
}

#[proc_macro_attribute]
pub fn routable(attr: TokenStream, item: TokenStream) -> TokenStream {
    routable::parse_routable(attr, item)
}