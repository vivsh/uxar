use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Expr, Fields, Lit, Type, parse_macro_input};

pub fn derive_multipart_data(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = input.ident;
    let fields = match input.data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => fields.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    ident,
                    "MultipartData can only be derived for structs with named fields",
                ));
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                ident,
                "MultipartData can only be derived for structs",
            ));
        }
    };

    let mut spec_parts = Vec::new();
    let mut initializers = Vec::new();

    for field in fields {
        let field_ident = field.ident.clone().expect("named field");
        let field_name = field_ident.to_string();
        let upload = UploadAttrs::parse(&field)?;

        if type_ends_with(&field.ty, "UploadedFile") {
            let content_types = upload.content_types;
            let extensions = upload.extensions;
            let max_size = upload.max_size;
            let sniff = upload.sniff;

            let mut rule = quote! { ::vyuh::routes::multipart::FileRule::new().required() };
            if !content_types.is_empty() {
                rule = quote! { #rule.content_types([#(#content_types),*]) };
            }
            if !extensions.is_empty() {
                rule = quote! { #rule.extensions([#(#extensions),*]) };
            }
            if let Some(max_size) = max_size {
                rule = quote! { #rule.max_size(#max_size) };
            }
            if let Some(sniff) = sniff {
                if sniff == "image" {
                    rule = quote! { #rule.sniff_image() };
                } else {
                    return Err(syn::Error::new_spanned(
                        field_ident,
                        "unsupported upload sniff rule; expected sniff = \"image\"",
                    ));
                }
            }

            spec_parts.push(quote! {
                spec = spec.file(#field_name, #rule);
            });
            initializers.push(quote! {
                #field_ident: map.file(#field_name)?.clone()
            });
        } else if type_ends_with(&field.ty, "String") {
            let rule = quote! { ::vyuh::routes::multipart::FieldRule::new().required() };
            spec_parts.push(quote! {
                spec = spec.text(#field_name, #rule);
            });
            initializers.push(quote! {
                #field_ident: map.text(#field_name)?.to_string()
            });
        } else {
            return Err(syn::Error::new_spanned(
                field.ty,
                "MultipartData derive supports String and UploadedFile fields in this pass",
            ));
        }
    }

    Ok(quote! {
        impl ::vyuh::routes::multipart::MultipartData for #ident {
            fn multipart_spec() -> ::vyuh::routes::multipart::MultipartSpec {
                let mut spec = ::vyuh::routes::multipart::MultipartSpec::new();
                #(#spec_parts)*
                spec
            }

            fn from_multipart(
                map: ::vyuh::routes::multipart::MultipartMap
            ) -> ::std::result::Result<Self, ::vyuh::routes::multipart::MultipartError> {
                Ok(Self {
                    #(#initializers),*
                })
            }
        }
    })
}

#[derive(Default)]
struct UploadAttrs {
    content_types: Vec<String>,
    extensions: Vec<String>,
    sniff: Option<String>,
    max_size: Option<u64>,
}

impl UploadAttrs {
    fn parse(field: &syn::Field) -> syn::Result<Self> {
        let mut attrs = Self::default();
        for attr in &field.attrs {
            if !attr.path().is_ident("upload") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("content_types") {
                    attrs.content_types = parse_string_values(meta.value()?.parse()?)?;
                } else if meta.path.is_ident("extensions") {
                    attrs.extensions = parse_string_values(meta.value()?.parse()?)?;
                } else if meta.path.is_ident("sniff") {
                    let value: Lit = meta.value()?.parse()?;
                    attrs.sniff = Some(match value {
                        Lit::Str(value) => value.value(),
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "sniff must be a string literal",
                            ));
                        }
                    });
                } else if meta.path.is_ident("max_size") {
                    let value: Lit = meta.value()?.parse()?;
                    attrs.max_size = Some(match value {
                        Lit::Int(value) => value.base10_parse()?,
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "max_size must be an integer literal",
                            ));
                        }
                    });
                } else {
                    return Err(meta.error("unsupported upload attribute"));
                }
                Ok(())
            })?;
        }
        Ok(attrs)
    }
}

fn parse_string_values(expr: Expr) -> syn::Result<Vec<String>> {
    match expr {
        Expr::Array(array) => array
            .elems
            .into_iter()
            .map(|expr| match expr {
                Expr::Lit(expr) => match expr.lit {
                    Lit::Str(value) => Ok(value.value()),
                    other => Err(syn::Error::new_spanned(other, "expected string literal")),
                },
                other => Err(syn::Error::new_spanned(other, "expected string literal")),
            })
            .collect(),
        Expr::Lit(expr) => match expr.lit {
            Lit::Str(value) => Ok(vec![value.value()]),
            other => Err(syn::Error::new_spanned(other, "expected string literal")),
        },
        other => Err(syn::Error::new_spanned(
            other,
            "expected string literal or array of string literals",
        )),
    }
}

fn type_ends_with(ty: &Type, ident: &str) -> bool {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == ident),
        _ => false,
    }
}
