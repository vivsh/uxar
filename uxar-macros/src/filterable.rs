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

    let field_stmts: Vec<_> = fields.iter().enumerate().map(|(idx, f)| {
        let ident = f.ident.as_ref().expect("named field");
        let ty = &f.ty;

        // Skip explicitly
        if f.skip {
            return quote! {};
        }

        // Flatten: delegate to nested Filterable
        if f.delegate {
            let (is_option, _inner_ty) = option_inner_type(ty);

            if is_option {
                quote! {
                    if let Some(inner) = self.#ident {
                        qs = ::uxar::db::Filterable::apply_filters_impl(inner, qs);
                    }
                }
            } else {
                quote! {
                    qs = ::uxar::db::Filterable::apply_filters_impl(self.#ident, qs);
                }
            }
        } else {
            // Normal scalar field — use named placeholder to avoid positional ambiguity
            let col_name = f.db_column.clone().unwrap_or_else(|| ident.to_string());
            let expr = f.expr.clone().unwrap_or_else(|| col_name);
            let op = f.op.clone().unwrap_or_else(|| "=".to_string());
            // Unique placeholder name: _ff_<idx>
            let param_name = format!("_ff_{}", idx);
            let filter_str = format!("{expr} {op} :{param_name}");

            let (is_option, _inner_ty) = option_inner_type(ty);

            if is_option {
                quote! {
                    if let Some(value) = self.#ident {
                        qs = ::uxar::db::FilteredBuilder::filter(qs, #filter_str);
                        qs = ::uxar::db::FilteredBuilder::bind_named_dyn(
                            qs,
                            #param_name,
                            ::uxar::db::ArgValue::new(value),
                        );
                    }
                }
            } else {
                quote! {
                    qs = ::uxar::db::FilteredBuilder::filter(qs, #filter_str);
                    qs = ::uxar::db::FilteredBuilder::bind_named_dyn(
                        qs,
                        #param_name,
                        ::uxar::db::ArgValue::new(self.#ident),
                    );
                }
            }
        }
    }).collect();

    let after = if let Some(after_path) = args.after {
        quote! {
            qs = #after_path(self, qs);
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #impl_generics ::uxar::db::Filterable for #ident #ty_generics #where_clause {
            fn apply_filters_select(
                self,
                mut qs: ::uxar::db::SelectFrom,
            ) -> ::uxar::db::SelectFrom {
                #(#field_stmts)*
                #after
                qs
            }

            fn apply_filters_update(
                self,
                mut qs: ::uxar::db::UpdateTable,
            ) -> ::uxar::db::UpdateTable {
                #(#field_stmts)*
                #after
                qs
            }

            fn apply_filters_delete(
                self,
                mut qs: ::uxar::db::DeleteFrom,
            ) -> ::uxar::db::DeleteFrom {
                #(#field_stmts)*
                #after
                qs
            }
        }
    };

    expanded.into()
}

/// Helper to detect Option<T> and extract T
/// Handles: Option, std::option::Option, core::option::Option, ::std::option::Option
fn option_inner_type(ty: &Type) -> (bool, Option<Type>) {
    if let Type::Path(type_path) = ty {
        // Check if this is an Option type by examining the path
        let path = &type_path.path;
        
        // Get the last segment (should be "Option")
        if let Some(last_seg) = path.segments.last() {
            if last_seg.ident != "Option" {
                return (false, None);
            }
            
            // Check if the path is just "Option" or a known Option path
            let is_option = if path.segments.len() == 1 {
                // Just "Option"
                true
            } else if path.segments.len() == 3 {
                // std::option::Option or core::option::Option
                let first = path.segments.first().unwrap();
                let second = path.segments.iter().nth(1).unwrap();
                (first.ident == "std" || first.ident == "core") && second.ident == "option"
            } else if path.segments.len() == 4 && path.leading_colon.is_some() {
                // ::std::option::Option or ::core::option::Option
                let first = path.segments.first().unwrap();
                let second = path.segments.iter().nth(1).unwrap();
                let third = path.segments.iter().nth(2).unwrap();
                (first.ident == "std" || first.ident == "core") && second.ident == "option" && third.ident == "Option"
            } else {
                false
            };
            
            if !is_option {
                return (false, None);
            }
            
            // Extract inner type from angle brackets
            if let syn::PathArguments::AngleBracketed(ab) = &last_seg.arguments {
                if let Some(syn::GenericArgument::Type(inner_ty)) = ab.args.first() {
                    return (true, Some(inner_ty.clone()));
                }
            }
            
            return (true, None);
        }
    }
    (false, None)
}