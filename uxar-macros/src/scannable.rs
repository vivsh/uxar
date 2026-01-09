use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Type};
use std::collections::HashSet;

use crate::schemable::{FieldMeta, ParsedStruct};

/// Derives the Scannable trait for deserializing database rows into structs.
pub fn derive_scannable(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    derive_scannable_impl(&input).into()
}

/// Internal implementation of Scannable derive macro.
pub(crate) fn derive_scannable_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let parsed = match ParsedStruct::from_derive_input(input.clone()) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error(),
    };

    let ident = &parsed.ident;
    let mut generics = parsed.generics.clone();

    gen_where_clause(&mut generics, &parsed.fields);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_inits = gen_field_initializers(&parsed.fields);

    quote! {
        impl #impl_generics ::uxar::db::Scannable for #ident #ty_generics #where_clause {
            fn scan_row_ordered(
                row: &::uxar::db::PgRow,
                start_idx: &mut usize,
            ) -> Result<Self, ::uxar::db::SqlxError> {
                Ok(Self {
                    #(#field_inits)*
                })
            }
        }

        impl #impl_generics ::uxar::db::FromRow<'_, ::uxar::db::PgRow> for #ident #ty_generics #where_clause {
            fn from_row(row: &::uxar::db::PgRow) -> Result<Self, ::uxar::db::SqlxError> {
                <Self as ::uxar::db::Scannable>::scan_row(row)
            }
        }
    }
}

/// Generate where clause predicates for Scannable trait bounds.
fn gen_where_clause(generics: &mut syn::Generics, fields: &[FieldMeta]) {
    let mut seen = HashSet::new();
    let wc = generics.where_clause.get_or_insert(syn::WhereClause {
        where_token: <syn::Token![where]>::default(),
        predicates: syn::punctuated::Punctuated::new(),
    });

    for field in fields {
        if is_skip(field) || !is_selectable(field) {
            continue;
        }

        let ty = &field.ty;
        let ty_str = quote::quote!(#ty).to_string();

        if is_flatten(field) {
            if seen.insert(ty_str) {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::uxar::db::Scannable
                });
            }
        } else if is_json(field) {
            if seen.insert(ty_str) {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::serde::de::DeserializeOwned
                });
            }
        }
    }

    for field in fields {
        if is_skip(field) || is_selectable(field) {
            continue;
        }

        let ty = &field.ty;
        let ty_str = quote::quote!(#ty).to_string();
        if seen.insert(ty_str) {
            wc.predicates.push(syn::parse_quote! {
                #ty: ::core::default::Default
            });
        }
    }
}

/// Generate field initializers for struct construction.
fn gen_field_initializers(fields: &[FieldMeta]) -> Vec<proc_macro2::TokenStream> {
    let mut inits = Vec::with_capacity(fields.len());

    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };

        let init = if is_skip(field) || !is_selectable(field) {
            gen_default_init(ident)
        } else if is_flatten(field) {
            gen_flatten_init(ident, &field.ty)
        } else if is_json(field) {
            gen_json_init(ident)
        } else {
            gen_scalar_init(ident)
        };

        inits.push(init);
    }

    inits
}

/// Generate default initialization for non-selectable field.
fn gen_default_init(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        #ident: ::core::default::Default::default(),
    }
}

/// Generate initialization for flattened field.
fn gen_flatten_init(ident: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
    quote! {
        #ident: <#ty as ::uxar::db::Scannable>::scan_row_ordered(row, start_idx)?,
    }
}

/// Generate initialization for JSON-deserialized field.
fn gen_json_init(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        #ident: {
            let json_val: ::uxar::db::serde_json::Value = ::uxar::db::Row::try_get(row, *start_idx)?;
            *start_idx += 1;
            ::uxar::db::serde_json::from_value(json_val)
                .map_err(|e| ::uxar::db::SqlxError::Decode(Box::new(e)))?
        },
    }
}

/// Generate initialization for scalar field.
fn gen_scalar_init(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        #ident: {
            let val = ::uxar::db::Row::try_get(row, *start_idx)?;
            *start_idx += 1;
            val
        },
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

/// Check if field is selectable (column attr only, None means true).
fn is_selectable(field: &FieldMeta) -> bool {
    field.column.selectable.unwrap_or(true)
}
