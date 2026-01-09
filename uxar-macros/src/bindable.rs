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

    quote! {
        impl #impl_generics ::uxar::db::Bindable for #ident #ty_generics #where_clause {
            fn bind_values(
                &self,
                args: &mut ::uxar::db::PgArguments,
            ) -> Result<(), ::uxar::db::SqlxError> {
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
                for<'q> &'q #ty: ::uxar::db::sqlx::Encode<'q, ::uxar::db::Postgres>
                    + ::uxar::db::sqlx::Type<::uxar::db::Postgres>
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
                .map_err(|e| ::uxar::db::SqlxError::Decode(Box::new(e)))?;
            <::uxar::db::PgArguments as ::uxar::db::Arguments<'_>>::add(args, value)
                .map_err(::uxar::db::SqlxError::Decode)?;
        }
    }
}

/// Generate bind statement for scalar field.
fn gen_scalar_bind(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        {
            <::uxar::db::PgArguments as ::uxar::db::Arguments<'_>>::add(args, &self.#ident)
                .map_err(::uxar::db::SqlxError::Decode)?;
        }
    }
}

/// Check if field should be skipped (column takes precedence).
fn is_skip(field: &FieldMeta) -> bool {
    field.column.skip || field.field.skip
}

/// Check if field should be flattened (column takes precedence).
fn is_flatten(field: &FieldMeta) -> bool {
    field.column.flatten || field.field.flatten
}

/// Check if field should be JSON-serialized (column takes precedence).
fn is_json(field: &FieldMeta) -> bool {
    field.column.json || field.field.json
}
