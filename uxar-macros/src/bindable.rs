use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Type};
use std::collections::HashSet;

use crate::schemable::{FieldMeta, ParsedStruct};

/// Derives the Bindable trait for binding struct fields to SQL parameters.
pub fn derive_bindable(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    derive_bindable_impl(&input).into()
}

/// Internal implementation of Bindable derive macro.
pub(crate) fn derive_bindable_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let parsed = match ParsedStruct::from_derive_input(input.clone()) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };

    let ident = &parsed.ident;
    let mut generics = parsed.generics.clone();

    gen_where_clause(&mut generics, &parsed.fields);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let bind_stmts = gen_bind_statements(&parsed.fields);
    let column_names = gen_bind_column_names(&parsed.fields);

    quote! {
        impl #impl_generics ::uxar::db::Bindable for #ident #ty_generics #where_clause {
            fn bind_column_names() -> Vec<String> {
                let mut cols = Vec::new();
                #(#column_names)*
                cols
            }

            fn bind_values<'q>(
                &'q self,
                args: &mut ::uxar::db::Arguments<'q>,
            ) -> Result<(), ::sqlx::Error> {
                #(#bind_stmts)*
                Ok(())
            }
        }
    }
}

/// Generate where clause predicates for Bindable trait bounds.
fn gen_where_clause(generics: &mut syn::Generics, fields: &[FieldMeta]) {
    let mut seen = HashSet::new();
    let wc = generics.where_clause.get_or_insert(syn::WhereClause {
        where_token: <syn::Token![where]>::default(),
        predicates: syn::punctuated::Punctuated::new(),
    });

    for field in fields {
        if is_skip(field) {
            continue;
        }

        let ty = &field.ty;
        let ty_str = quote::quote!(#ty).to_string();

        if is_flatten(field) {
            if seen.insert(ty_str) {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::uxar::db::Bindable
                });
            }
        } else if is_json(field) {
            if seen.insert(ty_str) {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::serde::Serialize
                });
            }
        } else {
            wc.predicates.push(syn::parse_quote! {
                for<'q> &'q #ty: ::sqlx::Encode<'q, ::uxar::db::Database>
                    + ::sqlx::Type<::uxar::db::Database>
                    + ::core::marker::Send
            });
        }
    }
}

/// Generate bind statements for each field.
fn gen_bind_statements(fields: &[FieldMeta]) -> Vec<proc_macro2::TokenStream> {
    let mut stmts = Vec::with_capacity(fields.len());

    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };

        if is_skip(field) {
            continue;
        }

        let stmt = if is_flatten(field) {
            gen_flatten_bind(ident, &field.ty)
        } else if is_json(field) {
            gen_json_bind(ident)
        } else {
            gen_scalar_bind(ident)
        };

        stmts.push(stmt);
    }

    stmts
}

/// Generate bind statement for flattened field.
fn gen_flatten_bind(ident: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
    quote! {
        <#ty as ::uxar::db::Bindable>::bind_values(&self.#ident, args)?;
    }
}

/// Generate bind statement for JSON-serialized field.
fn gen_json_bind(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        {
            let value = ::uxar::db::serde_json::to_value(&self.#ident)
                .map_err(|e| ::sqlx::Error::Decode(Box::new(e)))?;
            ::sqlx::Arguments::add(args, value)
                .map_err(::sqlx::Error::Decode)?;;
        }
    }
}

/// Generate bind statement for scalar field.
fn gen_scalar_bind(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        {
            ::sqlx::Arguments::add(args, &self.#ident)
                .map_err(::sqlx::Error::Decode)?;
        }
    }
}

/// Check if field should be skipped (column only).
fn is_skip(field: &FieldMeta) -> bool {
    field.column.skip || is_reference(field)
}

/// Check if field is a reference to another model.
fn is_reference(field: &FieldMeta) -> bool {
    field.column.reference.is_some()
}

/// Check if field should be flattened (column only).
fn is_flatten(field: &FieldMeta) -> bool {
    field.column.flatten
}

/// Check if field should be JSON-serialized (column only).
fn is_json(field: &FieldMeta) -> bool {
    field.column.json
}

/// Generate the bind_column_names implementation.
fn gen_bind_column_names(fields: &[FieldMeta]) -> Vec<proc_macro2::TokenStream> {
    let mut stmts = Vec::new();

    for field in fields {
        if is_skip(field) {
            continue;
        }

        if is_flatten(field) {
            let ty = &field.ty;
            stmts.push(quote! {
                cols.extend(<#ty as ::uxar::db::Bindable>::bind_column_names());
            });
        } else {
            let col_name = field.column.name.as_ref()
                .map(|lit| lit.value())
                .or_else(|| field.ident.as_ref().map(|i| i.to_string()))
                .unwrap_or_default();
            stmts.push(quote! {
                cols.push(#col_name.to_string());
            });
        }
    }

    stmts
}
