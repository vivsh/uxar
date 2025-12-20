use std::collections::HashSet;

use darling::FromDeriveInput;
use darling::ast::{self, Data};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

use crate::schemable::SchemableField;

#[derive(FromDeriveInput)]
pub(crate) struct BindableInput {
    ident: syn::Ident,
    generics: syn::Generics,

    #[darling(default, rename = "crate")]
    crate_path: Option<syn::Path>,

    data: Data<darling::util::Ignored, SchemableField>,
}

pub fn derive_bindable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match BindableInput::from_derive_input(&input) {
        Ok(a) => a,
        Err(e) => return e.write_errors().into(),
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

            if f.skip {
                continue;
            }

            if f.flatten {
                if history.insert(ty.clone()) == false {
                    continue;
                }
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::uxar::db::Bindable
                });
                continue;
            }

            if f.json {
                if history.insert(ty.clone()) == false {
                    continue;
                }
                wc.predicates.push(syn::parse_quote! {
                    #ty: ::serde::Serialize
                });
            } else {
                wc.predicates.push(syn::parse_quote! {
                    for<'q> &'q #ty: ::sqlx::Encode<'q, ::sqlx::Postgres>
                        + ::sqlx::Type<::sqlx::Postgres>
                        + ::core::marker::Send
                });
            }
        }
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let bind_stmts = fields.iter().map(|f| {
        let ident = f.ident.as_ref().expect("named field");
        let ty = &f.ty;

        if f.skip {
            // do nothing
            quote! {}
        } else if f.flatten {
            // delegate to nested Bindable
            quote! {
                <#ty as ::uxar::db::Bindable>::bind_values(&self.#ident, args)?;
            }
        } else if f.json {
            // serialize to serde_json::Value and bind as JSON
            quote! {
                {
                    let value = ::serde_json::to_value(&self.#ident)
                        .map_err(|e| ::sqlx::Error::Decode(Box::new(e)))?;
                    <::sqlx::postgres::PgArguments as ::sqlx::Arguments<'_>>::add(args, value)
                        .map_err(::sqlx::Error::Decode)?;
                }
            }
        } else {
            // scalar field
            quote! {
                {
                    <::sqlx::postgres::PgArguments as ::sqlx::Arguments<'_>>::add(args, &self.#ident)
                        .map_err(::sqlx::Error::Decode)?;
                }
            }
        }
    });

    let expanded = quote! {
        impl #impl_generics ::uxar::db::Bindable for #ident #ty_generics #where_clause {
            fn bind_values(
                &self,
                args: &mut ::sqlx::postgres::PgArguments,
            ) -> Result<(), ::sqlx::Error> {
                #(#bind_stmts)*
                Ok(())
            }
        }
    };

    expanded.into()
}
