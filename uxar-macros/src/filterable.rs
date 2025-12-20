use proc_macro::TokenStream;
use quote::{quote};
use syn::{parse_macro_input, DeriveInput, Ident, Type, Path};
use darling::{FromDeriveInput, FromField, ast};

#[derive(Debug, FromField)]
#[darling(attributes(filter))]
struct FilterField {
    /// Field identifier
    ident: Option<Ident>,

    /// Field type
    ty: Type,

    /// Optional explicit column name (defaults to field name)
    #[darling(default)]
    db_column: Option<String>,

    /// Optional raw SQL expression to use instead of column
    /// e.g. "LOWER(username)"
    #[darling(default)]
    expr: Option<String>,

    /// Optional operator (e.g. "=", ">", "<=", "ILIKE")
    /// Default is "="
    #[darling(default)]
    op: Option<String>,

    /// If true, delegate filtering to nested Filterable type
    #[darling(default)]
    delegate: bool,

    /// If true, ignore this field for filtering
    #[darling(default)]
    skip: bool,
}

#[derive(Debug, FromDeriveInput)]
#[darling(supports(struct_named), attributes(filterable))]
struct FilterableInput {
    ident: Ident,
    generics: syn::Generics,

    /// Optional struct-level delegate:
    /// #[filterable(after = "path::to::fn")]
    #[darling(default)]
    after: Option<Path>,

    data: ast::Data<(), FilterField>,
}

pub fn derive_filterable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let args = match FilterableInput::from_derive_input(&input) {
        Ok(a) => a,
        Err(e) => return e.write_errors().into(),
    };

    let ident = &args.ident;
    let generics = args.generics.clone();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields = match args.data {
        ast::Data::Struct(s) => s.fields,
        _ => unreachable!("Filterable only supports named structs"),
    };

    let field_stmts = fields.iter().map(|f| {
        let ident = f.ident.as_ref().expect("named field");
        let ty = &f.ty;

        // Skip explicitly
        if f.skip {
            return quote! {};
        }

        // Flatten: delegate to nested Filterable
        if f.delegate {
            let (is_option, inner_ty) = option_inner_type(ty);

            if is_option {
                // Option<Inner: Filterable>
                quote! {
                    if let Some(inner) = self.#ident.as_ref() {
                        qs = ::uxar::db::Filterable::filter_query(inner, qs);
                    }
                }
            } else {
                // Inner: Filterable
                quote! {
                    qs = ::uxar::db::Filterable::filter_query(&self.#ident, qs);
                }
            }
        } else {
            // Normal scalar field
            let col_name = f.db_column.clone().unwrap_or_else(|| ident.to_string());
            let expr = f.expr.clone().unwrap_or_else(|| col_name);
            let op = f.op.clone().unwrap_or_else(|| "=".to_string());

            let filter_str = format!("{expr} {op} ?");

            let (is_option, _inner_ty) = option_inner_type(ty);

            if is_option {
                // Option<T>: filter only when Some
                quote! {
                    if let Some(value) = self.#ident.as_ref() {
                        qs = qs.filter(#filter_str).bind(value);
                    }
                }
            } else {
                // Plain T: always filter
                quote! {
                    qs = qs.filter(#filter_str).bind(&self.#ident);
                }
            }
        }
    });

    let after = if let Some(after_path) = args.after {
        quote! {
            qs = #after_path(self, qs);
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #impl_generics ::uxar::db::Filterable for #ident #ty_generics #where_clause {
            fn filter_query(&self, mut qs: ::uxar::db::Query) -> ::uxar::db::Query {
                #(#field_stmts)*
                #after
                qs
            }
        }
    };

    expanded.into()
}

/// Helper to detect Option<T> and extract T
fn option_inner_type(ty: &Type) -> (bool, Option<Type>) {
    if let Type::Path(type_path) = ty {
        if let Some(seg) = type_path.path.segments.last() {
            if seg.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(ab) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = ab.args.first() {
                        return (true, Some(inner_ty.clone()));
                    }
                }
                return (true, None);
            }
        }
    }
    (false, None)
}