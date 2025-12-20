
mod schemable;
mod scannable;
mod validatable;
mod bindable;
mod filterable;
mod recordable;
mod viewable;

use proc_macro::TokenStream;
extern crate proc_macro;


#[proc_macro_derive(Schemable, attributes(column))]
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


#[proc_macro_attribute]
pub fn action(attr: TokenStream, item: TokenStream) -> TokenStream {
    viewable::parse_action(attr, item)
}

#[proc_macro_attribute]
pub fn viewable(attr: TokenStream, item: TokenStream) -> TokenStream {
    viewable::parse_viewable(attr, item)
}