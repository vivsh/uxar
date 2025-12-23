use std::collections::HashSet;

use darling::FromDeriveInput;
use darling::ast::{self, Data};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

use crate::schemable::SchemableField;

#[derive(FromDeriveInput)]
pub(crate) struct ScannableInput {
    ident: syn::Ident,
    generics: syn::Generics,

    #[darling(default, rename = "crate")]
    #[allow(dead_code)]
    crate_path: Option<syn::Path>,

    data: Data<darling::util::Ignored, SchemableField>,
}

#[allow(dead_code)]
pub fn derive_scannable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_scannable_impl(&input).into()
}

pub(crate) fn derive_scannable_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let input = input.clone();

    let args = match ScannableInput::from_derive_input(&input) {
        Ok(a) => a,
        Err(e) => return e.write_errors(),
    };

    let ident = &args.ident;
    let mut generics = args.generics.clone();

    let mut history = HashSet::new();

    let fields = match args.data {
        ast::Data::Struct(s) => s.fields,
        _ => unreachable!("supports(struct_named) guarantees named struct"),
    };

    // --- 1) Where bounds for Default + Scannable ---

    {
        let wc = generics.where_clause.get_or_insert(syn::WhereClause {
            where_token: <syn::Token![where]>::default(),
            predicates: syn::punctuated::Punctuated::new(),
        });

        for f in &fields {
            let ty = &f.ty;

            // skipped fields must be Default
            if f.skip || !f.selectable.unwrap_or(true) {
                if history.insert(ty.clone()) == false {
                    continue;
                }
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::core::default::Default
                });
            }

            if f.json {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::serde::de::DeserializeOwned
                });
            }

            // flattened fields must be Scannable
            if f.flatten {
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::uxar::db::Scannable
                });
            }
        }
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // --- 2) Field initializers for scan_row_ordered ---

    let field_inits = fields.iter().map(|f| {
        let ident = f.ident.as_ref().expect("named field");
        let ty = &f.ty;

        if f.skip || !f.selectable.unwrap_or(true) {
            // compile-time enforced Default
            quote! {
                #ident: <#ty as ::core::default::Default>::default()
            }
        } else if f.flatten {
            // nested struct; must also implement Scannable
            quote! {
                #ident: <#ty as ::uxar::db::Scannable>::scan_row_ordered(row, start_idx)?
            }
        } else if f.json {
            quote! {
                #ident: {
                    // decode the column as JSON value from postgres
                    let value: ::serde_json::Value =
                        ::sqlx::Row::try_get::<::serde_json::Value, _>(row, *start_idx)?;
                    *start_idx += 1;

                    ::serde_json::from_value::<#ty>(value)
                        .map_err(|e| ::sqlx::Error::Decode(Box::new(e)))?
                }
            }
        } else {
            // scalar field; sequential scan using sqlx
            quote! {
                #ident: {
                    let value = ::sqlx::Row::try_get::<#ty, _>(row, *start_idx)?;
                    *start_idx += 1;
                    value
                }
            }
        }
    });

    let expanded = quote! {
        impl #impl_generics ::uxar::db::Scannable for #ident #ty_generics #where_clause {
            fn scan_row_ordered(
                row: &::uxar::db::PgRow,
                start_idx: &mut usize,
            ) -> Result<Self, ::uxar::db::SqlxError> {
                Ok(Self {
                    #(#field_inits),*
                })
            }
        }

        impl<'r> ::uxar::db::FromRow<'r, ::uxar::db::PgRow> for #ident #ty_generics #where_clause {
            fn from_row(row: &'r ::uxar::db::PgRow) -> Result<Self, ::uxar::db::SqlxError> {
                <Self as ::uxar::db::Scannable>::scan_row(row)
            }
        }
    };

    expanded
}
