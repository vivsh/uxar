use darling::ast::Data;
use proc_macro::TokenStream;
use quote::quote;
use darling::{FromDeriveInput, FromField};
use syn::{DeriveInput, parse_macro_input};



#[derive(FromField)]
#[darling(attributes(column))]
pub(crate) struct SchemableField {
    /// The field identifier (filled by darling)
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,

    /// Optional column name override: #[schemable(column = "my_col")]
    #[darling(default)]
    pub db_column: Option<String>,

    /// Skip this field in schema: #[schemable(skip)]
    #[darling(default)]
    pub skip: bool,

    #[darling(default)]
    pub flatten: bool,

    #[darling(default)]
    pub json: bool,

    #[darling(default)]
    pub reference: bool,

    /// Whether this column is selectable
    #[darling(default)]
    pub selectable: Option<bool>,

    /// Whether this column is insertable
    #[darling(default)]
    pub insertable: Option<bool>,

    /// Whether this column is updatable
    #[darling(default)]
    pub updatable: Option<bool>,
}



#[derive(FromDeriveInput)]
pub(crate) struct SchemableInput {
    ident: syn::Ident,
    generics: syn::Generics,
    
    #[darling(default, rename = "crate")]
    crate_path: Option<syn::Path>,

    data: Data<darling::util::Ignored, SchemableField>,
}

pub (crate) fn derive_schemable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let schemable = match SchemableInput::from_derive_input(&input) {
        Ok(a) => a,
        Err(e) => return e.write_errors().into(),
    };
    impl_schemable(schemable).into()
}

// pub(crate) struct ColumnSpecGen{
//     pub kind: proc_macro2::TokenStream,
//     pub name: String,
//     pub type_expr: proc_macro2::TokenStream,
//     pub db_column: String,
//     pub selectable: bool,
//     pub insertable: bool,
//     pub updatable: bool,
// }

// pub (crate) fn parse_column_specs() -> Result<Vec<ColumnSpecGen>, syn::Error> {
//     Ok(vec![])

// }

pub(crate) fn impl_schemable(input: SchemableInput) -> proc_macro2::TokenStream {
    let ident = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Determine crate path (where Schemable & ColumnSpec live)
    // Defaults to `crate::Schemable`
    let crate_path: syn::Path = input
        .crate_path
        .unwrap_or_else(|| syn::parse_quote!(uxar::db));

    // Collect columns
    let fields = match input.data {
        Data::Struct(s) => s.fields,
        _ => {
            return syn::Error::new_spanned(
                ident,
                "Schemable only supports structs with named fields",
            )
            .into_compile_error();
        }
    };

    let mut column_specs = Vec::new();

    for field in fields {
        let field_ident = field.ident.as_ref().expect("named fields only");
        let field_name_literal = field_ident.to_string();
        let ty = field.ty;

        let db_column = field
            .db_column
            .unwrap_or_else(|| field_ident.to_string());

          // Decide ColumnKind expression
        let kind_expr = if field.flatten {
            // Embedded struct flattened into same row
            quote! { ::#crate_path::ColumnKind::Flatten { columns: <#ty>::SCHEMA } }
        } else if field.reference {
            // Related struct; not used in flat select/insert/update v1
            quote! { ::#crate_path::ColumnKind::Reference { columns: <#ty>::SCHEMA } }
        } else if field.json {
            quote! { ::#crate_path::ColumnKind::Json }
        } else {
            quote! { ::#crate_path::ColumnKind::Scalar }
        };

        // Flags: default true, then override, then apply skip
        let mut selectable = true;
        let mut insertable = true;
        let mut updatable = true;

        if let Some(v) = field.selectable {
            selectable = v;
        }
        if let Some(v) = field.insertable {
            insertable = v;
        }
        if let Some(v) = field.updatable {
            updatable = v;
        }
        if field.skip {
            selectable = false;
            insertable = false;
            updatable = false;
        }

        column_specs.push(quote! {
            ::#crate_path::ColumnSpec {
                kind: #kind_expr,
                name: #field_name_literal,
                db_column: #db_column,
                selectable: #selectable,
                insertable: #insertable,
                updatable: #updatable,
            }
        });
    }


    let expanded = quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub const SCHEMA: &'static [::#crate_path::ColumnSpec] = &[
                #(#column_specs),*
            ];
        }

        impl #impl_generics ::#crate_path::Schemable for #ident #ty_generics #where_clause {
            fn schema() -> &'static [::#crate_path::ColumnSpec] {
                Self::SCHEMA
            }
        }
    };

    expanded.into()

}