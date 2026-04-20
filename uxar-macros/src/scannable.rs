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

    // Support both internal (uxar crate) and external usage
    let crate_path = get_crate_path();

    gen_where_clause(&mut generics, &parsed.fields, &crate_path);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_inits = gen_field_initializers(&parsed.fields, &crate_path);
    let field_inits_unordered = gen_field_initializers_unordered(&parsed.fields, &crate_path);
    let column_names = gen_scan_column_names(&parsed.fields, &crate_path);

    quote! {
        impl #impl_generics #crate_path::db::Scannable for #ident #ty_generics #where_clause {
            fn scan_column_names() -> Vec<String> {
                let mut cols = Vec::new();
                #(#column_names)*
                cols
            }

            fn scan_row_ordered(
                row: &#crate_path::db::Row,
                start_idx: &mut usize,
            ) -> Result<Self, ::sqlx::Error> {
                use ::sqlx::Row as _;
                Ok(Self {
                    #(#field_inits)*
                })
            }

            fn scan_row_unordered(
                row: &#crate_path::db::Row,
            ) -> Result<Self, ::sqlx::Error> {
                use ::sqlx::Row as _;
                Ok(Self {
                    #(#field_inits_unordered)*
                })
            }
        }

        impl #impl_generics ::sqlx::FromRow<'_, #crate_path::db::Row> for #ident #ty_generics #where_clause {
            fn from_row(row: &#crate_path::db::Row) -> Result<Self, ::sqlx::Error> {
                <Self as #crate_path::db::Scannable>::scan_row(row)
            }
        }
    }
}

/// Determine the correct crate path for generated code
fn get_crate_path() -> proc_macro2::TokenStream {
    if std::env::var("CARGO_CRATE_NAME").as_deref() == Ok("uxar") {
        quote! { crate }
    } else {
        quote! { ::uxar }
    }
}

