use std::collections::HashSet;
use syn::{Type, Generics, WhereClause, WherePredicate};

/// Helper for building where clauses with deduplication
pub struct WhereClauseBuilder<'a> {
    generics: &'a mut Generics,
    seen_types: HashSet<String>,
}

impl<'a> WhereClauseBuilder<'a> {
    pub fn new(generics: &'a mut Generics) -> Self {
        Self {
            generics,
            seen_types: HashSet::new(),
        }
    }

    /// Get or create the where clause
    fn get_where_clause(&mut self) -> &mut WhereClause {
        self.generics.where_clause.get_or_insert_with(|| WhereClause {
            where_token: <syn::Token![where]>::default(),
            predicates: syn::punctuated::Punctuated::new(),
        })
    }

    /// Add a where predicate, deduplicated by type
    pub fn add_bound(&mut self, ty: &Type, predicate: WherePredicate) {
        let ty_string = quote::quote!(#ty).to_string();
        if self.seen_types.insert(ty_string) {
            self.get_where_clause().predicates.push(predicate);
        }
    }

    /// Add Default bound for a type
    pub fn add_default_bound(&mut self, ty: &Type) {
        self.add_bound(ty, syn::parse_quote! {
            #ty: ::core::default::Default
        });
    }

    /// Add sqlx Encode + Type bounds for a type
    pub fn add_sqlx_encode_bound(&mut self, ty: &Type) {
        let ty_string = quote::quote!(#ty).to_string();
        if self.seen_types.insert(ty_string) {
            self.get_where_clause().predicates.push(syn::parse_quote! {
                for<'q> &'q #ty: ::sqlx::Encode<'q, ::sqlx::Postgres>
                    + ::sqlx::Type<::sqlx::Postgres>
                    + ::core::marker::Send
            });
        }
    }

    /// Add serde Serialize bound for a type
    pub fn add_serialize_bound(&mut self, ty: &Type) {
        self.add_bound(ty, syn::parse_quote! {
            #ty: ::serde::Serialize
        });
    }

    /// Add serde Deserialize bound for a type
    pub fn add_deserialize_bound(&mut self, ty: &Type) {
        self.add_bound(ty, syn::parse_quote! {
            #ty: ::serde::de::DeserializeOwned
        });
    }

    /// Add Bindable bound for a type (for flatten)
    pub fn add_bindable_bound(&mut self, ty: &Type) {
        self.add_bound(ty, syn::parse_quote! {
            #ty: ::uxar::db::Bindable
        });
    }

    /// Add Scannable bound for a type (for flatten)
    pub fn add_scannable_bound(&mut self, ty: &Type) {
        self.add_bound(ty, syn::parse_quote! {
            #ty: ::uxar::db::Scannable
        });
    }
}
