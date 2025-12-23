use darling::ast::Data;
use proc_macro::TokenStream;
use quote::quote;
use darling::{FromDeriveInput, FromField, FromMeta};
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

#[derive(Default)]
struct ValidationAttrs {
    email: bool,
    url: bool,
    min_length: Option<usize>,
    max_length: Option<usize>,
    exact_length: Option<usize>,
    min_value: Option<i64>,
    max_value: Option<i64>,
    range: Option<(i64, i64)>,
    regex: Option<String>,
    non_empty: bool,
    alphanumeric: bool,
    slug: bool,
    digits: bool,
    uuid: bool,
    ipv4: bool,
}

impl FromMeta for ValidationAttrs {
    fn from_list(items: &[darling::ast::NestedMeta]) -> darling::Result<Self> {
        use darling::ast::NestedMeta;
        use syn::{Meta, Expr, ExprLit, Lit, ExprTuple};
        
        let mut result = Self::default();
        
        for item in items {
            match item {
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("email") => {
                    result.email = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("url") => {
                    result.url = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("non_empty") => {
                    result.non_empty = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("alphanumeric") => {
                    result.alphanumeric = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("slug") => {
                    result.slug = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("digits") => {
                    result.digits = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("uuid") => {
                    result.uuid = true;
                }
                NestedMeta::Meta(Meta::Path(path)) if path.is_ident("ipv4") => {
                    result.ipv4 = true;
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("min_length") => {
                    if let Expr::Lit(ExprLit { lit: Lit::Int(lit), .. }) = &nv.value {
                        result.min_length = Some(lit.base10_parse()?);
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("max_length") => {
                    if let Expr::Lit(ExprLit { lit: Lit::Int(lit), .. }) = &nv.value {
                        result.max_length = Some(lit.base10_parse()?);
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("exact_length") => {
                    if let Expr::Lit(ExprLit { lit: Lit::Int(lit), .. }) = &nv.value {
                        result.exact_length = Some(lit.base10_parse()?);
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("min_value") => {
                    result.min_value = Some(parse_i64_expr(&nv.value)?);
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("max_value") => {
                    result.max_value = Some(parse_i64_expr(&nv.value)?);
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("range") => {
                    if let Expr::Tuple(ExprTuple { elems, .. }) = &nv.value {
                        if elems.len() == 2 {
                            let first = parse_i64_expr(&elems[0])?;
                            let second = parse_i64_expr(&elems[1])?;
                            result.range = Some((first, second));
                        }
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("regex") => {
                    if let Expr::Lit(ExprLit { lit: Lit::Str(lit), .. }) = &nv.value {
                        result.regex = Some(lit.value());
                    }
                }
                _ => {}
            }
        }
        
        Ok(result)
    }
}

fn parse_i64_expr(expr: &syn::Expr) -> darling::Result<i64> {
    use syn::{Expr, ExprLit, Lit, ExprUnary, UnOp};
    
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Int(lit_int), .. }) => {
            lit_int.base10_parse().map_err(|e| darling::Error::custom(format!("Invalid integer: {}", e)))
        }
        Expr::Unary(ExprUnary { op: UnOp::Neg(_), expr, .. }) => {
            let val = parse_i64_expr(expr)?;
            Ok(-val)
        }
        _ => Err(darling::Error::custom("Expected integer literal")),
    }
}

impl ValidationAttrs {
    fn has_any(&self) -> bool {
        self.email
            || self.url
            || self.min_length.is_some()
            || self.max_length.is_some()
            || self.exact_length.is_some()
            || self.min_value.is_some()
            || self.max_value.is_some()
            || self.range.is_some()
            || self.regex.is_some()
            || self.non_empty
            || self.alphanumeric
            || self.slug
            || self.digits
            || self.uuid
            || self.ipv4
    }
}

fn extract_validation(attrs: &[syn::Attribute]) -> Option<ValidationAttrs> {
    attrs
        .iter()
        .find(|attr| attr.path().is_ident("validate"))
        .and_then(|attr| {
            ValidationAttrs::from_meta(&attr.meta)
                .ok()
                .filter(|v| v.has_any())
        })
}



#[derive(FromDeriveInput)]
#[darling(attributes(schemable))]
pub(crate) struct SchemableInput {
    ident: syn::Ident,
    generics: syn::Generics,
    
    #[darling(default, rename = "crate")]
    crate_path: Option<syn::Path>,

    #[darling(default)]
    name: Option<String>,

    data: Data<darling::util::Ignored, SchemableField>,
}

pub (crate) fn derive_schemable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    derive_schemable_impl(&input).into()
}

pub(crate) fn derive_schemable_impl(input: &DeriveInput) -> proc_macro2::TokenStream {
    let input_clone = input.clone();

    let schemable = match SchemableInput::from_derive_input(&input_clone) {
        Ok(a) => a,
        Err(e) => return e.write_errors(),
    };
    impl_schemable(schemable, input_clone)
}



pub(crate) fn impl_schemable(input: SchemableInput, original_input: DeriveInput) -> proc_macro2::TokenStream {
    let ident = input.ident;
    let generics = input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Determine crate path (where Schemable & ColumnSpec live)
    let crate_path: syn::Path = input
        .crate_path
        .unwrap_or_else(|| syn::parse_quote!(uxar::db));

    // Collect columns from darling-parsed data
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

    // Get raw fields from original input for validation parsing
    let raw_fields = match original_input.data {
        syn::Data::Struct(s) => match s.fields {
            syn::Fields::Named(f) => f.named.into_iter().collect::<Vec<_>>(),
            _ => {
                return syn::Error::new_spanned(
                    ident,
                    "Schemable only supports structs with named fields",
                )
                .into_compile_error();
            }
        },
        _ => {
            return syn::Error::new_spanned(
                ident,
                "Schemable only supports structs",
            )
            .into_compile_error();
        }
    };

    // Determine the schema name (custom or default to struct name)
    let schema_name = input
        .name
        .unwrap_or_else(|| ident.to_string());

    let mut column_specs = Vec::new();

    for (idx, field) in fields.iter().enumerate() {
        let field_ident = field.ident.as_ref().expect("named fields only");
        let field_name_literal = field_ident.to_string();
        let ty = &field.ty;

        let db_column = field
            .db_column
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or_else(|| field_name_literal.as_str());

        // Decide ColumnKind expression
        let kind_expr = if field.flatten {
            quote! { ::#crate_path::ColumnKind::Flatten { columns: <#ty>::SCHEMA } }
        } else if field.reference {
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

        // Extract validation from raw field attributes
        let raw_field = &raw_fields[idx];
        let validation_expr = if let Some(validation) = extract_validation(&raw_field.attrs) {
            let email = validation.email;
            let url = validation.url;
            let min_length = match validation.min_length {
                Some(v) => quote! { Some(#v) },
                None => quote! { None },
            };
            let max_length = match validation.max_length {
                Some(v) => quote! { Some(#v) },
                None => quote! { None },
            };
            let exact_length = match validation.exact_length {
                Some(v) => quote! { Some(#v) },
                None => quote! { None },
            };
            let min_value = match validation.min_value {
                Some(v) => quote! { Some(#v) },
                None => quote! { None },
            };
            let max_value = match validation.max_value {
                Some(v) => quote! { Some(#v) },
                None => quote! { None },
            };
            let range = match validation.range {
                Some((a, b)) => quote! { Some((#a, #b)) },
                None => quote! { None },
            };
            let regex = match validation.regex {
                Some(ref s) => quote! { Some(#s) },
                None => quote! { None },
            };
            let non_empty = validation.non_empty;
            let alphanumeric = validation.alphanumeric;
            let slug = validation.slug;
            let digits = validation.digits;
            let uuid = validation.uuid;
            let ipv4 = validation.ipv4;
            
            quote! {
                Some(::#crate_path::ColumnValidation {
                    email: #email,
                    url: #url,
                    min_length: #min_length,
                    max_length: #max_length,
                    exact_length: #exact_length,
                    min_value: #min_value,
                    max_value: #max_value,
                    range: #range,
                    regex: #regex,
                    non_empty: #non_empty,
                    alphanumeric: #alphanumeric,
                    slug: #slug,
                    digits: #digits,
                    uuid: #uuid,
                    ipv4: #ipv4,
                })
            }
        } else {
            quote! { None }
        };

        column_specs.push(quote! {
            ::#crate_path::ColumnSpec {
                kind: #kind_expr,
                name: #field_name_literal,
                db_column: #db_column,
                selectable: #selectable,
                insertable: #insertable,
                updatable: #updatable,
                validation: #validation_expr,
            }
        });
    }


    let expanded = quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            pub const SCHEMA: &'static [::#crate_path::ColumnSpec] = &[
                #(#column_specs),*
            ];
            pub const NAME: &'static str = #schema_name;
        }

        impl #impl_generics ::#crate_path::SchemaInfo for #ident #ty_generics #where_clause {
            fn schema() -> &'static [::#crate_path::ColumnSpec] {
                Self::SCHEMA
            }
            fn name() -> &'static str {
                Self::NAME
            }
        }
    };

    expanded.into()

}