/// Generate where clause predicates for Scannable trait bounds.
fn gen_where_clause(generics: &mut syn::Generics, fields: &[FieldMeta], crate_path: &proc_macro2::TokenStream) {
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

        if is_flatten(field) || is_reference(field) {
            if seen.insert(ty_str) {
                wc.predicates.push(syn::parse_quote! {
                    #ty: #crate_path::db::Scannable
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
}

/// Generate field initializers for struct construction.
fn gen_field_initializers(fields: &[FieldMeta], crate_path: &proc_macro2::TokenStream) -> Vec<proc_macro2::TokenStream> {
    let mut inits = Vec::with_capacity(fields.len());

    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };

        let init = if is_skip(field) || !is_selectable(field) {
            gen_default_init(ident)
        } else if is_flatten(field) || is_reference(field) {
            gen_flatten_init(ident, &field.ty, crate_path)
        } else if is_json(field) {
            gen_json_init(ident, crate_path)
        } else {
            gen_scalar_init(ident)
        };

        inits.push(init);
    }

    inits
}

/// Generate field initializers for unordered (name-based) struct construction.
fn gen_field_initializers_unordered(fields: &[FieldMeta], crate_path: &proc_macro2::TokenStream) -> Vec<proc_macro2::TokenStream> {
    let mut inits = Vec::with_capacity(fields.len());

    for field in fields {
        let Some(ident) = &field.ident else {
            continue;
        };

        let init = if is_skip(field) || !is_selectable(field) {
            gen_default_init(ident)
        } else if is_reference(field) {
            // Reference fields cannot be scanned unordered - they need prefixed column names
            gen_reference_unordered_error(ident, &field.ty)
        } else if is_flatten(field) {
            gen_flatten_init_unordered(ident, &field.ty, crate_path)
        } else if is_json(field) {
            gen_json_init_unordered(ident, field, crate_path)
        } else {
            gen_scalar_init_unordered(ident, field)
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

/// Generate compile error for reference field in unordered scan.
fn gen_reference_unordered_error(ident: &syn::Ident, ty: &Type) -> proc_macro2::TokenStream {
    let error_msg = format!(
        "Cannot use scan_row_unordered with reference field '{}' of type '{}'. \
        Reference fields require ordered scanning (scan_row_ordered) because they use \
        prefixed column names. Use scan_row_ordered or scan_row instead.",
        ident, quote::quote!(#ty)
    );
    quote! {
        #ident: unimplemented!(#error_msg),
    }
}

/// Generate initialization for flattened field.
fn gen_flatten_init(ident: &syn::Ident, ty: &Type, crate_path: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        #ident: <#ty as #crate_path::db::Scannable>::scan_row_ordered(row, start_idx)?,
    }
}

/// Generate initialization for JSON-deserialized field.
fn gen_json_init(ident: &syn::Ident, crate_path: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        #ident: {
            let json_val: ::serde_json::Value = row.try_get(*start_idx)?;
            *start_idx += 1;
            ::serde_json::from_value(json_val)
                .map_err(|e| ::sqlx::Error::Decode(Box::new(e)))?
        },
    }
}

/// Generate initialization for scalar field.
fn gen_scalar_init(ident: &syn::Ident) -> proc_macro2::TokenStream {
    quote! {
        #ident: {
            let val = row.try_get(*start_idx)?;
            *start_idx += 1;
            val
        },
    }
}

/// Check if field should be skipped (column only).
fn is_skip(field: &FieldMeta) -> bool {
    field.column.skip
}

/// Check if field should be flattened (column only).
fn is_flatten(field: &FieldMeta) -> bool {
    field.column.flatten
}

/// Check if field is a reference (column only).
fn is_reference(field: &FieldMeta) -> bool {
    field.column.reference.is_some()
}

/// Check if field should be JSON-serialized (column only).
fn is_json(field: &FieldMeta) -> bool {
    field.column.json
}

/// Check if field is selectable (column attr only, None means true).
fn is_selectable(field: &FieldMeta) -> bool {
    field.column.selectable.unwrap_or(true)
}

/// Generate the scan_column_names implementation.
fn gen_scan_column_names(fields: &[FieldMeta], crate_path: &proc_macro2::TokenStream) -> Vec<proc_macro2::TokenStream> {
    let mut stmts = Vec::new();

    for field in fields {
        if is_skip(field) || !is_selectable(field) {
            continue;
        }

        if is_reference(field) {
            let ty = &field.ty;
            let field_name = field.column.name.as_ref()
                .map(|lit| lit.value())
                .or_else(|| field.ident.as_ref().map(|i| i.to_string()))
                .unwrap_or_default();
            
            stmts.push(quote! {
                {
                    let nested_cols = <#ty as #crate_path::db::Scannable>::scan_column_names();
                    for col in nested_cols {
                        cols.push(format!("{}.{}", #field_name, col));
                    }
                }
            });
        } else if is_flatten(field) {
            let ty = &field.ty;
            stmts.push(quote! {
                cols.extend(<#ty as #crate_path::db::Scannable>::scan_column_names());
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

/// Generate initialization for flattened field (unordered).
fn gen_flatten_init_unordered(ident: &syn::Ident, ty: &Type, crate_path: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    quote! {
        #ident: <#ty as #crate_path::db::Scannable>::scan_row_unordered(row)?,
    }
}

/// Generate initialization for JSON-deserialized field (unordered).
fn gen_json_init_unordered(ident: &syn::Ident, field: &FieldMeta, crate_path: &proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let col_name = field.column.name.as_ref()
        .map(|lit| lit.value())
        .unwrap_or_else(|| ident.to_string());
    
    quote! {
        #ident: {
            let json_val: ::serde_json::Value = row.try_get(#col_name)?;
            ::serde_json::from_value(json_val)
                .map_err(|e| ::sqlx::Error::Decode(Box::new(e)))?
        },
    }
}

/// Generate initialization for scalar field (unordered).
fn gen_scalar_init_unordered(ident: &syn::Ident, field: &FieldMeta) -> proc_macro2::TokenStream {
    let col_name = field.column.name.as_ref()
        .map(|lit| lit.value())
        .unwrap_or_else(|| ident.to_string());
    
    quote! {
        #ident: row.try_get(#col_name)?,
    }
}
