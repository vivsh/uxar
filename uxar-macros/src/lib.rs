
mod schemable;
mod scannable;
mod validatable;
mod bindable;
mod filterable;
mod recordable;
mod routable;

use proc_macro::TokenStream;
extern crate proc_macro;


#[proc_macro_derive(Schemable, attributes(column, validate, schemable))]
pub fn derive_schemable(input: TokenStream) -> TokenStream {
    schemable::derive_schemable(input)
}


#[proc_macro_derive(Scannable, attributes(column))]
pub fn derive_scannable(input: TokenStream) -> TokenStream {
    scannable::derive_scannable(input)
}


#[proc_macro_derive(Bindable, attributes(column))]
pub fn derive_bindable(input: TokenStream) -> TokenStream {
    bindable::derive_bindable(input)
}


#[proc_macro_derive(Filterable, attributes(filterable, filter))]
pub fn derive_filterable(input: TokenStream) -> TokenStream {
    filterable::derive_filterable(input)
}


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